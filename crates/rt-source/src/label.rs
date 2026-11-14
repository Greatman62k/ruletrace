//! Numbering labels (design §3.5, Extraction).

/// The numbering label at the start of `s`, when one is followed by whitespace.
///
/// The accepted forms are `\d+(\.\d+)*[.)]?`, `\([a-z]{1,2}\)`, `\([ivxlc]+\)`
/// and `[A-Z][.)]`. The label text includes its closing punctuation. A label at
/// the very end of `s` has no following whitespace and is not a label.
pub(crate) fn leading_label(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    let len = match *bytes.first()? {
        b'0'..=b'9' => numeric_len(bytes),
        b'(' => parenthesised_len(bytes)?,
        b'A'..=b'Z' => match bytes.get(1) {
            Some(b'.' | b')') => 2,
            _ => return None,
        },
        _ => return None,
    };
    let label = s.get(..len)?;
    let next = s.get(len..)?.chars().next()?;
    next.is_whitespace().then_some(label)
}

fn digits(bytes: &[u8], from: usize) -> usize {
    bytes.get(from..).map_or(0, |rest| {
        rest.iter().take_while(|b| b.is_ascii_digit()).count()
    })
}

fn numeric_len(bytes: &[u8]) -> usize {
    let mut i = digits(bytes, 0);
    while bytes.get(i) == Some(&b'.') {
        let more = digits(bytes, i + 1);
        if more == 0 {
            break;
        }
        i += 1 + more;
    }
    if matches!(bytes.get(i), Some(b'.' | b')')) {
        i += 1;
    }
    i
}

fn parenthesised_len(bytes: &[u8]) -> Option<usize> {
    let close = bytes.iter().position(|&b| b == b')')?;
    let inner = bytes.get(1..close)?;
    let letters = !inner.is_empty() && inner.len() <= 2 && inner.iter().all(u8::is_ascii_lowercase);
    let roman = !inner.is_empty() && inner.iter().all(|b| b"ivxlc".contains(b));
    (letters || roman).then_some(close + 1)
}

#[cfg(test)]
mod tests {
    use super::leading_label;

    #[test]
    fn accepts_each_form() {
        for (text, label) in [
            ("1 x", "1"),
            ("1. x", "1."),
            ("1.2.3) x", "1.2.3)"),
            ("12.4\nx", "12.4"),
            ("(a) x", "(a)"),
            ("(ab) x", "(ab)"),
            ("(xiv) x", "(xiv)"),
            ("B. x", "B."),
            ("C) x", "C)"),
        ] {
            assert_eq!(leading_label(text), Some(label), "{text:?}");
        }
    }

    #[test]
    fn rejects_other_text() {
        for text in [
            "", "1", "1.", "1.x ", "1.2.x ", "(abc) x", "(A) x", "() x", "AB. x", "A x", "a. x",
            "x 1. ", "(1) x",
        ] {
            assert_eq!(leading_label(text), None, "{text:?}");
        }
    }
}
