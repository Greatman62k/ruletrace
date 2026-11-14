//! Diagnostic catalogue: reads `catalogue/diagnostics.v1.json`, validates it and renders the
//! generated `Code` enum of `rt-foundation`.
//!
//! The reader accepts only the catalogue shape (an array of flat objects whose values are strings
//! or non-negative integers). It is replaced by `rt-json` once that parser exists (task 2.7).

use std::fmt::Write as _;
use std::path::Path;

/// Catalogue file, relative to the repository root.
pub const CATALOGUE_PATH: &str = "catalogue/diagnostics.v1.json";
/// Generated file, relative to the repository root.
pub const GENERATED_PATH: &str = "crates/rt-foundation/src/code_generated.rs";

const SEVERITIES: [&str; 4] = ["Error", "Warning", "Ambiguity", "Unsupported"];
const EXITS: [(u32, &str); 7] = [
    (0, "Success"),
    (2, "InvalidInput"),
    (3, "Blocked"),
    (4, "Indeterminate"),
    (5, "Provider"),
    (6, "Response"),
    (70, "Internal"),
];

/// One catalogue row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// Identifier `RT-<family>-<4 digits>`.
    pub code: String,
    /// Mnemonic in upper snake case.
    pub mnemonic: String,
    /// One of Error, Warning, Ambiguity, Unsupported.
    pub severity: String,
    /// Meaning text.
    pub meaning: String,
    /// Exit status.
    pub exit: u32,
    /// Executables and commands that return the code.
    pub executable: String,
}

struct Reader {
    chars: Vec<char>,
    pos: usize,
}

impl Reader {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(' ' | '\n' | '\r' | '\t')) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, want: char) -> Result<(), String> {
        self.skip_ws();
        if self.peek() == Some(want) {
            self.pos += 1;
            Ok(())
        } else {
            Err(format!("expected `{want}` at character {}", self.pos))
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect('"')?;
        let mut out = String::new();
        loop {
            let c = self.peek().ok_or("unterminated string")?;
            self.pos += 1;
            match c {
                '"' => return Ok(out),
                '\\' => {
                    let e = self.peek().ok_or("unterminated escape")?;
                    self.pos += 1;
                    match e {
                        '"' | '\\' | '/' => out.push(e),
                        'n' => out.push('\n'),
                        't' => out.push('\t'),
                        'u' => {
                            let hex: String = self.chars.iter().skip(self.pos).take(4).collect();
                            let n = u32::from_str_radix(&hex, 16).map_err(|e| e.to_string())?;
                            out.push(char::from_u32(n).ok_or("bad \\u escape")?);
                            self.pos += 4;
                        }
                        other => return Err(format!("unsupported escape `\\{other}`")),
                    }
                }
                c if c.is_control() => return Err("control character in string".to_string()),
                c => out.push(c),
            }
        }
    }

    fn integer(&mut self) -> Result<u32, String> {
        let mut digits = String::new();
        while let Some(c @ '0'..='9') = self.peek() {
            digits.push(c);
            self.pos += 1;
        }
        digits
            .parse()
            .map_err(|_| "expected an integer".to_string())
    }
}

/// Parses the catalogue text.
///
/// # Errors
/// Returns a message for malformed text, a missing, unknown or repeated key, or a wrong value type.
pub fn parse(text: &str) -> Result<Vec<Row>, String> {
    let mut r = Reader {
        chars: text.chars().collect(),
        pos: 0,
    };
    let mut rows = Vec::new();
    r.expect('[')?;
    r.skip_ws();
    if r.peek() == Some(']') {
        r.pos += 1;
    } else {
        loop {
            rows.push(row(&mut r)?);
            r.skip_ws();
            match r.peek() {
                Some(',') => r.pos += 1,
                Some(']') => {
                    r.pos += 1;
                    break;
                }
                _ => return Err(format!("expected `,` or `]` at character {}", r.pos)),
            }
        }
    }
    r.skip_ws();
    if r.peek().is_some() {
        return Err("text after the catalogue array".to_string());
    }
    Ok(rows)
}

