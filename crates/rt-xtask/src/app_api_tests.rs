//! Tests of the `app-api` rule. Every negative source is embedded here and
//! the expected lines are counted by hand. Each finding must name the rule,
//! the crate, the file and the line.

use super::*;

const LIST: &str = "tools/app-api-allow.v1";

fn workspace() -> BTreeSet<String> {
    [
        "rt-data",
        "rt-engine",
        "rt-numeric",
        "prize-check",
        "bracket-check",
        "rt-lab",
    ]
    .map(String::from)
    .into()
}

fn allow(lines: &[&str]) -> Vec<AllowEntry> {
    let mut text = format!("{HEADER}\n");
    for line in lines {
        text.push_str(line);
        text.push('\n');
    }
    parse_allow(&text, LIST).unwrap()
}

fn entry(krate: &str, path: &str) -> String {
    format!("allow {krate} {path} -- test reason")
}

fn run(krate: &str, body: &str, lines: &[String]) -> Vec<String> {
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let mut crates = BTreeMap::new();
    crates.insert(
        krate.to_string(),
        vec![("src/lib.rs".to_string(), body.to_string())],
    );
    check_api(&crates, &workspace(), &allow(&refs), LIST)
}

fn unlisted(krate: &str, line: usize, item: &str) -> String {
    format!("[app-api] {krate}: src/lib.rs:{line}: external item `{item}` is not listed in {LIST}")
}

#[test]
fn an_unlisted_simple_use_fails_on_its_line() {
    let found = run("prize-check", "fn f() {}\nuse rt_data::Question;\n", &[]);
    assert_eq!(found, vec![unlisted("prize-check", 2, "rt_data::Question")]);
}

#[test]
fn an_unlisted_item_in_a_grouped_use_fails_on_its_own_line() {
    let body = "use rt_data::{\n    Question,\n    tabular::{FileInputs, NamedFile},\n};\n";
    let lines = [
        entry("prize-check", "rt_data::Question"),
        entry("prize-check", "rt_data::tabular::FileInputs"),
    ];
    let found = run("prize-check", body, &lines);
    assert_eq!(
        found,
        vec![unlisted("prize-check", 3, "rt_data::tabular::NamedFile")]
    );
}

#[test]
fn a_group_with_self_checks_the_module_path() {
    let body = "use rt_data::tabular::{self, FileInputs};\n";
    let lines = [entry("prize-check", "rt_data::tabular::FileInputs")];
    let found = run("prize-check", body, &lines);
    assert_eq!(found, vec![unlisted("prize-check", 1, "rt_data::tabular")]);
}

#[test]
fn a_renamed_use_is_checked_under_its_original_path() {
    let body = "use rt_data::Question as Q;\n";
    let found = run("prize-check", body, &[]);
    assert_eq!(found, vec![unlisted("prize-check", 1, "rt_data::Question")]);
    let lines = [entry("prize-check", "rt_data::Question")];
    assert_eq!(run("prize-check", body, &lines), Vec::<String>::new());
    let alias_listed = [entry("prize-check", "rt_data::Q")];
    let found = run("prize-check", body, &alias_listed);
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(found[0], unlisted("prize-check", 1, "rt_data::Question"));
}

#[test]
fn a_glob_import_fails_and_cannot_be_listed() {
    let body = "fn f() {}\nuse rt_data::tabular::*;\n";
    let found = run("prize-check", body, &[]);
    assert_eq!(
        found,
        vec![
            "[app-api] prize-check: src/lib.rs:2: glob import of `rt_data::tabular::*` is not allowed"
                .to_string()
        ]
    );
    let errors = parse_allow(
        &format!("{HEADER}\nallow prize-check rt_data::tabular::* -- all of it\n"),
        LIST,
    )
    .unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].starts_with(&format!("[app-api] {LIST}:2: malformed path")));
}

#[test]
fn a_fully_qualified_call_in_an_expression_fails_on_its_line() {
    let body = "fn f() {\n    let e = rt_engine::evaluate(1);\n}\n";
    let found = run("prize-check", body, &[]);
    assert_eq!(
        found,
        vec![unlisted("prize-check", 2, "rt_engine::evaluate")]
    );
}

#[test]
fn a_fully_qualified_path_in_a_type_position_fails_on_its_line() {
    let body = "struct S {\n    q: Vec<rt_data::Question>,\n}\nfn f(x: &rt_numeric::Number) {}\n";
    let found = run("prize-check", body, &[]);
    assert_eq!(
        found,
        vec![
            unlisted("prize-check", 2, "rt_data::Question"),
            unlisted("prize-check", 4, "rt_numeric::Number"),
        ]
    );
}

