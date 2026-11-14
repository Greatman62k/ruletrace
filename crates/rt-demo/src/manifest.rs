//! The demonstration manifest and its checks (design §8.11.3, Req 33.2, 33.13, 28.11).

use crate::error::DemoError;
use crate::json::{array, boolean, check_relative, field, integer, parse_strict, string};
use rt_hash::{sha256, Sha256Hex};
use rt_json::{Kind, Position, Value};

/// What the harness keeps of a checked document entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestDocument {
    /// The document identifier.
    pub id: String,
    /// The path of the committed Source_Document, relative to the demonstration directory.
    pub path: String,
    /// Whether the document may be transmitted to an AI provider.
    pub transmission_permitted: bool,
}

/// A manifest that passed every check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    /// The demonstration name.
    pub name: String,
    /// The documents, in manifest order.
    pub documents: Vec<ManifestDocument>,
}

impl Manifest {
    /// Whether `ruletrace-ai` may be run for the document. Unknown documents are refused.
    #[must_use]
    pub fn may_transmit(&self, document_id: &str) -> bool {
        self.documents
            .iter()
            .any(|d| d.id == document_id && d.transmission_permitted)
    }

    /// Refuses a transmission for a document whose `transmission_permitted` is not `true`.
    ///
    /// # Errors
    /// [`DemoError::Manifest`] naming the document and the field.
    pub fn require_transmission(&self, document_id: &str) -> Result<(), DemoError> {
        if self.may_transmit(document_id) {
            Ok(())
        } else {
            Err(DemoError::Manifest {
                subject: document_id.to_owned(),
                field: "transmission_permitted".to_owned(),
                problem: "is not true (or the document is not in the manifest); \
                          the harness does not run ruletrace-ai for it"
                    .to_owned(),
            })
        }
    }
}

/// Strings every document entry must carry (non-empty).
const DOCUMENT_STRINGS: [&str; 10] = [
    "id",
    "path",
    "title",
    "publisher",
    "address",
    "retrieved",
    "licence",
    "licence_reference",
    "ud34_confirmation",
    "coverage_note",
];

fn err(subject: &str, field: &str, problem: String) -> DemoError {
    DemoError::Manifest {
        subject: subject.to_owned(),
        field: field.to_owned(),
        problem,
    }
}