fn row(r: &mut Reader) -> Result<Row, String> {
    let (mut code, mut mnemonic, mut severity, mut meaning, mut exit, mut executable) =
        (None, None, None, None, None, None);
    r.expect('{')?;
    loop {
        let key = r.string()?;
        r.expect(':')?;
        r.skip_ws();
        let dup = || format!("repeated key `{key}`");
        match key.as_str() {
            "code" if code.is_none() => code = Some(r.string()?),
            "mnemonic" if mnemonic.is_none() => mnemonic = Some(r.string()?),
            "severity" if severity.is_none() => severity = Some(r.string()?),
            "meaning" if meaning.is_none() => meaning = Some(r.string()?),
            "exit" if exit.is_none() => exit = Some(r.integer()?),
            "executable" if executable.is_none() => executable = Some(r.string()?),
            "code" | "mnemonic" | "severity" | "meaning" | "exit" | "executable" => {
                return Err(dup())
            }
            other => return Err(format!("unknown key `{other}`")),
        }
        r.skip_ws();
        match r.peek() {
            Some(',') => r.pos += 1,
            Some('}') => {
                r.pos += 1;
                break;
            }
            _ => return Err(format!("expected `,` or `}}` at character {}", r.pos)),
        }
    }
    let missing = |name: &str| format!("row is missing `{name}`");
    Ok(Row {
        code: code.ok_or_else(|| missing("code"))?,
        mnemonic: mnemonic.ok_or_else(|| missing("mnemonic"))?,
        severity: severity.ok_or_else(|| missing("severity"))?,
        meaning: meaning.ok_or_else(|| missing("meaning"))?,
        exit: exit.ok_or_else(|| missing("exit"))?,
        executable: executable.ok_or_else(|| missing("executable"))?,
    })
}

fn valid_id(id: &str) -> bool {
    let Some(rest) = id.strip_prefix("RT-") else {
        return false;
    };
    let Some((family, number)) = rest.split_once('-') else {
        return false;
    };
    !family.is_empty()
        && family.bytes().all(|b| b.is_ascii_uppercase())
        && number.len() == 4
        && number.bytes().all(|b| b.is_ascii_digit())
}

/// Converts `UPPER_SNAKE` to `UpperCamel`.
#[must_use]
pub fn variant_name(mnemonic: &str) -> String {
    mnemonic
        .split('_')
        .map(|w| {
            let mut cs = w.chars();
            cs.next()
                .map(|f| f.to_ascii_uppercase().to_string() + &cs.as_str().to_ascii_lowercase())
                .unwrap_or_default()
        })
        .collect()
}

/// Checks the rows: identifier shape, unique identifiers, mnemonics and variant names, known
/// severity and exit status, non-empty text, and a Warning only with exit 0 (Req 7.19).
///
/// # Errors
/// Returns the first problem found, naming the code.
pub fn validate(rows: &[Row]) -> Result<(), String> {
    let mut ids = std::collections::BTreeSet::new();
    let mut mnemonics = std::collections::BTreeSet::new();
    let mut variants = std::collections::BTreeSet::new();
    for r in rows {
        let at = &r.code;
        if !valid_id(&r.code) {
            return Err(format!("{at}: identifier is not RT-<FAMILY>-<4 digits>"));
        }
        let mnemonic_ok = !r.mnemonic.is_empty()
            && r.mnemonic.split('_').all(|w| {
                !w.is_empty()
                    && w.bytes()
                        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
            })
            && r.mnemonic
                .bytes()
                .next()
                .is_some_and(|b| b.is_ascii_uppercase());
        if !mnemonic_ok {
            return Err(format!(
                "{at}: mnemonic `{}` is not UPPER_SNAKE",
                r.mnemonic
            ));
        }
        if !SEVERITIES.contains(&r.severity.as_str()) {
            return Err(format!("{at}: unknown severity `{}`", r.severity));
        }
        if !EXITS.iter().any(|(n, _)| *n == r.exit) {
            return Err(format!("{at}: unknown exit status {}", r.exit));
        }
        if r.severity == "Warning" && r.exit != 0 {
            return Err(format!("{at}: a Warning must have exit 0"));
        }
        if r.meaning.is_empty() || r.executable.is_empty() {
            return Err(format!("{at}: meaning and executable must not be empty"));
        }
        if r.meaning.contains('\n') {
            return Err(format!("{at}: meaning must be one line"));
        }
        if !ids.insert(r.code.clone()) {
            return Err(format!("{at}: identifier appears twice"));
        }
        if !mnemonics.insert(r.mnemonic.clone()) {
            return Err(format!("{at}: mnemonic `{}` appears twice", r.mnemonic));
        }
        if !variants.insert(variant_name(&r.mnemonic)) {
            return Err(format!(
                "{at}: mnemonic `{}` gives a duplicate variant",
                r.mnemonic
            ));
        }
    }
    Ok(())
}

