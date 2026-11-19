//! Isolation of the two Workflow_Modules in one process (task 9.17,
//! Req 31.15 and 20.6).
//!
//! `prize_check::evaluate` runs over the committed `slice-basic`
//! Compiled_Rulebook and `bracket_check::evaluate` over the Compiled_Rulebook
//! of `app-rules.md`. They run in the orders Prize then Bracket, Bracket then
//! Prize, interleaved per participant record (every record a run of its own,
//! the two modules alternating), and on two threads at once. Each run must
//! equal the same run made alone: results bytes (`serialize_results`), the
//! human and JSON report bytes and the result class.
//!
//! The expected values do not come from an interleaved run. The solo runs are
//! the baseline, and a few Outcomes of the solo runs are pinned to values
//! derived by hand from the rules (see `support/app_runs.rs`).
//!
//! **Validates: Requirements 31.15, 20.6**

#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

#[path = "support/app_runs.rs"]
mod app_runs;

use app_runs::{
    all_jobs, baseline_child, boolean, missing, number, outcomes, record_jobs, render_job, run,
    standings, whole_jobs, App, Job, Sel, CSV_FULL, CSV_MISSING_AGE, CSV_MISSING_JUDGES, CSV_TIE,
};
use prize_check::ResultClass;
use std::collections::BTreeMap;

/// The bytes of every job as a fresh process that ran only that job's module
/// prints them.
fn solo_runs(_jobs: &[Job]) -> BTreeMap<String, Vec<u8>> {
    let mut solo = baseline_child(App::Prize);
    solo.extend(baseline_child(App::Bracket));
    solo
}

