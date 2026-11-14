//! Byte-for-byte golden comparison and the bless mode (design §8.11.12, Req
//! 20.11).

use crate::error::DemoError;
use std::fs;
use std::path::Path;

/// Whether golden files are compared or rewritten.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Compare every produced file with its golden file.
    Compare,
    /// Write the produced bytes as the golden files.
    Bless,
}

impl Mode {
    /// Chooses the mode from the values of `RT_DEMO_BLESS` and `CI` (`None`
    /// when unset). Bless mode needs `RT_DEMO_BLESS=1`; it is refused when `CI`
    /// is set to anything.
    ///
    /// # Errors
    /// [`DemoError::BlessRefused`] when bless is requested and `CI` is set.
    pub fn from_env_values(bless: Option<&str>, ci: Option<&str>) -> Result<Self, DemoError> {
        if bless != Some("1") {
            return Ok(Mode::Compare);
        }
        if ci.is_some() {
            return Err(DemoError::BlessRefused);
        }
        Ok(Mode::Bless)
    }
}

/// The offset of the first byte where `a` and `b` differ, or the length of the
/// shorter input when one is a prefix of the other. `None` when they are equal.
#[must_use]
pub fn first_difference(a: &[u8], b: &[u8]) -> Option<usize> {
    let common = a.iter().zip(b.iter()).position(|(x, y)| x != y);
    match common {
        Some(i) => Some(i),
        None if a.len() == b.len() => None,
        None => Some(a.len().min(b.len())),
    }
}

fn io_error(path: &Path, e: &std::io::Error) -> DemoError {
    DemoError::Io {
        path: path.display().to_string(),
        problem: e.to_string(),
    }
}

/// Compares `actual` with the golden file `demo_dir/golden` (or writes it in
/// bless mode).
///
/// # Errors
/// [`DemoError::Golden`] with the step, the golden path and the first differing
/// offset. A missing golden file is reported at offset 0.
pub fn check_golden(
    mode: Mode,
    step: &str,
    demo_dir: &Path,
    golden: &str,
    actual: &[u8],
) -> Result<(), DemoError> {
    let path = demo_dir.join(golden);
    if mode == Mode::Bless {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| io_error(parent, &e))?;
        }
        return fs::write(&path, actual).map_err(|e| io_error(&path, &e));
    }
    let expected = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) => {
            return Err(DemoError::Golden {
                step: step.to_owned(),
                golden: golden.to_owned(),
                offset: 0,
                problem: format!("golden file cannot be read: {e}"),
            })
        }
    };
    match first_difference(actual, &expected) {
        None => Ok(()),
        Some(offset) => Err(DemoError::Golden {
            step: step.to_owned(),
            golden: golden.to_owned(),
            offset,
            problem: format!(
                "produced {} bytes, golden has {} bytes",
                actual.len(),
                expected.len()
            ),
        }),
    }
}