#[test]
fn a_qualified_path_extending_a_listed_item_is_allowed_but_a_sibling_is_not() {
    let body = "fn f() {\n    rt_data::Question::new();\n    rt_data::QuestionX::new();\n}\n";
    let lines = [entry("prize-check", "rt_data::Question")];
    let found = run("prize-check", body, &lines);
    assert_eq!(
        found,
        vec![unlisted("prize-check", 3, "rt_data::QuestionX::new")]
    );
}

#[test]
fn a_use_must_equal_a_listed_item_exactly() {
    let body = "use rt_data::Question::new;\n";
    let lines = [entry("prize-check", "rt_data::Question")];
    let found = run("prize-check", body, &lines);
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(
        found[0],
        unlisted("prize-check", 1, "rt_data::Question::new")
    );
    assert!(found[1].contains("stale entry `rt_data::Question`"));
}

#[test]
fn an_item_listed_for_another_crate_does_not_allow_this_crate() {
    let mut crates = BTreeMap::new();
    crates.insert(
        "prize-check".to_string(),
        vec![(
            "src/lib.rs".to_string(),
            "use rt_data::Question;\n".to_string(),
        )],
    );
    crates.insert(
        "bracket-check".to_string(),
        vec![("src/lib.rs".to_string(), "fn f() {}\n".to_string())],
    );
    let entries = allow(&[&entry("bracket-check", "rt_data::Question")]);
    let found = check_api(&crates, &workspace(), &entries, LIST);
    assert_eq!(
        found,
        vec![
            unlisted("prize-check", 1, "rt_data::Question"),
            format!(
                "[app-api] bracket-check: {LIST}:2: stale entry `rt_data::Question`, no source uses it"
            ),
        ]
    );
}

#[test]
fn a_stale_entry_fails_with_its_list_line() {
    let lines = [
        entry("prize-check", "rt_data::Question"),
        entry("prize-check", "rt_data::TargetName"),
    ];
    let found = run("prize-check", "use rt_data::Question;\n", &lines);
    assert_eq!(
        found,
        vec![format!(
            "[app-api] prize-check: {LIST}:3: stale entry `rt_data::TargetName`, no source uses it"
        )]
    );
}

#[test]
fn an_entry_for_a_nonexistent_crate_fails() {
    let lines = [entry("no-such-crate", "rt_data::Question")];
    let found = run("prize-check", "fn f() {}\n", &lines);
    assert_eq!(
        found,
        vec![format!(
            "[app-api] no-such-crate: {LIST}:2: crate does not exist in the workspace"
        )]
    );
}

#[test]
fn an_entry_for_a_crate_that_is_not_scanned_fails() {
    let lines = [entry("rt-lab", "rt_data::Question")];
    let found = run("prize-check", "fn f() {}\n", &lines);
    assert_eq!(
        found,
        vec![format!(
            "[app-api] rt-lab: {LIST}:2: crate is not a checked application layer"
        )]
    );
}

#[test]
fn malformed_lines_report_their_line_number() {
    let text = format!(
        "{HEADER}\n\
         # comment\n\
         allow prize-check rt_data::Question\n\
         allow prize-check rt_data::Question --\n\
         allow prize-check rt_data::Question --   \n\
         permit prize-check rt_data::Question -- why\n\
         allow prize-check -- why\n\
         allow prize-check Question -- why\n\
         allow prize-check rt_data::{{A, B}} -- why\n\
         allow prize-check rt_data::A -- one\n\
         allow prize-check rt_data::A -- two\n"
    );
    let errors = parse_allow(&text, LIST).unwrap_err();
    let lines: Vec<&str> = errors.iter().map(String::as_str).collect();
    let prefix = |n: usize| format!("[app-api] {LIST}:{n}: ");
    assert_eq!(lines.len(), 8, "{lines:?}");
    assert!(lines[0].starts_with(&prefix(3)) && lines[0].ends_with("the reason is missing"));
    assert!(lines[1].starts_with(&prefix(4)) && lines[1].ends_with("the reason is missing"));
    assert!(lines[2].starts_with(&prefix(5)) && lines[2].ends_with("the reason is missing"));
    assert!(lines[3].starts_with(&prefix(6)) && lines[3].contains("must start with `allow`"));
    assert!(
        lines[4].starts_with(&prefix(7)) && lines[4].contains("expected `allow <crate> <path>")
    );
    assert!(lines[5].starts_with(&prefix(8)) && lines[5].contains("malformed path"));
    assert!(
        lines[6].starts_with(&prefix(9)) && lines[6].contains("expected `allow <crate> <path>")
    );
    assert_eq!(
        lines[7],
        format!("[app-api] {LIST}:11: duplicate entry `rt_data::A` for prize-check")
    );
}

