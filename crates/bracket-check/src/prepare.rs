//! The Prepare group: map the files, check the Targets, build the Questions
//! and return the canonical bytes of what the engine receives (design §11.3).

use crate::classify::ResultClass;
use crate::mapping::{input_schema, internal, target_kinds, FileInputs, NamedFile};
use crate::questions::{build_questions, check_request, BracketRequest};
use rt_compiled::{sha256_of, CompiledRulebook};
use rt_data::tabular::{
    data_bytes, map_files, published_bytes, questions_bytes, MappedFiles, UnusedPublished,
};
use rt_data::{CompetitionData, PublishedResult, Question};
use rt_explain::report::Sha256Hex;
use rt_foundation::{Diagnostic, DiagnosticState};

/// Role names of the input hashes in the report header.
const ROLE_PARTICIPANTS: &str = "participants";
const ROLE_COLLECTIONS: &str = "collections";
const ROLE_PUBLISHED: &str = "published";

/// Why Prepare returned no Questions. The engine is not called.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rejection {
    /// Invalid input: the mapping errors of Req 31.9 and the Target errors
    /// `APP_TARGET_NOT_DECLARED` and `APP_TARGET_KIND_MISMATCH`, all
    /// collected, in canonical order.
    InvalidInput(DiagnosticState),
    /// The Compiled_Rulebook cannot be turned into a mapping schema.
    Internal(Box<Diagnostic>),
}

impl Rejection {
    /// The result classification of the rejected run.
    #[must_use]
    pub fn class(&self) -> ResultClass {
        match self {
            Self::InvalidInput(_) => ResultClass::InvalidInput,
            Self::Internal(_) => ResultClass::Internal,
        }
    }

    /// Every Diagnostic of the rejection, in canonical order.
    #[must_use]
    pub fn diagnostics(&self) -> DiagnosticState {
        match self {
            Self::InvalidInput(state) => state.clone(),
            Self::Internal(d) => {
                let mut state = DiagnosticState::empty();
                state.push((**d).clone());
                state
            }
        }
    }
}

/// What Prepare produced: the inputs of one engine call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prepared {
    data: CompetitionData,
    questions: Vec<Question>,
    published: Option<PublishedResult>,
    not_requested: Vec<UnusedPublished>,
    input_hashes: Vec<(&'static str, Sha256Hex)>,
}

impl Prepared {
    /// The mapped Competition_Data.
    #[must_use]
    pub fn data(&self) -> &CompetitionData {
        &self.data
    }

    /// The Questions, in the order the engine answers them.
    #[must_use]
    pub fn questions(&self) -> &[Question] {
        &self.questions
    }

    /// The mapped Published_Result, when a published file was given.
    #[must_use]
    pub fn published(&self) -> Option<&PublishedResult> {
        self.published.as_ref()
    }

    /// Published rows for declared Targets that were not requested.
    #[must_use]
    pub fn not_requested(&self) -> &[UnusedPublished] {
        &self.not_requested
    }

    /// The SHA-256 of each input file's bytes by role, in a fixed order.
    #[must_use]
    pub fn input_hashes(&self) -> &[(&'static str, Sha256Hex)] {
        &self.input_hashes
    }

    /// The canonical RT-JSON bytes of the Competition_Data.
    #[must_use]
    pub fn data_bytes(&self) -> Vec<u8> {
        data_bytes(&self.data)
    }

    /// The canonical RT-JSON bytes of the Questions.
    #[must_use]
    pub fn questions_bytes(&self) -> Vec<u8> {
        questions_bytes(&self.questions)
    }

    /// The canonical RT-JSON bytes of the Published_Result, when there is
    /// one.
    #[must_use]
    pub fn published_bytes(&self) -> Option<Vec<u8>> {
        self.published.as_ref().map(published_bytes)
    }
}

fn hash_of(
    role: &'static str,
    file: NamedFile<'_>,
) -> Result<(&'static str, Sha256Hex), Rejection> {
    Sha256Hex::new(sha256_of(file.bytes).as_str())
        .map(|hash| (role, hash))
        .ok_or_else(|| {
            Rejection::Internal(Box::new(internal(
                "a SHA-256 digest is not 64 lowercase hexadecimal digits".to_owned(),
            )))
        })
}

fn input_hashes(files: &FileInputs<'_>) -> Result<Vec<(&'static str, Sha256Hex)>, Rejection> {
    let mut out = vec![hash_of(ROLE_PARTICIPANTS, files.participants)?];
    if let Some(file) = files.collections {
        out.push(hash_of(ROLE_COLLECTIONS, file)?);
    }
    if let Some(file) = files.published {
        out.push(hash_of(ROLE_PUBLISHED, file)?);
    }
    Ok(out)
}

/// Maps the input files, checks the requested Targets and builds the
/// Questions.
///
/// Every violation of every file and of the request is returned together in
/// canonical order, and then no Question exists and no engine call can
/// follow. A mapping error is exactly the Diagnostic `rt-data::tabular`
/// returned. A Target that is not declared is `APP_TARGET_NOT_DECLARED`, and a
/// Target whose kind its option does not report is
/// `APP_TARGET_KIND_MISMATCH`, each with the Cli_Location of its argument.
///
/// # Errors
/// [`Rejection`] for invalid input or an unusable Compiled_Rulebook.
pub fn prepare(
    compiled: &CompiledRulebook,
    request: &BracketRequest,
    files: &FileInputs<'_>,
) -> Result<Prepared, Rejection> {
    let schema = input_schema(compiled).map_err(Rejection::Internal)?;
    let kinds = target_kinds(compiled).map_err(Rejection::Internal)?;
    let (checked, problems) = check_request(request, &kinds);
    let mut state = DiagnosticState::empty();
    for d in problems {
        state.push(d);
    }
    let mapped = map_files(&schema, &kinds, &checked.all(), files, &mut state);
    let Some(MappedFiles { data, published }) = mapped else {
        return Err(Rejection::InvalidInput(state.sorted()));
    };
    if !state.as_slice().is_empty() {
        return Err(Rejection::InvalidInput(state.sorted()));
    }
    let questions = build_questions(&data, &checked);
    let (published, not_requested) = match published {
        Some(mapped) => (Some(mapped.result), mapped.unused),
        None => (None, Vec::new()),
    };
    Ok(Prepared {
        data,
        questions,
        published,
        not_requested,
        input_hashes: input_hashes(files)?,
    })
}
