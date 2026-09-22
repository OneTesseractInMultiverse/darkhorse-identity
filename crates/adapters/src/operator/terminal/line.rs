use zeroize::{Zeroize, Zeroizing};

pub(super) const LIMIT: usize = 1024;
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Step {
    Continue,
    Complete,
}

// Fixed capacity: editing and partial UTF-8 never grow a secret allocation.
pub(super) struct Line {
    bytes: Zeroizing<[u8; LIMIT]>,
    len: usize,
}
impl Line {
    pub(super) fn new() -> Self {
        Self {
            bytes: Zeroizing::new([0; LIMIT]),
            len: 0,
        }
    }
    pub(super) fn push(&mut self, byte: u8) -> Result<Step, &'static str> {
        match byte {
            b'\r' | b'\n' => return Ok(Step::Complete),
            3 | 4 | 26 | 28 => return Err("Terminal input cancelled."),
            8 | 127 => self.pop(),
            21 => self.truncate(0),
            23 => self.word(),
            0..=31 => return Err("Invalid terminal input."),
            _ => {
                if self.len == LIMIT {
                    return Err("Terminal input is too long.");
                }
                self.bytes[self.len] = byte;
                self.len += 1;
            }
        }
        Ok(Step::Continue)
    }
    pub(super) fn text(&self) -> Result<Zeroizing<String>, &'static str> {
        let text =
            std::str::from_utf8(&self.bytes[..self.len]).map_err(|_| "Invalid terminal input.")?;
        if text.chars().any(char::is_control) {
            return Err("Invalid terminal input.");
        }
        Ok(Zeroizing::new(text.to_owned()))
    }
    fn truncate(&mut self, len: usize) {
        self.bytes[len..self.len].zeroize();
        self.len = len;
    }
    fn pop(&mut self) {
        if self.len == 0 {
            return;
        }
        let mut start = self.len - 1;
        while start > 0 && self.bytes[start] & 0xc0 == 0x80 {
            start -= 1;
        }
        self.truncate(start);
    }
    fn word(&mut self) {
        while self.len > 0 && self.bytes[self.len - 1] == b' ' {
            self.pop();
        }
        while self.len > 0 && self.bytes[self.len - 1] != b' ' {
            self.pop();
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/operator/terminal/line.rs"]
mod tests;
