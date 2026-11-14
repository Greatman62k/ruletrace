//! The single control-character escaping function (Req 19.5, 19.10).

use std::fmt::Write as _;

/// Escapes every Unicode `Cc` character as `\u{hex}`, except a line feed that
/// is the final character of the input. All rendered untrusted text must pass
/// through this function.
#[must_use]
pub fn escape_controls(text: &str) -> String {
    let (body, tail) = match text.strip_suffix('\n') {
        Some(body) => (body, "\n"),
        None => (text, ""),
    };
    let mut out = String::with_capacity(text.len());
    for c in body.chars() {
        if c.is_control() {
            // Writing to a String cannot fail.
            let _ = write!(out, "\\u{{{:x}}}", u32::from(c));
        } else {
            out.push(c);
        }
    }
    out.push_str(tail);
    out
}

#[cfg(test)]
mod tests {
    use super::escape_controls;

    #[test]
    fn escapes_esc_nul_cr_and_c1() {
        assert_eq!(
            escape_controls("a\u{1b}[0m\0\r\u{85}\u{7f}"),
            "a\\u{1b}[0m\\u{0}\\u{d}\\u{85}\\u{7f}"
        );
    }

    #[test]
    fn keeps_only_the_final_line_feed() {
        assert_eq!(escape_controls("a\nb\n"), "a\\u{a}b\n");
        assert_eq!(escape_controls("\n"), "\n");
        assert_eq!(escape_controls("a\n\n"), "a\\u{a}\n");
    }

    #[test]
    fn leaves_plain_and_non_cc_text_unchanged() {
        assert_eq!(
            escape_controls("héllo \u{200b}\u{2028}"),
            "héllo \u{200b}\u{2028}"
        );
    }

    #[test]
    fn output_has_no_control_character_except_final_lf() {
        for cp in 0u32..0x200 {
            if let Some(c) = char::from_u32(cp) {
                let out = escape_controls(&format!("x{c}y\n"));
                let body = out.strip_suffix('\n').unwrap_or(&out);
                assert!(!body.chars().any(char::is_control), "U+{cp:04X}");
            }
        }
    }
}
