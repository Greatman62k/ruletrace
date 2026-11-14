//! Validated name, identity and date newtypes.

use std::fmt;

/// Why a [`Name`] was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameError {
    /// The name is empty.
    Empty,
    /// The name contains a Unicode control character.
    ControlCharacter,
    /// An opaque construct name is not `[a-z][a-z0-9_]*`.
    NotWellFormed,
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            NameError::Empty => "name is empty",
            NameError::ControlCharacter => "name contains a control character",
            NameError::NotWellFormed => "name does not match [a-z][a-z0-9_]*",
        })
    }
}

impl std::error::Error for NameError {}

/// A declared name (input, field, enum, set, collection, Target). Non-empty, no
/// control character. Ordered bytewise.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Name(String);

impl Name {
    /// Builds a name.
    ///
    /// # Errors
    /// Returns [`NameError`] for empty text or text with a control character.
    pub fn new(text: impl Into<String>) -> Result<Self, NameError> {
        let text = text.into();
        if text.is_empty() {
            Err(NameError::Empty)
        } else if text.chars().any(char::is_control) {
            Err(NameError::ControlCharacter)
        } else {
            Ok(Self(text))
        }
    }

    /// The text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The name of a well-formed construct kind or node that this version does not
/// define: `[a-z][a-z0-9_]*` (design §4.1.3).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OpaqueName(String);

impl OpaqueName {
    /// Builds the name.
    ///
    /// # Errors
    /// Returns [`NameError::NotWellFormed`] unless the text matches
    /// `[a-z][a-z0-9_]*`.
    pub fn new(text: impl Into<String>) -> Result<Self, NameError> {
        let text = text.into();
        let mut bytes = text.bytes();
        let ok = matches!(bytes.next(), Some(b'a'..=b'z'))
            && bytes.all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_'));
        if ok {
            Ok(Self(text))
        } else {
            Err(NameError::NotWellFormed)
        }
    }

    /// The text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Why an [`Identity`] was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityError {
    /// The identity is empty.
    Empty,
    /// The identity is longer than 200 Unicode scalar values.
    TooLong,
}

impl fmt::Display for IdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            IdentityError::Empty => "identity is empty",
            IdentityError::TooLong => "identity is longer than 200 characters",
        })
    }
}

impl std::error::Error for IdentityError {}

/// An author, proposer or AI identity: 1 to 200 Unicode scalar values (Req
/// 25.2).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Identity(String);

impl Identity {
    /// Maximum length in Unicode scalar values.
    pub const MAX_CHARS: usize = 200;

    /// Builds an identity.
    ///
    /// # Errors
    /// Returns [`IdentityError`] for empty text or more than 200 scalar values.
    pub fn new(text: impl Into<String>) -> Result<Self, IdentityError> {
        let text = text.into();
        if text.is_empty() {
            return Err(IdentityError::Empty);
        }
        if text.chars().nth(Self::MAX_CHARS).is_some() {
            return Err(IdentityError::TooLong);
        }
        Ok(Self(text))
    }

    /// The text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Why a [`Date`] was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateError {
    /// The text is not `YYYY-MM-DD` with ASCII digits.
    BadShape,
    /// The year is not 1 to 9999.
    YearOutOfRange,
    /// The month is not 1 to 12.
    MonthOutOfRange,
    /// The day does not exist in that month.
    DayOutOfRange,
}

impl fmt::Display for DateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            DateError::BadShape => "date is not YYYY-MM-DD",
            DateError::YearOutOfRange => "year is not 1 to 9999",
            DateError::MonthOutOfRange => "month is not 1 to 12",
            DateError::DayOutOfRange => "day does not exist in that month",
        })
    }
}

impl std::error::Error for DateError {}

/// A calendar date without time (proleptic Gregorian), `0001-01-01` to
/// `9999-12-31`. Ordered chronologically.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    year: u16,
    month: u8,
    day: u8,
}

impl Date {
    /// Builds a date, checking month lengths and leap years.
    ///
    /// # Errors
    /// Returns the first failed range as a [`DateError`].
    pub fn new(year: u32, month: u32, day: u32) -> Result<Self, DateError> {
        let year = u16::try_from(year)
            .ok()
            .filter(|y| (1..=9999).contains(y))
            .ok_or(DateError::YearOutOfRange)?;
        let month = u8::try_from(month)
            .ok()
            .filter(|m| (1..=12).contains(m))
            .ok_or(DateError::MonthOutOfRange)?;
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        let last = match month {
            2 if leap => 29,
            2 => 28,
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        };
        let day = u8::try_from(day)
            .ok()
            .filter(|d| (1..=last).contains(d))
            .ok_or(DateError::DayOutOfRange)?;
        Ok(Self { year, month, day })
    }

    /// Parses exactly `YYYY-MM-DD`.
    ///
    /// # Errors
    /// Returns [`DateError::BadShape`] for any other shape, else the range
    /// error.
    pub fn parse(text: &str) -> Result<Self, DateError> {
        let b = text.as_bytes();
        let digits = |range: std::ops::Range<usize>| -> Option<u32> {
            let part = b.get(range)?;
            part.iter().try_fold(0u32, |acc, d| {
                d.is_ascii_digit().then(|| acc * 10 + u32::from(*d - b'0'))
            })
        };
        if b.len() != 10 || b.get(4) != Some(&b'-') || b.get(7) != Some(&b'-') {
            return Err(DateError::BadShape);
        }
        match (digits(0..4), digits(5..7), digits(8..10)) {
            (Some(y), Some(m), Some(d)) => Self::new(y, m, d),
            _ => Err(DateError::BadShape),
        }
    }

    /// Year, 1 to 9999.
    #[must_use]
    pub fn year(&self) -> u16 {
        self.year
    }

    /// Month, 1 to 12.
    #[must_use]
    pub fn month(&self) -> u8 {
        self.month
    }

    /// Day of month, 1 to 31.
    #[must_use]
    pub fn day(&self) -> u8 {
        self.day
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}