fn exit_variant(exit: u32) -> &'static str {
    EXITS
        .iter()
        .find(|(n, _)| *n == exit)
        .map_or("Internal", |(_, name)| name)
}

/// Renders the generated source file. The output is formatted as `rustfmt` would format it.
///
/// # Errors
/// Returns the problem found by [`validate`].
pub fn render(rows: &[Row]) -> Result<String, String> {
    validate(rows)?;
    let mut o = String::new();
    let mut w = |s: &str| o.push_str(s);
    w("//! GENERATED by `cargo run -p rt-xtask -- gen-catalogue` from\n");
    w("//! `catalogue/diagnostics.v1.json`. Do not edit; `rt-xtask check-catalogue` compares it with a\n");
    w("//! fresh generation.\n\n");
    w("use crate::{ExitCode, Severity};\n\n");
    w("/// Every catalogue code. No other way to name a code exists.\n");
    w("#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]\n");
    w("pub enum Code {\n");
    for r in rows {
        let _ = writeln!(o, "    /// `{}`: {}", r.code, r.meaning);
        let _ = writeln!(o, "    {},", variant_name(&r.mnemonic));
    }
    o.push_str("}\n\nimpl Code {\n");
    o.push_str("    /// Every code, in catalogue order.\n");
    let _ = writeln!(o, "    pub const ALL: [Code; {}] = [", rows.len());
    for r in rows {
        let _ = writeln!(o, "        Code::{},", variant_name(&r.mnemonic));
    }
    o.push_str("    ];\n");
    let mut table = |doc: &str, sig: &str, arm: &dyn Fn(&Row) -> String| {
        let _ = writeln!(
            o,
            "\n    /// {doc}\n    #[must_use]\n    {sig} {{\n        match self {{"
        );
        for r in rows {
            let _ = writeln!(
                o,
                "            Self::{} => {},",
                variant_name(&r.mnemonic),
                arm(r)
            );
        }
        o.push_str("        }\n    }\n");
    };
    table(
        "The identifier, for example `RT-APR-0001`.",
        "pub fn id(self) -> &'static str",
        &|r| format!("\"{}\"", r.code),
    );
    table(
        "The mnemonic, for example `APPROVAL_UNRESOLVED`.",
        "pub fn mnemonic(self) -> &'static str",
        &|r| format!("\"{}\"", r.mnemonic),
    );
    table(
        "The severity, fixed by the catalogue.",
        "pub fn severity(self) -> Severity",
        &|r| format!("Severity::{}", r.severity),
    );
    table(
        "The exit status the catalogue assigns to the code.",
        "pub fn exit_code(self) -> ExitCode",
        &|r| format!("ExitCode::{}", exit_variant(r.exit)),
    );
    for (name, field, doc) in [
        ("from_id", "code", "The code with this identifier."),
        ("from_mnemonic", "mnemonic", "The code with this mnemonic."),
    ] {
        let _ = writeln!(
            o,
            "\n    /// {doc}\n    #[must_use]\n    pub fn {name}(text: &str) -> Option<Self> {{\n        match text {{"
        );
        for r in rows {
            let key = if field == "code" {
                &r.code
            } else {
                &r.mnemonic
            };
            let _ = writeln!(
                o,
                "            \"{key}\" => Some(Self::{}),",
                variant_name(&r.mnemonic)
            );
        }
        o.push_str("            _ => None,\n        }\n    }\n");
    }
    o.push_str("}\n");
    Ok(o)
}

