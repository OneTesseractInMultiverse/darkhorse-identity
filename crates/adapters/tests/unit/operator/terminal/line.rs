use super::*;

fn enter(line: &mut Line, bytes: &[u8]) -> Result<(), &'static str> {
    for byte in bytes {
        assert_eq!(line.push(*byte)?, Step::Continue);
    }
    Ok(())
}

#[test]
fn input_rejects_the_first_excess_byte_without_waiting_for_newline() {
    let mut line = Line::new();
    enter(&mut line, &[b'a'; LIMIT]).unwrap();
    assert_eq!(line.text().unwrap().len(), LIMIT);
    assert_eq!(line.push(b'b'), Err("Terminal input is too long."));
    assert_eq!(line.text().unwrap().len(), LIMIT);
    assert_eq!(line.push(b'\n'), Ok(Step::Complete));
}

#[test]
fn editing_preserves_unicode_and_spaces_and_erases_removed_bytes() {
    let mut line = Line::new();
    enter(&mut line, " a🌳é".as_bytes()).unwrap();
    line.push(0x7f).unwrap();
    assert_eq!(line.text().unwrap().as_str(), " a🌳");
    line.push(0x08).unwrap();
    assert_eq!(line.text().unwrap().as_str(), " a");
    assert!(line.bytes[line.len..].iter().all(|b| *b == 0));
    enter(&mut line, b" b  ").unwrap();
    line.push(0x17).unwrap();
    assert_eq!(line.text().unwrap().as_str(), " a ");
    line.push(0x15).unwrap();
    assert_eq!(line.text().unwrap().as_str(), "");
    assert!(line.bytes.iter().all(|b| *b == 0));
    line.push(0x08).unwrap();
    line.push(0x17).unwrap();
    assert_eq!(line.push(b'\r'), Ok(Step::Complete));
}

#[test]
fn cancellation_invalid_utf8_and_control_sequences_never_become_password_text() {
    for byte in [3, 4, 26, 28] {
        let mut line = Line::new();
        enter(&mut line, b"synthetic-secret").unwrap();
        assert_eq!(line.push(byte), Err("Terminal input cancelled."));
    }
    for bytes in [vec![0xff], vec![0xc3], "a\u{009b}b".as_bytes().to_vec()] {
        let mut line = Line::new();
        enter(&mut line, &bytes).unwrap();
        assert!(line.text().is_err());
    }
    for byte in [0, 1, 9, 27, 31] {
        assert_eq!(Line::new().push(byte), Err("Invalid terminal input."));
    }
    let mut line = Line::new();
    enter(&mut line, b";$(echo literal)").unwrap();
    assert_eq!(line.text().unwrap().as_str(), ";$(echo literal)");
}
