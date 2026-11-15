//! Fixture documents and payload helpers shared by the AI tests.
//!
//! Hashes come from `sha256sum` of the exact bytes below.

#![allow(dead_code)]

use rt_foundation::{DocumentId, InputId};
use rt_srcmodel::{DocumentKind, Element, ElementKind, SourceModel, SourceModels};

// Bytes: "Alpha beta." 0..11, LF at 11, "Gamma δ delta." 12..27 with δ at
// 18..20, LF at 27, "End." 28..32. Length 32.
pub const RULES: &str = "Alpha beta.\nGamma δ delta.\nEnd.";
pub const RULES_HASH: &str = "f7090ac9560b215b913bce74610d0d305454076f8a40034b0c160d0b16bccec5";
// Bytes: C a f é(2) space o k . = 9.
pub const CAFE: &str = "Café ok.";
pub const CAFE_HASH: &str = "bd83ae62d79924ec08a0d4af3aaf5e61650c4fa753508ec1af67ad94e1bc327c";

pub const LIMIT: u64 = 1 << 20;

pub fn doc(name: &str) -> DocumentId {
    DocumentId::new(name).unwrap()
}

pub fn rid() -> InputId {
    InputId::new("response-1").unwrap()
}

pub fn model(name: &str, text: &str) -> SourceModel {
    let end = u32::try_from(text.len()).unwrap();
    let whole = Element {
        id: 0,
        kind: ElementKind::Paragraph,
        level: None,
        label: None,
        start: 0,
        end,
        line: 1,
        column: 1,
        parent: None,
    };
    SourceModel::new(doc(name), DocumentKind::Text, text.to_owned(), vec![whole]).unwrap()
}

pub fn models() -> SourceModels {
    let mut all = SourceModels::new();
    all.insert(model("rules.md", RULES)).unwrap();
    all.insert(model("cafe.md", CAFE)).unwrap();
    all
}

pub fn escaped(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\u{0}', "\\u0000")
}

pub fn wrap(proposals: &[String]) -> String {
    format!(
        r#"{{"schema_version":"1","proposals":[{}]}}"#,
        proposals.join(",")
    )
}