/// Reads the catalogue under `root` and writes the generated file.
///
/// # Errors
/// Returns a message when a file cannot be read or written or the catalogue is invalid.
pub fn generate(root: &Path) -> Result<(), String> {
    let src = root.join(CATALOGUE_PATH);
    let text = std::fs::read_to_string(&src).map_err(|e| format!("{}: {e}", src.display()))?;
    let rows = parse(&text).map_err(|e| format!("{CATALOGUE_PATH}: {e}"))?;
    let out = render(&rows)?;
    let dst = root.join(GENERATED_PATH);
    std::fs::write(&dst, out).map_err(|e| format!("{}: {e}", dst.display()))
}

/// Baseline file, relative to the repository root (written by the first release, task 12.12).
pub const BASELINE_PATH: &str = "catalogue/baseline.v1.json";

/// Compares the current rows with the baseline rows (Req 7.11). Every baseline code must exist
/// in the current catalogue with the same severity and a byte-identical meaning. A baseline code
/// whose mnemonic now sits under another identifier is reported as renumbered.
#[must_use]
pub fn compare_baseline(baseline: &[Row], current: &[Row]) -> Vec<String> {
    let mut out = Vec::new();
    for b in baseline {
        match current.iter().find(|c| c.code == b.code) {
            None => match current.iter().find(|c| c.mnemonic == b.mnemonic) {
                Some(c) => out.push(format!(
                    "{}: renumbered to {} (baseline mnemonic {})",
                    b.code, c.code, b.mnemonic
                )),
                None => out.push(format!("{}: removed from the catalogue", b.code)),
            },
            Some(c) => {
                if c.severity != b.severity {
                    out.push(format!(
                        "{}: severity changed from {} to {}",
                        b.code, b.severity, c.severity
                    ));
                }
                if c.meaning != b.meaning {
                    out.push(format!("{}: meaning text changed", b.code));
                }
            }
        }
    }
    out
}

