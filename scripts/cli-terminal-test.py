"""Real POSIX terminal/pipe boundary tests; all inputs are synthetic and defined here."""
import errno
import json
import os
from pathlib import Path
import pty
import select
import signal
import subprocess
import termios
import time

ROOT = Path(__file__).resolve().parent.parent
BINARY = str(Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "debug/darkhorse-server")
ENV = {"PATH": os.environ.get("PATH", ""), "DARKHORSE_DATABASE_URL": "invalid-private-url"}
if "LLVM_PROFILE_FILE" in os.environ:
    ENV["LLVM_PROFILE_FILE"] = os.environ["LLVM_PROFILE_FILE"]
SECRET = b"synthetic-only-terminal-passphrase"


class Terminal:
    def __init__(self, args, background=False, separate_stderr=False):
        error_read, error_write = os.pipe() if separate_stderr else (None, None)
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            if separate_stderr:
                os.close(error_read)
                os.dup2(error_write, 2)
                os.close(error_write)
            if background:
                result = subprocess.run([BINARY, *args], env=ENV, preexec_fn=os.setpgrp, timeout=5)
                os._exit(result.returncode)
            os.execve(BINARY, [BINARY, *args], ENV)
        if separate_stderr:
            os.close(error_write)
        self.error_fd = error_read
        self.data = b""
        self.status = None
        self.original = termios.tcgetattr(self.fd)
        self.deadline = time.monotonic() + 10

    def read(self):
        assert time.monotonic() < self.deadline, "terminal test exceeded deadline"
        descriptors = ([] if self.fd is None else [self.fd]) + ([] if self.error_fd is None else [self.error_fd])
        for descriptor in select.select(descriptors, [], [], 0.05)[0]:
            try:
                self.data += os.read(descriptor, 4096)
            except OSError as error:
                if error.errno != errno.EIO:
                    raise
        assert len(self.data) < 65536, "terminal output exceeded bound"

    def expect(self, text):
        while text not in self.data:
            self.read()

    def send(self, value):
        os.write(self.fd, value)

    def hidden(self, prompt, value):
        self.expect(prompt)
        # Synchronize with the terminal's echo state, not a sleep after the prompt.
        while termios.tcgetattr(self.fd)[3] & termios.ECHO:
            self.read()
        self.send(value)

    def finish(self):
        while self.status is None:
            self.read()
            pid, status = os.waitpid(self.pid, os.WNOHANG)
            if pid:
                self.status = status
        self.read()
        assert SECRET not in self.data, "terminal exposed a password"
        assert b"panicked" not in self.data
        return os.waitstatus_to_exitcode(self.status)

    def close(self):
        if self.status is None:
            try:
                os.kill(self.pid, signal.SIGKILL)
                os.waitpid(self.pid, 0)
            except ProcessLookupError:
                pass
        if self.fd is not None:
            os.close(self.fd)
        if self.error_fd is not None:
            os.close(self.error_fd)


def bootstrap(until):
    terminal = Terminal(["bootstrap", "--yes"])
    try:
        for prompt, value in [(b"Email: ", b"terminal@example.com\n"), (b"First name: ", b"Terminal\n"), (b"Last name: ", b"Test\n")]:
            terminal.expect(prompt)
            terminal.send(value)
        if until == "complete":
            terminal.hidden(b"Password: ", SECRET + b"\n")
            terminal.hidden(b"Confirm password: ", SECRET + b"\n")
            assert terminal.finish() == 1  # Invalid configuration, after secret collection.
            assert b"Invalid database configuration" in terminal.data
        else:
            terminal.hidden(b"Password: ", SECRET + (b"\x15\x04" if until == "eof" else b"\x03"))
            result = terminal.finish()
            assert result == 1
        assert termios.tcgetattr(terminal.fd) == terminal.original, "terminal modes were not restored"
    finally:
        terminal.close()


def password_boundary(scenario, at_confirmation=False):
    terminal = Terminal(["operator", "bootstrap", "--yes"])
    try:
        for prompt, value in [(b"Email: ", b"terminal@example.com\n"), (b"First name: ", b"Terminal\n"), (b"Last name: ", b"Test\n")]:
            terminal.expect(prompt)
            terminal.send(value)
        prompt = b"Password: "
        if at_confirmation:
            terminal.hidden(prompt, SECRET + b"\n")
            prompt = b"Confirm password: "
        if isinstance(scenario, int):
            terminal.hidden(prompt, SECRET)
            os.kill(terminal.pid, scenario)
            expected = b"Operator command interrupted"
        elif scenario == "oversized":
            terminal.hidden(prompt, SECRET + b"x" * (1025 - len(SECRET)))
            expected = b"Terminal input is too long"
        elif scenario == "eof":
            terminal.hidden(prompt, SECRET + b"\x04")
            expected = b"Terminal input cancelled"
        else:
            suffix = b"\x1b[31m" if scenario == "escape" else b"\xff\n"
            terminal.hidden(prompt, SECRET + suffix)
            expected = b"Invalid terminal input"
        assert terminal.finish() == 1
        assert expected in terminal.data, "wrong redacted failure category"
        assert b"Invalid database configuration" not in terminal.data
        assert termios.tcgetattr(terminal.fd) == terminal.original, "terminal modes were not restored"
    finally:
        terminal.close()


