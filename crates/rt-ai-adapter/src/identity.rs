//! Provider identity and usage, as plain provider-neutral values (UD-37,
//! Req 35.1).

use std::borrow::Cow;
use std::fmt;

/// The longest provider identifier, in bytes.
pub const MAX_PROVIDER_ID_LEN: usize = 64;

/// Why a text is not a [`ProviderId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderIdError {
    /// The text is empty.
    Empty,
    /// The text is longer than [`MAX_PROVIDER_ID_LEN`] bytes.
    TooLong,
    /// The first character is not a lowercase ASCII letter.
    BadStart,
    /// A character is not a lowercase ASCII letter, a digit or a hyphen.
    BadCharacter,
    /// The text ends with a hyphen.
    TrailingHyphen,
}

/// A provider identifier such as `meta` or `mock-replay`.
///
/// One to 64 bytes of lowercase ASCII letters, digits and hyphens. It
/// starts with a letter and does not end with a hyphen.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProviderId(Cow<'static, str>);

impl ProviderId {
    /// Validates `text`.
    ///
    /// # Errors
    /// The first rule that `text` breaks, as a [`ProviderIdError`].
    pub fn new(text: &str) -> Result<Self, ProviderIdError> {
        Self::check(text).map(|()| Self(Cow::Owned(text.to_owned())))
    }

    /// An identifier written in source, for an adapter that names itself.
    ///
    /// Used in a `const`, an invalid literal is a compile error, so no
    /// run-time path can fail.
    ///
    /// # Panics
    /// When `text` is not valid, which stops compilation in a `const` item.
    #[must_use]
    #[allow(clippy::panic)]
    pub const fn literal(text: &'static str) -> Self {
        match Self::check(text) {
            Ok(()) => Self(Cow::Borrowed(text)),
            Err(_) => panic!("invalid provider identifier literal"),
        }
    }

    const fn check(text: &str) -> Result<(), ProviderIdError> {
        let mut rest = text.as_bytes();
        if rest.is_empty() {
            return Err(ProviderIdError::Empty);
        }
        if rest.len() > MAX_PROVIDER_ID_LEN {
            return Err(ProviderIdError::TooLong);
        }
        let mut first = true;
        let mut last = 0u8;
        while let [byte, tail @ ..] = rest {
            let byte = *byte;
            let letter = byte >= b'a' && byte <= b'z';
            if first && !letter {
                return Err(ProviderIdError::BadStart);
            }
            if !(letter || byte.is_ascii_digit() || byte == b'-') {
                return Err(ProviderIdError::BadCharacter);
            }
            first = false;
            last = byte;
            rest = tail;
        }
        if last == b'-' {
            return Err(ProviderIdError::TrailingHyphen);
        }
        Ok(())
    }

    /// The identifier text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The wire protocol that an adapter speaks. A later protocol is a new
/// variant, so a `match` outside this crate needs a wildcard arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Protocol {
    /// The OpenAI Responses protocol (`openai-responses`).
    OpenAiResponses,
    /// The recorded-script protocol of the mock providers (`mock-replay`).
    MockReplay,
}

impl Protocol {
    /// The name used in configuration and in stored batches.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenAiResponses => "openai-responses",
            Self::MockReplay => "mock-replay",
        }
    }

    /// The protocol with this exact name, if there is one.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "openai-responses" => Some(Self::OpenAiResponses),
            "mock-replay" => Some(Self::MockReplay),
            _ => None,
        }
    }
}

/// Who answered: the provider, the protocol, and the adapter version. It
/// names the adapter, not the model, and is never read from a response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// The provider identifier.
    pub provider: ProviderId,
    /// The protocol that the adapter speaks.
    pub protocol: Protocol,
    /// The adapter version.
    pub adapter_version: String,
}

impl Identity {
    /// An identity from its three parts.
    #[must_use]
    pub fn new(provider: ProviderId, protocol: Protocol, adapter_version: &str) -> Self {
        Self {
            provider,
            protocol,
            adapter_version: adapter_version.to_owned(),
        }
    }
}

/// Token counts as the provider reported them. An absent count stays
/// absent: nothing is derived, summed or estimated.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    /// Tokens of the request.
    pub input_tokens: Option<u64>,
    /// Tokens of the answer.
    pub output_tokens: Option<u64>,
    /// Total tokens.
    pub total_tokens: Option<u64>,
}
