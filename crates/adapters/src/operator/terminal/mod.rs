use nix::sys::termios::{self, FlushArg, LocalFlags, SetArg, Termios};
use std::{
    fs::{File, OpenOptions},
    io::{self, IsTerminal, Read, Write},
    os::unix::fs::OpenOptionsExt,
};
use tokio::io::unix::AsyncFd;
use zeroize::Zeroizing;
mod line;
use line::{Line, Step};

const UNAVAILABLE: &str = "Cannot access the foreground terminal.";
const RESTORE: &str = "Cannot restore terminal settings; restore them before entering more input.";

pub(super) struct Terminal {
    fd: AsyncFd<File>,
    original: Termios,
    restored: bool,
}
impl Terminal {
    pub(super) fn open() -> Result<Self, &'static str> {
        if !io::stdin().is_terminal() {
            return Err(
                "Interactive bootstrap requires a terminal; use --stdin for protected piped input.",
            );
        }
        let source = nix::sys::stat::fstat(io::stdin()).map_err(|_| UNAVAILABLE)?;
        let path = nix::unistd::ttyname(io::stdin()).map_err(|_| UNAVAILABLE)?;
        // A separate open file description avoids changing the shell's stdin flags.
        // macOS kqueue requires the concrete terminal, not the /dev/tty alias.
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(nix::libc::O_NONBLOCK | nix::libc::O_NOCTTY)
            .open(path)
            .map_err(|_| UNAVAILABLE)?;
        let opened = nix::sys::stat::fstat(&file).map_err(|_| UNAVAILABLE)?;
        if opened.st_dev != source.st_dev
            || opened.st_ino != source.st_ino
            || opened.st_rdev != source.st_rdev
        {
            return Err(UNAVAILABLE);
        }
        if nix::unistd::tcgetpgrp(&file).map_err(|_| UNAVAILABLE)? != nix::unistd::getpgrp() {
            return Err(UNAVAILABLE);
        }
        let original = termios::tcgetattr(&file).map_err(|_| UNAVAILABLE)?;
        let fd =
            AsyncFd::with_interest(file, tokio::io::Interest::READABLE).map_err(|_| UNAVAILABLE)?;
        Ok(Self {
            fd,
            original,
            restored: false,
        })
    }
    pub(super) fn hide(&self) -> Result<(), &'static str> {
        termios::tcflush(self.fd.get_ref(), FlushArg::TCIFLUSH).map_err(|_| UNAVAILABLE)?;
        let mode = hidden(self.original.clone());
        termios::tcsetattr(self.fd.get_ref(), SetArg::TCSANOW, &mode).map_err(|_| UNAVAILABLE)?;
        if termios::tcgetattr(self.fd.get_ref()).map_err(|_| UNAVAILABLE)? != mode {
            return Err(UNAVAILABLE);
        }
        Ok(())
    }
    pub(super) async fn prompt(
        &self,
        message: &'static str,
    ) -> Result<Zeroizing<String>, &'static str> {
        prompt(message)?;
        let mut line = Line::new();
        loop {
            match line.push(self.byte().await?)? {
                Step::Continue => (),
                Step::Complete => return line.text(),
            }
        }
    }
    async fn byte(&self) -> Result<u8, &'static str> {
        // Yield even if the device remains readable during continuous editing input.
        tokio::task::yield_now().await;
        let mut byte = Zeroizing::new([0]);
        loop {
            let mut ready = self
                .fd
                .readable()
                .await
                .map_err(|_| "Cannot read terminal input.")?;
            match ready.try_io(|fd| fd.get_ref().read(&mut *byte)) {
                Ok(Ok(1)) => return Ok(byte[0]),
                Ok(Ok(_)) => return Err("Terminal input cancelled."),
                Ok(Err(_)) => return Err("Cannot read terminal input."),
                Err(_) => (),
            }
        }
    }
    pub(super) fn restore(&mut self) -> Result<(), &'static str> {
        if self.restored {
            return Ok(());
        }
        // Do not wait for output to drain: a stopped terminal must remain cancellable.
        let flushed = termios::tcflush(self.fd.get_ref(), FlushArg::TCIFLUSH);
        let restored = termios::tcsetattr(self.fd.get_ref(), SetArg::TCSANOW, &self.original);
        let verified = termios::tcgetattr(self.fd.get_ref());
        self.restored =
            restored.is_ok() && verified.as_ref().is_ok_and(|mode| *mode == self.original);
        if flushed.is_err() || !self.restored {
            return Err(RESTORE);
        }
        Ok(())
    }
}
impl Drop for Terminal {
    fn drop(&mut self) {
        if self.restore().is_err() {
            // Cancellation already fails; report cleanup failure without a printing panic.
            let _ = writeln!(io::stderr().lock(), "{RESTORE}");
        }
    }
}
fn hidden(mut original: Termios) -> Termios {
    let output = original.output_flags;
    termios::cfmakeraw(&mut original);
    original.output_flags = output;
    original.local_flags.remove(LocalFlags::EXTPROC);
    original.local_flags.insert(LocalFlags::ISIG);
    original
}
fn prompt(message: &'static str) -> Result<(), &'static str> {
    let mut error = io::stderr().lock();
    error
        .write_all(message.as_bytes())
        .and_then(|()| error.flush())
        .map_err(|_| "Cannot write prompt.")
}