/// Pure part of `check-catalogue`. `enum_ids` are the identifiers of every `Code` variant in
/// declaration order, `generated` is the text of the checked-in generated file, `baseline` is the
/// baseline file text when it exists, and `release_tags` is the output of `git tag --list`.
/// Returns a one-line report.
///
/// # Errors
/// One line per finding.
pub fn check_texts(
    catalogue: &str,
    enum_ids: &[&str],
    generated: &str,
    baseline: Option<&str>,
    release_tags: &str,
) -> Result<String, String> {
    let rows = parse(catalogue).map_err(|e| format!("{CATALOGUE_PATH}: {e}"))?;
    let fresh = render(&rows)?;
    let mut findings = Vec::new();
    for r in &rows {
        if !enum_ids.contains(&r.code.as_str()) {
            findings.push(format!("{}: catalogue row has no `Code` variant", r.code));
        }
    }
    for id in enum_ids {
        if !rows.iter().any(|r| r.code == *id) {
            findings.push(format!("{id}: `Code` variant is not in {CATALOGUE_PATH}"));
        }
    }
    if fresh != generated {
        findings.push(format!(
            "{GENERATED_PATH} differs from a fresh generation; run `rt-xtask gen-catalogue`"
        ));
    }
    let mut note = String::new();
    match baseline {
        Some(text) => match parse(text) {
            Ok(base) => findings.extend(compare_baseline(&base, &rows)),
            Err(e) => findings.push(format!("{BASELINE_PATH}: {e}")),
        },
        None if release_tags.trim().is_empty() => {
            note = " (baseline skipped: no release tag exists, Req 7.12)".to_string();
        }
        None => findings.push(format!(
            "{BASELINE_PATH} is missing but release tags exist ({}); Req 7.12 requires it",
            release_tags
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
    if findings.is_empty() {
        Ok(format!("check-catalogue: ok ({} codes){note}", rows.len()))
    } else {
        Err(findings.join("\n"))
    }
}

/// `check-catalogue` on the repository at `root`. `release_tags` is the output of
/// `git tag --list`, passed by the caller so that no core code runs a process.
///
/// # Errors
/// One line per finding, or the file that could not be read.
pub fn check(root: &Path, release_tags: &str) -> Result<String, String> {
    let read = |rel: &str| {
        let path = root.join(rel);
        std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))
    };
    let catalogue = read(CATALOGUE_PATH)?;
    let generated = read(GENERATED_PATH)?;
    let baseline = if root.join(BASELINE_PATH).exists() {
        Some(read(BASELINE_PATH)?)
    } else {
        None
    };
    let ids: Vec<&str> = rt_foundation::Code::ALL.iter().map(|c| c.id()).collect();
    check_texts(
        &catalogue,
        &ids,
        &generated,
        baseline.as_deref(),
        release_tags,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rt_foundation::{Code, Severity};

    fn root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn rows() -> Vec<Row> {
        let text = std::fs::read_to_string(root().join(CATALOGUE_PATH)).unwrap();
        parse(&text).unwrap()
    }

    fn row(code: &str, mnemonic: &str, severity: &str, exit: u32) -> Row {
        Row {
            code: code.into(),
            mnemonic: mnemonic.into(),
            severity: severity.into(),
            meaning: "m".into(),
            exit,
            executable: "rt".into(),
        }
    }

    #[test]
    fn checked_in_catalogue_is_valid() {
        validate(&rows()).unwrap();
    }

    #[test]
    fn catalogue_and_enum_round_trip() {
        let rows = rows();
        assert_eq!(rows.len(), Code::ALL.len());
        for (r, code) in rows.iter().zip(Code::ALL) {
            assert_eq!(code.id(), r.code);
            assert_eq!(code.mnemonic(), r.mnemonic);
            assert_eq!(format!("{:?}", code.severity()), r.severity);
            assert_eq!(u32::from(code.exit_code().value()), r.exit);
            assert_eq!(Code::from_id(&r.code), Some(code));
            assert_eq!(Code::from_mnemonic(&r.mnemonic), Some(code));
            assert_eq!(code.to_string(), r.code);
        }
        assert_eq!(Code::from_id("RT-XXX-0001"), None);
        assert_eq!(Code::from_mnemonic("NOPE"), None);
    }

    #[test]
    fn checked_in_generated_file_is_fresh() {
        let on_disk = std::fs::read_to_string(root().join(GENERATED_PATH)).unwrap();
        assert_eq!(render(&rows()).unwrap(), on_disk);
    }

    #[test]
    fn named_codes_have_the_catalogue_properties() {
        assert_eq!(Code::ApprovalUnresolved.id(), "RT-APR-0001");
        assert_eq!(Code::ApprovalUnresolved.severity(), Severity::Error);
        assert_eq!(Code::ApprovalUnresolved.exit_code().value(), 3);
        assert_eq!(Code::RankReferenceCycle.id(), "RT-VAL-0023");
        assert_eq!(Code::AppTargetKindMismatch.id(), "RT-APP-0006");
        assert_eq!(Code::AiRunIncomplete.severity(), Severity::Warning);
        assert_eq!(Code::AiProviderTimeout.exit_code().value(), 0);
        assert!(Code::AiProviderAuthFailed.is_blocking());
        assert!(!Code::PublishedResultDiffers.is_blocking());
    }

    #[test]
    fn warnings_exit_zero_and_ids_are_unique() {
        let rows = rows();
        for r in rows.iter().filter(|r| r.severity == "Warning") {
            assert_eq!(r.exit, 0, "{}", r.code);
        }
        let mut ids: Vec<_> = rows.iter().map(|r| &r.code).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), rows.len());
    }

    #[test]
    fn validate_rejects_bad_rows() {
        let ok = row("RT-AI-0001", "A_B", "Error", 2);
        assert!(validate(std::slice::from_ref(&ok)).is_ok());
        let bad = [
            row("RT-AI-1", "A_B", "Error", 2),
            row("RT-AI-0001", "a_b", "Error", 2),
            row("RT-AI-0001", "A_B", "Fatal", 2),
            row("RT-AI-0001", "A_B", "Error", 9),
            row("RT-AI-0001", "A_B", "Warning", 2),
        ];
        for b in bad {
            assert!(validate(std::slice::from_ref(&b)).is_err(), "{b:?}");
        }
        let same_id = row("RT-AI-0001", "C", "Error", 2);
        assert!(validate(&[ok.clone(), same_id]).is_err());
        let same_mnemonic = row("RT-AI-0002", "A_B", "Error", 2);
        assert!(validate(&[ok, same_mnemonic]).is_err());
    }

    #[test]
    fn parser_is_strict() {
        let good = r#"[{"code": "RT-A-0001", "mnemonic": "A", "severity": "Error", "meaning": "x\"y", "exit": 2, "executable": "rt"}]"#;
        assert_eq!(parse(good).unwrap()[0].meaning, "x\"y");
        assert!(parse(&good.replace("\"exit\": 2, ", "")).is_err());
        assert!(parse(&good.replace("\"exit\": 2", "\"exit\": 2, \"exit\": 3")).is_err());
        assert!(parse(&good.replace("\"exe", "\"extra\": 1, \"exe")).is_err());
        assert!(parse(&format!("{good} x")).is_err());
        assert!(parse("[").is_err());
    }

    #[test]
    fn variant_names() {
        assert_eq!(variant_name("INGEST_NOT_UTF8"), "IngestNotUtf8");
        assert_eq!(variant_name("RANK_REFERENCE_CYCLE"), "RankReferenceCycle");
    }

    fn ids() -> Vec<&'static str> {
        Code::ALL.iter().map(|c| c.id()).collect()
    }

    fn texts() -> (String, String) {
        let root = root();
        (
            std::fs::read_to_string(root.join(CATALOGUE_PATH)).unwrap(),
            std::fs::read_to_string(root.join(GENERATED_PATH)).unwrap(),
        )
    }

    #[test]
    fn check_passes_on_the_workspace_and_skips_without_baseline() {
        let (c, g) = texts();
        let report = check_texts(&c, &ids(), &g, None, "").unwrap();
        assert!(report.contains("baseline skipped"), "{report}");
    }

    #[test]
    fn hand_edited_generated_enum_fails_freshness() {
        let (c, g) = texts();
        let edited = g.replacen("    IngestNotUtf8,", "    IngestNotUtf8Edited,", 1);
        assert_ne!(edited, g);
        let err = check_texts(&c, &ids(), &edited, None, "").unwrap_err();
        assert!(err.contains("differs from a fresh generation"), "{err}");
    }

    #[test]
    fn code_variant_missing_from_catalogue_is_named() {
        let (c, g) = texts();
        let mut ids = ids();
        ids.push("RT-ZZZ-9999");
        let err = check_texts(&c, &ids, &g, None, "").unwrap_err();
        assert!(err.contains("RT-ZZZ-9999"), "{err}");
    }

    #[test]
    fn baseline_detects_removed_renumbered_and_changed_codes() {
        let (c, g) = texts();
        let current = parse(&c).unwrap();
        let mut base = current.clone();
        base.push(row("RT-OLD-0001", "GONE_CODE", "Error", 2));
        let mut renum = row("RT-OLD-0002", &current[0].mnemonic, "Error", 2);
        renum.meaning = current[0].meaning.clone();
        base.push(renum);
        base[1].severity = "Warning".into();
        base[2].meaning = "different".into();
        let found = compare_baseline(&base, &current).join("\n");
        assert!(found.contains("RT-OLD-0001: removed"), "{found}");
        assert!(found.contains("RT-OLD-0002: renumbered to"), "{found}");
        assert!(found.contains("severity changed"), "{found}");
        assert!(found.contains("meaning text changed"), "{found}");
        assert!(compare_baseline(&current, &current).is_empty());
        // Through the whole check, with the baseline given as text.
        let mut text = String::from("[\n");
        for (i, b) in base.iter().enumerate() {
            let comma = if i + 1 < base.len() { "," } else { "" };
            text += &format!(
                "{{\"code\": \"{}\", \"mnemonic\": \"{}\", \"severity\": \"{}\", \"meaning\": \"{}\", \"exit\": {}, \"executable\": \"{}\"}}{comma}\n",
                b.code, b.mnemonic, b.severity, b.meaning, b.exit, b.executable
            );
        }
        text += "]";
        assert!(check_texts(&c, &ids(), &g, Some(&text), "").is_err());
    }

    #[test]
    fn missing_baseline_fails_once_a_release_tag_exists() {
        let (c, g) = texts();
        let err = check_texts(&c, &ids(), &g, None, "v1.0.0\n").unwrap_err();
        assert!(err.contains("baseline.v1.json is missing"), "{err}");
        assert!(check_texts(&c, &ids(), &g, Some(&c), "v1.0.0\n").is_ok());
    }
}
