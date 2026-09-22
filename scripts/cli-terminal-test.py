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
    def __init__(self, args):
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            os.execve(BINARY, [BINARY, *args], ENV)
        self.data = b""
        self.status = None
        self.deadline = time.monotonic() + 10

    def read(self):
        assert time.monotonic() < self.deadline, "terminal test exceeded deadline"
        if select.select([self.fd], [], [], 0.05)[0]:
            try:
                self.data += os.read(self.fd, 4096)
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
        os.close(self.fd)


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
            assert result == (1 if until == "eof" else -signal.SIGINT)
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
broken_output()
print("CLI terminal checks passed: hidden passwords, confirmation, EOF, interruption, invalid UTF-8 and broken output without panics or secret disclosure.")