#[test]
fn a_duplicate_entry_and_a_missing_header_are_refused() {
    let errors = parse_allow(
        &format!("{HEADER}\nallow a-b rt_data::A -- one\nallow a-b rt_data::A -- two\n"),
        LIST,
    )
    .unwrap_err();
    assert_eq!(
        errors,
        vec![format!(
            "[app-api] {LIST}:3: duplicate entry `rt_data::A` for a-b"
        )]
    );
    let errors = parse_allow("allow a-b rt_data::A -- one\n", LIST).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].starts_with(&format!("[app-api] {LIST}:1: first line must be")));
}

#[test]
fn internal_std_test_and_non_path_mentions_are_ignored() {
    let body = "use crate::x;\nuse super::y;\nuse self::z;\nuse std::fmt;\n\
                fn f() {\n    prize_check::g();\n    let s = \"rt_data::Question\";\n\
                \x20   let t = a.rt_data::b;\n    let rt_data = 1;\n}\n\
                #[cfg(test)]\nmod tests {\n    use rt_engine::evaluate;\n}\n";
    assert_eq!(run("prize-check", body, &[]), Vec::<String>::new());
}

#[test]
fn an_extern_crate_of_a_workspace_crate_fails() {
    let found = run("prize-check", "extern crate rt_data;\n", &[]);
    assert_eq!(found, vec![unlisted("prize-check", 1, "rt_data")]);
}

#[test]
fn another_application_crate_is_external() {
    let found = run(
        "bracket-check",
        "fn f() {\n    prize_check::go();\n}\n",
        &[],
    );
    assert_eq!(found, vec![unlisted("bracket-check", 2, "prize_check::go")]);
}

#[test]
fn a_laboratory_section_is_decided_by_what_is_not_listed() {
    let body = "use rt_data::Question;\nfn f() {\n    rt_engine::evaluate();\n}\n";
    let lines = [entry("rt-lab", "rt_data::Question")];
    let found = run("rt-lab", body, &lines);
    assert_eq!(found, vec![unlisted("rt-lab", 3, "rt_engine::evaluate")]);
    let both = [
        entry("rt-lab", "rt_data::Question"),
        entry("rt-lab", "rt_engine::evaluate"),
    ];
    assert_eq!(run("rt-lab", body, &both), Vec::<String>::new());
}

#[test]
fn a_test_module_file_is_not_scanned() {
    let mut crates = BTreeMap::new();
    crates.insert(
        "prize-check".to_string(),
        vec![
            (
                "src/lib.rs".to_string(),
                "#[cfg(test)]\nmod tests_x;\n".to_string(),
            ),
            (
                "src/tests_x.rs".to_string(),
                "use rt_engine::evaluate;\n".to_string(),
            ),
        ],
    );
    let found = check_api(&crates, &workspace(), &[], LIST);
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn a_leading_double_colon_path_in_an_expression_fails() {
    let body = "fn f() {\n    let _ = ::rt_engine::evaluate(1);\n}\n";
    let found = run("prize-check", body, &[]);
    assert_eq!(
        found,
        vec![unlisted("prize-check", 2, "rt_engine::evaluate")]
    );
}

#[test]
fn a_path_after_a_module_prefix_is_a_continuation() {
    let body = "fn f() {\n    let _ = local::rt_engine::evaluate(1);\n}\n";
    assert_eq!(run("prize-check", body, &[]), Vec::<String>::new());
}

#[test]
fn a_leading_double_colon_path_after_a_keyword_fails() {
    let keywords = [
        "as", "return", "if", "dyn", "impl", "in", "mut", "match", "else", "move", "break", "for",
        "while", "where", "let", "const", "static", "ref", "unsafe", "async", "await", "yield",
        "type", "fn", "struct", "enum", "pub", "use", "loop", "continue", "trait", "mod",
    ];
    for kw in keywords {
        let body = format!("fn f() {{\n    {kw} ::rt_engine::evaluate(1);\n}}\n");
        assert_eq!(
            run("prize-check", &body, &[]),
            vec![unlisted("prize-check", 2, "rt_engine::evaluate")],
            "keyword `{kw}`"
        );
    }
}
