//! The Coverage_Statement of the stand-in demonstration, `hand` profile (task 6.7).
//!
//! The inputs are the blessed artifacts under `golden/artifacts/`, which the runbook test checks
//! against the real binaries. Bless the coverage goldens with `RT_DEMO_BLESS=1`.

use rt_demo::coverage::{
    classify, parse_notes, print_json, print_markdown, CoverageClass, CoverageError, CoverageNote,
    CoverageNotes, CoverageStatement, PendingEntry, StoreBatches, StoreFinding,
};
use rt_demo::{check_golden, Mode};
use rt_foundation::{DocumentId, InputId};
use rt_ir::{parse_rulebook, Rulebook};
use rt_json::ParseLimits;
use rt_ledger::{parse_ledger, ApprovalLedger};
use rt_srcmodel::{
    default_load_limits, parse_candidates, CandidateClause, SourceModel, SourceModels,
};
use std::path::{Path, PathBuf};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn demo_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demo/stand-in-hand")
}

struct Inputs {
    clauses: Vec<CandidateClause>,
    rulebook: Rulebook,
    ledger: ApprovalLedger,
    models: SourceModels,
}

fn load() -> Result<Inputs, Box<dyn std::error::Error>> {
    let dir = demo_dir();
    let art = |p: &str| std::fs::read(dir.join(p));
    let limits = ParseLimits::new(8 * 1024 * 1024, 64);
    let input = |n: &str| InputId::new(n);
    let model = SourceModel::load(
        &input("source-model.json")?,
        &art("golden/artifacts/source-model.json")?,
        default_load_limits(),
    )
    .map_err(|d| d.message().to_owned())?;
    let clauses = parse_candidates(
        &input("candidates.json")?,
        &art("golden/artifacts/candidates.json")?,
        limits,
        &model,
    )?;
    let staged = parse_rulebook(
        &art("rulebook/hand-authored.json")?,
        &input("hand-authored.json")?,
        8 * 1024 * 1024,
        64,
    )
    .map_err(|_| "rulebook rejected")?;
    let (rulebook, _) = staged.into_parts();
    let ledger = parse_ledger(
        &input("ledger.jsonl")?,
        &art("golden/artifacts/ledger.jsonl")?,
        limits,
    )
    .map_err(|_| "ledger rejected")?;
    let mut models = SourceModels::new();
    models.insert(model).map_err(|_| "duplicate document")?;
    Ok(Inputs {
        clauses,
        rulebook,
        ledger,
        models,
    })
}

fn run(
    i: &Inputs,
    ledger: &ApprovalLedger,
    store: &StoreBatches,
    notes: &CoverageNotes,
) -> Result<CoverageStatement, CoverageError> {
    let validation = rt_validate::validate(&i.rulebook, ledger, &i.models, Some(&i.clauses));
    classify(&i.clauses, &i.rulebook, ledger, store, &validation, notes)
}

fn stand_in_notes() -> Result<CoverageNotes, Box<dyn std::error::Error>> {
    Ok(parse_notes(&std::fs::read(
        demo_dir().join("coverage-notes.json"),
    )?)?)
}

fn note_for(c: &CandidateClause) -> CoverageNote {
    CoverageNote {
        document: c.span().doc().clone(),
        start: c.span().start().get(),
        end: c.span().end().get(),
        reason: "authored test reason".to_owned(),
    }
}

#[test]
fn stand_in_statement_matches_its_golden_files() -> TestResult {
    let i = load()?;
    let st = run(&i, &i.ledger, &StoreBatches::default(), &stand_in_notes()?)?;
    assert_eq!(st.entries.len(), i.clauses.len());
    let starts: Vec<u32> = st
        .entries
        .iter()
        .map(|e| e.clause.span().start().get())
        .collect();
    assert!(starts.windows(2).all(|w| w[0] < w[1]));
    assert!(st
        .entries
        .iter()
        .all(|e| matches!(e.class, CoverageClass::RuleApproved { .. })));
    let bless = std::env::var("RT_DEMO_BLESS").ok();
    let ci = std::env::var("CI").ok();
    let mode = Mode::from_env_values(bless.as_deref(), ci.as_deref())?;
    let dir = demo_dir();
    check_golden(
        mode,
        "coverage",
        &dir,
        "golden/artifacts/coverage-statement.json",
        print_json(&st).as_bytes(),
    )?;
    check_golden(
        mode,
        "coverage",
        &dir,
        "golden/artifacts/coverage-statement.md",
        print_markdown(&st).as_bytes(),
    )?;
    Ok(())
}

#[test]
fn clause_without_rule_or_note_fails_naming_the_clause() -> TestResult {
    let i = load()?;
    let empty = ApprovalLedger::empty();
    let err = run(
        &i,
        &empty,
        &StoreBatches::default(),
        &CoverageNotes::default(),
    )
    .unwrap_err();
    let text = err.to_string();
    assert!(text.contains("'1.'") && text.contains("byte 161"), "{text}");
    Ok(())
}

#[test]
fn removing_a_notes_entry_fails_naming_that_clause() -> TestResult {
    let i = load()?;
    let empty = ApprovalLedger::empty();
    let all: Vec<CoverageNote> = i.clauses.iter().map(note_for).collect();
    let ok = run(
        &i,
        &empty,
        &StoreBatches::default(),
        &CoverageNotes::new(all.clone()),
    )?;
    assert!(ok
        .entries
        .iter()
        .all(|e| matches!(&e.class, CoverageClass::NotARule { .. })));
    let mut fewer = all;
    fewer.remove(1);
    let err = run(
        &i,
        &empty,
        &StoreBatches::default(),
        &CoverageNotes::new(fewer),
    )
    .unwrap_err();
    assert!(err.to_string().contains("'2.'"), "{err}");
    Ok(())
}

#[test]
fn store_findings_classify_before_notes_and_output_is_ordered() -> TestResult {
    let i = load()?;
    let empty = ApprovalLedger::empty();
    let doc: DocumentId = i.clauses[0].span().doc().clone();
    let entry = |c: &CandidateClause, finding| PendingEntry {
        document: doc.clone(),
        start: c.span().start().get(),
        end: c.span().end().get(),
        finding,
    };
    let store = StoreBatches {
        pending: vec![
            entry(
                &i.clauses[0],
                Some(StoreFinding::Unsupported {
                    capability: "rounding mode".to_owned(),
                }),
            ),
            entry(
                &i.clauses[1],
                Some(StoreFinding::Ambiguity {
                    interpretations: vec!["a".to_owned(), "b".to_owned()],
                }),
            ),
        ],
    };
    let notes = CoverageNotes::new(vec![note_for(&i.clauses[0]), note_for(&i.clauses[2])]);
    let reversed = Inputs {
        clauses: i.clauses.iter().rev().cloned().collect(),
        rulebook: i.rulebook.clone(),
        ledger: i.ledger.clone(),
        models: i.models.clone(),
    };
    let st = run(&reversed, &empty, &store, &notes)?;
    let names: Vec<&str> = st.entries.iter().map(|e| e.class.name()).collect();
    assert_eq!(names, ["unsupported", "ambiguity", "not_a_rule"]);
    Ok(())
}

#[test]
fn notes_file_rejects_a_non_authored_origin() {
    let text = br#"{"format":"ruletrace-coverage-notes","version":1,"notes":[
        {"document":"d","start":0,"end":3,"origin":"model","reason":"x"}]}"#;
    let err = parse_notes(text).unwrap_err();
    assert!(err.to_string().contains("notes[0].origin"), "{err}");
}