def edited_password():
    terminal = Terminal(["bootstrap", "--yes"])
    try:
        for prompt, value in [(b"Email: ", b"terminal@example.com\n"), (b"First name: ", b"Terminal\n"), (b"Last name: ", b"Test\n")]:
            terminal.expect(prompt)
            terminal.send(value)
        terminal.hidden(b"Password: ", "🌳é".encode() + b"\x7f\x7f" + SECRET + b" discarded  \x17\x7f\n")
        terminal.hidden(b"Confirm password: ", SECRET + b"\n")
        assert terminal.finish() == 1
        assert b"Invalid database configuration" in terminal.data
        assert termios.tcgetattr(terminal.fd) == terminal.original
    finally:
        terminal.close()


def profile_cancellation_and_background_refusal():
    for action in ["eof", "signal"]:
        terminal = Terminal(["bootstrap", "--yes"])
        try:
            terminal.expect(b"Email: ")
            if action == "eof":
                terminal.send(b"\x04")
            else:
                os.kill(terminal.pid, signal.SIGTERM)
            assert terminal.finish() == 1
            assert termios.tcgetattr(terminal.fd) == terminal.original
        finally:
            terminal.close()
    terminal = Terminal(["bootstrap", "--yes"], background=True)
    try:
        assert terminal.finish() == 1
        assert b"Cannot access the foreground terminal" in terminal.data
        assert b"Email: " not in terminal.data
        assert termios.tcgetattr(terminal.fd) == terminal.original
    finally:
        terminal.close()


def mismatch_restores_terminal():
    terminal = Terminal(["bootstrap", "--yes"])
    try:
        for prompt, value in [(b"Email: ", b"terminal@example.com\n"), (b"First name: ", b"Terminal\n"), (b"Last name: ", b"Test\n")]:
            terminal.expect(prompt)
            terminal.send(value)
        terminal.hidden(b"Password: ", SECRET + b"\n")
        terminal.hidden(b"Confirm password: ", b"different-synthetic-passphrase\n")
        assert terminal.finish() == 1
        assert b"Password confirmation does not match" in terminal.data
        assert termios.tcgetattr(terminal.fd) == terminal.original
    finally:
        terminal.close()


def prompt_write_failure_restores_terminal():
    for at_confirmation in [False, True]:
        terminal = Terminal(["bootstrap", "--yes"], separate_stderr=True)
        try:
            for prompt, value in [(b"Email: ", b"terminal@example.com\n"), (b"First name: ", b"Terminal\n")]:
                terminal.expect(prompt)
                terminal.send(value)
            terminal.expect(b"Last name: ")
            if at_confirmation:
                terminal.send(b"Test\n")
                terminal.hidden(b"Password: ", SECRET)
            os.close(terminal.error_fd)
            terminal.error_fd = None
            terminal.send(b"\n" if at_confirmation else b"Test\n")
            assert terminal.finish() == 74  # Failure diagnostic also has a closed stderr.
            assert termios.tcgetattr(terminal.fd) == terminal.original
        finally:
            terminal.close()


def terminal_loss_is_reported_without_secret_disclosure():
    terminal = Terminal(["bootstrap", "--yes"], separate_stderr=True)
    try:
        for prompt, value in [(b"Email: ", b"terminal@example.com\n"), (b"First name: ", b"Terminal\n"), (b"Last name: ", b"Test\n")]:
            terminal.expect(prompt)
            terminal.send(value)
        terminal.hidden(b"Password: ", SECRET)
        os.close(terminal.fd)
        terminal.fd = None
        assert terminal.finish() == 1
        assert any(message in terminal.data for message in [b"Operator command interrupted", b"Cannot read terminal input", b"Terminal input cancelled"])
        assert b"Invalid database configuration" not in terminal.data
    finally:
        terminal.close()


def confirmation():
    for reply, expected in [(b"no\n", 3), (b"\x04", 3), (b"yes\n", 1)]:
        terminal = Terminal(["operator", "migrate"])
        try:
            terminal.expect(b"typing yes: ")
            terminal.send(reply)
            assert terminal.finish() == expected
            if expected == 3:
                assert b"Invalid database configuration" not in terminal.data
        finally:
            terminal.close()


def broken_output():
    read_fd, write_fd = os.pipe()
    os.close(read_fd)
    try:
        result = subprocess.run([BINARY, "--help"], env=ENV, stdout=write_fd, stderr=subprocess.PIPE, timeout=10)
        assert result.returncode == 74
        assert b"panicked" not in result.stderr
    finally:
        os.close(write_fd)
    result = subprocess.run([os.fsencode(BINARY), b"\xff"], env=ENV, capture_output=True, timeout=10)
    assert result.returncode == 2 and result.stdout == b""


confirmation()
automated = Terminal(["--output", "json", "operator", "migrate"])
try:
    assert automated.finish() == 3
    assert json.loads(automated.data)["error"]["code"] == "confirmation_required"
finally:
    automated.close()
for scenario in ["complete", "eof", "interrupt"]:
    bootstrap(scenario)
for at_confirmation in [False, True]:
    for scenario in ["oversized", "escape", "invalid_utf8", "eof", signal.SIGINT, signal.SIGTERM, signal.SIGHUP, signal.SIGQUIT, signal.SIGTSTP, signal.SIGTTIN, signal.SIGTTOU]:
        password_boundary(scenario, at_confirmation)
edited_password()
profile_cancellation_and_background_refusal()
mismatch_restores_terminal()
prompt_write_failure_restores_terminal()
terminal_loss_is_reported_without_secret_disclosure()
broken_output()
print("CLI terminal checks passed: bounded hidden input, Unicode editing, secret redaction, EOF, catchable-signal cancellation and exact terminal restoration, plus malformed input and broken output.")