/// Checks the recorded hash of a file against the file bytes.
fn check_hash(
    subject: &str,
    node: &Value<Position>,
    path: &str,
    prefix: &str,
    files: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<(), DemoError> {
    let key = format!("{prefix}sha256");
    let recorded = string(node, "sha256").map_err(|p| err(subject, &key, p))?;
    let recorded = Sha256Hex::parse(recorded).ok_or_else(|| {
        err(
            subject,
            &key,
            "is not 64 lowercase hexadecimal characters".to_owned(),
        )
    })?;
    check_relative(path).map_err(|p| err(subject, &format!("{prefix}path"), p))?;
    let bytes = files(path).ok_or_else(|| {
        err(
            subject,
            &format!("{prefix}path"),
            format!("file '{path}' cannot be read"),
        )
    })?;
    let actual = sha256(&bytes);
    if actual == recorded {
        Ok(())
    } else {
        Err(err(
            subject,
            &key,
            format!("recorded {recorded} but the file '{path}' hashes to {actual}"),
        ))
    }
}

fn check_original(subject: &str, doc: &Value<Position>) -> Result<(), DemoError> {
    let original = field(doc, "original").map_err(|p| err(subject, "original", p))?;
    if matches!(original.kind(), Kind::Null) {
        return Ok(());
    }
    let f = |name: &str, p: String| err(subject, &format!("original.{name}"), p);
    string(original, "format").map_err(|p| f("format", p))?;
    let hash = string(original, "sha256").map_err(|p| f("sha256", p))?;
    if Sha256Hex::parse(hash).is_none() {
        return Err(f(
            "sha256",
            "is not 64 lowercase hexadecimal characters".to_owned(),
        ));
    }
    let conversion = field(original, "conversion").map_err(|p| f("conversion", p))?;
    for key in ["method", "person", "date"] {
        string(conversion, key).map_err(|p| f(&format!("conversion.{key}"), p))?;
    }
    array(conversion, "differences").map_err(|p| f("conversion.differences", p))?;
    Ok(())
}

fn check_document(
    doc: &Value<Position>,
    files: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<ManifestDocument, DemoError> {
    let id = string(doc, "id").map_err(|p| err("(document without id)", "id", p))?;
    for key in DOCUMENT_STRINGS {
        string(doc, key).map_err(|p| err(id, key, p))?;
    }
    let permitted =
        boolean(doc, "transmission_permitted").map_err(|p| err(id, "transmission_permitted", p))?;
    check_original(id, doc)?;
    let path = string(doc, "path").map_err(|p| err(id, "path", p))?;
    check_hash(id, doc, path, "", files)?;
    Ok(ManifestDocument {
        id: id.to_owned(),
        path: path.to_owned(),
        transmission_permitted: permitted,
    })
}

fn check_data(
    entry: &Value<Position>,
    files: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<(), DemoError> {
    let path = string(entry, "path").map_err(|p| err("(data file without path)", "path", p))?;
    string(entry, "note").map_err(|p| err(path, "note", p))?;
    match string(entry, "nature").map_err(|p| err(path, "nature", p))? {
        "real" | "synthetic" => {}
        other => {
            return Err(err(
                path,
                "nature",
                format!("is '{other}', expected 'real' or 'synthetic'"),
            ))
        }
    }
    boolean(entry, "seeded_difference").map_err(|p| err(path, "seeded_difference", p))?;
    check_hash(path, entry, path, "", files)
}

fn check_script(
    script: &Value<Position>,
    files: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<(), DemoError> {
    let path = string(script, "path").map_err(|p| err("provider_script", "path", p))?;
    let origins = field(script, "origins").map_err(|p| err(path, "origins", p))?;
    for key in ["recorded", "authored"] {
        integer(origins, key).map_err(|p| err(path, &format!("origins.{key}"), p))?;
    }
    check_hash(path, script, path, "", files)
}

/// Checks a manifest. `files` returns the bytes of a file by its path relative to the
/// demonstration directory, or `None` when it cannot be read.
///
/// Every field of §8.11.3 must be present. A recorded hash must equal the file hash. A licence,
/// its reference and `transmission_permitted` must be recorded. A failure names the document
/// identifier (or the data or script path) and the field.
///
/// # Errors
/// The first failure, as [`DemoError::Manifest`].
pub fn check_manifest(
    bytes: &[u8],
    files: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<Manifest, DemoError> {
    let doc =
        parse_strict("manifest.json", bytes).map_err(|p| err("manifest.json", "(file)", p))?;
    let top = "manifest.json";
    if string(&doc, "format").map_err(|p| err(top, "format", p))? != "ruletrace-demo-manifest" {
        return Err(err(
            top,
            "format",
            "is not 'ruletrace-demo-manifest'".to_owned(),
        ));
    }
    if integer(&doc, "version").map_err(|p| err(top, "version", p))? != 1 {
        return Err(err(top, "version", "is not 1".to_owned()));
    }
    let name = string(&doc, "name").map_err(|p| err(top, "name", p))?;
    let mut documents = Vec::new();
    for entry in array(&doc, "documents").map_err(|p| err(top, "documents", p))? {
        documents.push(check_document(entry, files)?);
    }
    for entry in array(&doc, "data").map_err(|p| err(top, "data", p))? {
        check_data(entry, files)?;
    }
    let script = field(&doc, "provider_script").map_err(|p| err(top, "provider_script", p))?;
    // The `hand` profile has no Recorded_Provider_Script: the member is present and null.
    if !matches!(script.kind(), Kind::Null) {
        check_script(script, files)?;
    }
    Ok(Manifest {
        name: name.to_owned(),
        documents,
    })
}