/// Runs the jobs in the given order and compares each (class, results, human
/// and JSON bytes) with its baseline from the fresh child.
fn assert_sequence_equals_solo(label: &str, sequence: &[&Job], solo: &BTreeMap<String, Vec<u8>>) {
    for (position, job) in sequence.iter().enumerate() {
        let got = render_job(job, &job.run());
        let want = &solo[&job.name];
        let at = format!("{label}, step {position}, job {}", job.name);
        assert!(
            &got == want,
            "bytes differ from the fresh-process baseline at byte {}: {at}",
            got.iter()
                .zip(want)
                .position(|(a, b)| a != b)
                .unwrap_or_else(|| got.len().min(want.len()))
        );
    }
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

fn solo_outcomes(app: App, csv: &str, sel: Sel) -> Vec<String> {
    outcomes(&run(app, csv, sel).results)
}

#[test]
fn solo_runs_have_the_hand_derived_outcomes_and_classes() {
    // Prize over slice-basic. eligible = age >= 16 and licence: p1 17 true,
    // p2 20 true, p3 15 false, p4 30 without licence false.
    assert_eq!(
        solo_outcomes(App::Prize, CSV_FULL, Sel::Eligibility),
        vec![boolean(true), boolean(true), boolean(false), boolean(false)]
    );
    // Scores 7.6, 7.8, 6 and 7.8, exactly.
    assert_eq!(
        solo_outcomes(App::Prize, CSV_FULL, Sel::Score),
        vec![number("7.6"), number("7.8"), number("6"), number("7.8")]
    );
    // p2 and p4 tie on 7.8 and judges 9 against 7 puts p4 first.
    assert_eq!(
        solo_outcomes(App::Prize, CSV_FULL, Sel::Ranking),
        vec![standings(&[
            (1, &["p4"]),
            (2, &["p2"]),
            (3, &["p1"]),
            (4, &["p3"])
        ])]
    );
    // Equal score and equal judges: one Tie_Group at 1, next position 3.
    assert_eq!(
        solo_outcomes(App::Prize, CSV_TIE, Sel::Ranking),
        vec![standings(&[(1, &["p2", "p4"]), (3, &["p1"]), (4, &["p3"])])]
    );
    // p2 has no age: its eligibility is Indeterminate and names the age.
    assert_eq!(
        solo_outcomes(App::Prize, CSV_MISSING_AGE, Sel::Eligibility),
        vec![
            boolean(true),
            missing("p2", "age"),
            boolean(false),
            boolean(false)
        ]
    );
    // The All request holds exactly those Outcomes of the three Targets.
    let mut union = solo_outcomes(App::Prize, CSV_FULL, Sel::Eligibility);
    union.extend(solo_outcomes(App::Prize, CSV_FULL, Sel::Score));
    union.extend(solo_outcomes(App::Prize, CSV_FULL, Sel::Ranking));
    assert_eq!(
        sorted(solo_outcomes(App::Prize, CSV_FULL, Sel::All)),
        sorted(union)
    );

    // Bracket over app-rules.md. qualified = score >= 7.7: p1 7.6 false,
    // p2 7.8 true, p3 6 false, p4 7.8 true.
    assert_eq!(
        solo_outcomes(App::Bracket, CSV_FULL, Sel::Qualification),
        vec![boolean(false), boolean(true), boolean(false), boolean(true)]
    );
    assert_eq!(
        solo_outcomes(App::Bracket, CSV_FULL, Sel::Ranking),
        vec![standings(&[
            (1, &["p4"]),
            (2, &["p2"]),
            (3, &["p1"]),
            (4, &["p3"])
        ])]
    );
    assert_eq!(
        solo_outcomes(App::Bracket, CSV_TIE, Sel::Ranking),
        vec![standings(&[(1, &["p2", "p4"]), (3, &["p1"]), (4, &["p3"])])]
    );

    // Exit classes: a complete run is Complete, a missing value makes the
    // run Indeterminate.
    assert_eq!(
        run(App::Prize, CSV_FULL, Sel::All).class,
        ResultClass::Complete
    );
    assert_eq!(
        run(App::Prize, CSV_MISSING_AGE, Sel::All).class,
        ResultClass::Indeterminate
    );
    assert_eq!(
        run(App::Bracket, CSV_FULL, Sel::All).class,
        ResultClass::Complete
    );
    assert_eq!(
        run(App::Bracket, CSV_MISSING_JUDGES, Sel::All).class,
        ResultClass::Indeterminate
    );
}

#[test]
fn the_fresh_process_baseline_reports_name_exactly_their_own_participants() {
    // Hand-derived: a report cites the participants of its own data file and
    // no others, and the data sets differ, so no two whole-file reports of a
    // module may be equal. A baseline that repeated the first report of a
    // module would fail both checks.
    let jobs = all_jobs();
    let solo = solo_runs(&jobs);
    for job in &jobs {
        let text = String::from_utf8(solo[&job.name].clone()).unwrap();
        let rows: Vec<&str> = job
            .csv
            .lines()
            .skip(1)
            .map(|r| r.split(',').next().unwrap())
            .collect();
        for id in ["p1", "p2", "p3", "p4"] {
            let cited = text.contains(&format!("\"{id}\""));
            assert_eq!(cited, rows.contains(&id), "{} cites {id}: {text}", job.name);
        }
    }
    for app in [App::Prize, App::Bracket] {
        let wholes = whole_jobs(app);
        for (i, a) in wholes.iter().enumerate() {
            for b in &wholes[i + 1..] {
                assert_ne!(solo[&a.name], solo[&b.name], "{} equals {}", a.name, b.name);
            }
        }
    }
}

#[test]
fn the_two_modules_run_over_different_rulebooks() {
    let prize = run(App::Prize, CSV_FULL, Sel::All);
    let bracket = run(App::Bracket, CSV_FULL, Sel::All);
    // The document each rulebook cites differs, so the reports cannot be
    // equal by accident of one shared rulebook.
    assert!(bracket.human.contains("app-rules.md"), "{}", bracket.human);
    assert!(!prize.human.contains("app-rules.md"), "{}", prize.human);
    assert_ne!(prize.json, bracket.json);
}

#[test]
fn prize_then_bracket_and_bracket_then_prize_equal_the_solo_runs() {
    let jobs = all_jobs();
    let solo = solo_runs(&jobs);
    let prize: Vec<&Job> = jobs.iter().filter(|j| j.app == App::Prize).collect();
    let bracket: Vec<&Job> = jobs.iter().filter(|j| j.app == App::Bracket).collect();
    let a_then_b: Vec<&Job> = prize.iter().chain(&bracket).copied().collect();
    assert_sequence_equals_solo("prize then bracket", &a_then_b, &solo);
    let b_then_a: Vec<&Job> = bracket.iter().chain(&prize).copied().collect();
    assert_sequence_equals_solo("bracket then prize", &b_then_a, &solo);
    // A third pass in the first order: a second run after both must not
    // differ from the first.
    assert_sequence_equals_solo("prize then bracket again", &a_then_b, &solo);
}

#[test]
fn runs_interleaved_per_participant_record_equal_the_solo_runs() {
    let jobs = all_jobs();
    let solo = solo_runs(&jobs);
    let prize = record_jobs(App::Prize);
    let bracket = record_jobs(App::Bracket);
    assert_eq!(prize.len(), 4);
    assert_eq!(bracket.len(), 4);
    let wholes_p = whole_jobs(App::Prize);
    let wholes_b = whole_jobs(App::Bracket);

    // Prize record i, then Bracket record i.
    let mut forward: Vec<&Job> = Vec::new();
    for i in 0..4 {
        forward.push(&prize[i]);
        forward.push(&bracket[i]);
    }
    assert_sequence_equals_solo("interleaved, prize first", &forward, &solo);

    // Bracket record i, then Prize record i, the records in reverse.
    let mut backward: Vec<&Job> = Vec::new();
    for i in (0..4).rev() {
        backward.push(&bracket[i]);
        backward.push(&prize[i]);
    }
    assert_sequence_equals_solo("interleaved, bracket first, reversed", &backward, &solo);

    // Whole data sets between the single-record runs of the other module.
    let mut mixed: Vec<&Job> = Vec::new();
    for i in 0..3 {
        mixed.push(&wholes_p[i]);
        mixed.push(&bracket[i]);
        mixed.push(&wholes_b[i]);
        mixed.push(&prize[i]);
    }
    assert_sequence_equals_solo("whole sets between records", &mixed, &solo);
}

#[test]
fn single_record_runs_equal_the_matching_row_of_hand_derived_scores() {
    // The per-record data sets are one participant each. Their solo Outcomes
    // are pinned independently of any interleaved run: record i of the full
    // data set has the eligibility and score derived in the first test.
    let eligible = [true, true, false, false];
    let scores = ["7.6", "7.8", "6", "7.8"];
    let qualified = [false, true, false, true];
    for (i, csv) in app_runs::single_record_files(CSV_FULL).iter().enumerate() {
        assert_eq!(
            solo_outcomes(App::Prize, csv, Sel::Eligibility),
            vec![boolean(eligible[i])],
            "prize record {i}"
        );
        assert_eq!(
            solo_outcomes(App::Prize, csv, Sel::Score),
            vec![number(scores[i])],
            "prize record {i}"
        );
        assert_eq!(
            solo_outcomes(App::Bracket, csv, Sel::Qualification),
            vec![boolean(qualified[i])],
            "bracket record {i}"
        );
    }
}

#[test]
fn two_threads_running_the_modules_at_once_equal_the_solo_runs() {
    let jobs = all_jobs();
    let solo = solo_runs(&jobs);
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        for app in [App::Prize, App::Bracket] {
            let (jobs, solo, barrier) = (&jobs, &solo, &barrier);
            scope.spawn(move || {
                let own: Vec<&Job> = jobs.iter().filter(|j| j.app == app).collect();
                barrier.wait();
                for round in 0..10 {
                    assert_sequence_equals_solo(
                        &format!("{app:?} thread, round {round}"),
                        &own,
                        solo,
                    );
                }
            });
        }
    });
}

/// The child of the fresh-process baseline. Without the marker it does
/// nothing.
#[test]
fn child_entry() {
    if std::env::var(app_runs::CHILD_MARKER).as_deref() == Ok("baseline") {
        app_runs::print_baseline_frames();
    }
}
