use super::*;
use rt_hash::sha256;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn files() -> BTreeMap<String, Vec<u8>> {
    let mut m = BTreeMap::new();
    m.insert("documents/d1.md".to_owned(), b"# Rules\n".to_vec());
    m.insert("data/p.csv".to_owned(), b"a,b\n".to_vec());
    m.insert("provider/s.json".to_owned(), b"{}".to_vec());
    m
}

fn manifest_text(files: &BTreeMap<String, Vec<u8>>, doc_extra: &str) -> String {
    let h = |p: &str| sha256(files.get(p).map_or(&[][..], Vec::as_slice)).to_string();
    format!(
        r#"{{"format":"ruletrace-demo-manifest","version":1,"name":"n",
"documents":[{{"id":"d1","path":"documents/d1.md","title":"t","publisher":"p","address":"https://x",
"retrieved":"r","sha256":"{}","licence_reference":"l","transmission_permitted":true,
"ud34_confirmation":"u","original":null,"coverage_note":"c"{doc_extra}}}],
"data":[{{"path":"data/p.csv","sha256":"{}","nature":"synthetic","note":"n","seeded_difference":false}}],
"provider_script":{{"path":"provider/s.json","sha256":"{}","origins":{{"recorded":0,"authored":1}}}}}}"#,
        h("documents/d1.md"),
        h("data/p.csv"),
        h("provider/s.json")
    )
}

fn check(text: &str, files: &BTreeMap<String, Vec<u8>>) -> Result<Manifest, DemoError> {
    check_manifest(text.as_bytes(), &|p| files.get(p).cloned())
}

#[test]
fn manifest_accepts_a_complete_entry() {
    let f = files();
    let m = check(&manifest_text(&f, r#","licence":"CC-BY""#), &f).unwrap();
    assert!(m.may_transmit("d1"));
    assert!(m.require_transmission("other").is_err());
}

#[test]
fn manifest_accepts_a_null_provider_script_for_the_hand_profile() {
    let f = files();
    let text = manifest_text(&f, r#","licence":"CC-BY""#);
    let start = text.find("\"provider_script\":").unwrap();
    let text = format!("{}\"provider_script\":null}}", &text[..start]);
    assert!(check(&text, &f).is_ok());
}

#[test]
fn manifest_missing_licence_names_document_and_field() {
    let f = files();
    let e = check(&manifest_text(&f, ""), &f).unwrap_err();
    assert_eq!(
        e,
        DemoError::Manifest {
            subject: "d1".into(),
            field: "licence".into(),
            problem: "field is missing".into()
        }
    );
    assert!(e.to_string().contains("'d1'") && e.to_string().contains("'licence'"));
    let blank = check(&manifest_text(&f, r#","licence":"  ""#), &f).unwrap_err();
    assert!(matches!(blank, DemoError::Manifest { field, .. } if field == "licence"));
}

#[test]
fn manifest_hash_mismatch_and_missing_permission_are_reported() {
    let mut f = files();
    let text = manifest_text(&f, r#","licence":"x""#);
    f.insert("documents/d1.md".into(), b"# Rules!\n".to_vec());
    let e = check(&text, &f).unwrap_err();
    assert!(matches!(&e, DemoError::Manifest { subject, field, .. }
        if subject == "d1" && field == "sha256"));
    let f = files();
    let text =
        manifest_text(&f, r#","licence":"x""#).replace("\"transmission_permitted\":true,", "");
    let e = check(&text, &f).unwrap_err();
    assert!(matches!(e, DemoError::Manifest { field, .. } if field == "transmission_permitted"));
}

#[test]
fn manifest_rejects_flag_keys_and_escaping_paths() {
    let f = files();
    let text = manifest_text(&f, r#","licence":"x","has_errors":false"#);
    assert!(check(&text, &f).is_err());
    let text = manifest_text(&f, r#","licence":"x""#).replace("documents/d1.md", "../d1.md");
    assert!(check(&text, &f).is_err());
}

#[test]
fn transmission_not_true_is_refused() {
    let f = files();
    let text = manifest_text(&f, r#","licence":"x""#).replace(
        "\"transmission_permitted\":true",
        "\"transmission_permitted\":false",
    );
    let m = check(&text, &f).unwrap();
    let e = m.require_transmission("d1").unwrap_err();
    assert!(e.to_string().contains("transmission_permitted"));
}

const STEP: &str = r#"{"id":"s01","argv":["ruletrace","ingest","--out","work/o.json"],"expect_exit":0,
"stdout_golden":"golden/steps/s01.stdout","stderr_golden":"golden/steps/s01.stderr",
"outputs":[{"path":"work/o.json","golden":"golden/artifacts/o.json"}]}"#;

fn runbook_md(extra: &str) -> String {
    format!(
        "# Demo\n\n```text live\nruletrace-ai propose --config x\n```\n{extra}\n```ruletrace-runbook\n\
         {{\"format\":\"ruletrace-runbook\",\"version\":1,\"profiles\":{{\"hand\":{{}}}},\"steps\":[{STEP}]}}\n```\n"
    )
}

#[test]
fn runbook_parses_and_live_blocks_are_allowed() {
    let rb = parse_runbook(&runbook_md("")).unwrap();
    assert_eq!(rb.steps.len(), 1);
    assert_eq!(rb.steps_for("hand").unwrap().len(), 1);
    assert!(rb.steps_for("ai").is_err());
}

#[test]
fn runbook_fails_on_an_unmarked_command_line() {
    for line in [
        "ruletrace validate --x",
        "$ prize-check a",
        "  bracket-check",
        "ruletrace-ai propose",
    ] {
        let md = runbook_md(&format!("\n```sh\n# comment\n{line}\n```\n"));
        let e = parse_runbook(&md).unwrap_err().to_string();
        assert!(e.contains("unmarked command"), "{line}: {e}");
    }
    // A longer name is not a RuleTrace command, and prose outside fences is
    // ignored.
    let md = runbook_md("\nruletrace validate\n```sh\nruletrace-extra x\n```\n");
    assert!(parse_runbook(&md).is_ok());
}

#[test]
fn runbook_requires_exactly_one_block_and_relative_arguments() {
    assert!(parse_runbook("# none\n").is_err());
    let md = runbook_md("");
    assert!(parse_runbook(&format!("{md}{md}")).is_err());
    let abs = runbook_md("").replace("work/o.json\"]", "/tmp/o.json\"]");
    assert!(parse_runbook(&abs)
        .unwrap_err()
        .to_string()
        .contains("not relative"));
    let unknown = runbook_md("").replace("\"ruletrace\",\"ingest\"", "\"sh\",\"ingest\"");
    assert!(parse_runbook(&unknown).is_err());
    let open = "```ruletrace-runbook\n{}\n";
    assert!(parse_runbook(open)
        .unwrap_err()
        .to_string()
        .contains("not closed"));
}

#[test]
fn first_difference_reports_offset() {
    assert_eq!(first_difference(b"abc", b"abc"), None);
    assert_eq!(first_difference(b"abc", b"abd"), Some(2));
    assert_eq!(first_difference(b"abc", b"abcd"), Some(3));
    assert_eq!(first_difference(b"", b"x"), Some(0));
    assert_eq!(first_difference(b"xbc", b"abc"), Some(0));
}

#[test]
fn bless_is_refused_in_ci() {
    assert_eq!(Mode::from_env_values(None, None), Ok(Mode::Compare));
    assert_eq!(Mode::from_env_values(Some("0"), None), Ok(Mode::Compare));
    assert_eq!(Mode::from_env_values(Some("1"), None), Ok(Mode::Bless));
    assert_eq!(
        Mode::from_env_values(Some("1"), Some("true")),
        Err(DemoError::BlessRefused)
    );
    assert_eq!(
        Mode::from_env_values(Some("1"), Some("")),
        Err(DemoError::BlessRefused)
    );
}

fn temp(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rt-demo-test-{}-{label}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn golden_one_byte_difference_reports_step_file_and_offset() {
    let dir = temp("golden");
    check_golden(Mode::Bless, "s1", &dir, "golden/a.txt", b"hello world").unwrap();
    check_golden(Mode::Compare, "s1", &dir, "golden/a.txt", b"hello world").unwrap();
    let e = check_golden(Mode::Compare, "s1", &dir, "golden/a.txt", b"hello World").unwrap_err();
    assert_eq!(
        e,
        DemoError::Golden {
            step: "s1".into(),
            golden: "golden/a.txt".into(),
            offset: 6,
            problem: "produced 11 bytes, golden has 11 bytes".into()
        }
    );
    let missing = check_golden(Mode::Compare, "s1", &dir, "golden/none", b"x").unwrap_err();
    assert!(matches!(missing, DemoError::Golden { offset: 0, .. }));
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn runbook_runs_with_cleared_environment_and_compares_goldens() {
    use std::os::unix::fs::PermissionsExt;
    let root = temp("run");
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    // Stub of the logical executable: prints its argument count and the
    // environment size.
    let script = "#!/bin/sh\nmkdir -p work\nprintf 'out' > work/o.json\n\
                  echo \"args=$# env=$(env | wc -l)\"\nexit 0\n";
    let exe = binary_path(&bin, "ruletrace");
    std::fs::write(&exe, script).unwrap();
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    let demo = root.join("demo");
    std::fs::create_dir_all(&demo).unwrap();
    let rb = parse_runbook(&runbook_md("")).unwrap();
    let work = root.join("work-copy");
    // First bless the goldens, then compare. `env` itself may add `PWD`/`_`, so
    // the golden is blessed from the same run mechanism.
    run_runbook(&rb, "hand", &demo, &work, &bin, Mode::Bless).unwrap();
    let out = std::fs::read_to_string(demo.join("golden/steps/s01.stdout")).unwrap();
    assert!(out.starts_with("args=3 env="), "{out}");
    let _ = std::fs::remove_dir_all(&work);
    run_runbook(&rb, "hand", &demo, &work, &bin, Mode::Compare).unwrap();
    // A one-byte change in the artifact golden is reported with its offset.
    std::fs::write(demo.join("golden/artifacts/o.json"), b"oXt").unwrap();
    let _ = std::fs::remove_dir_all(&work);
    let e = run_runbook(&rb, "hand", &demo, &work, &bin, Mode::Compare).unwrap_err();
    assert!(
        matches!(&e, DemoError::Golden { step, golden, offset: 1, .. }
        if step == "s01" && golden == "golden/artifacts/o.json"),
        "{e}"
    );
    // A wrong declared exit code names the step.
    let mut wrong = rb.clone();
    wrong.steps[0].expect_exit = 4;
    let _ = std::fs::remove_dir_all(&work);
    let e = run_runbook(&wrong, "hand", &demo, &work, &bin, Mode::Compare).unwrap_err();
    assert!(matches!(e, DemoError::Step { step, .. } if step == "s01"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_windows_variant_replaces_its_golden_file_on_windows_only() {
    let dir = temp("variant");
    let steps = dir.join("golden").join("steps");
    std::fs::create_dir_all(&steps).unwrap();
    std::fs::write(steps.join("s.stdout"), b"base").unwrap();
    std::fs::write(steps.join("s.stdout.windows"), b"windows").unwrap();
    let (applies, other): (&[u8], &[u8]) = if cfg!(windows) {
        (b"windows", b"base")
    } else {
        (b"base", b"windows")
    };
    let golden = "golden/steps/s.stdout";
    check_golden(Mode::Compare, "s", &dir, golden, applies).unwrap();
    let e = check_golden(Mode::Compare, "s", &dir, golden, other).unwrap_err();
    assert!(matches!(e, DemoError::Golden { offset: 0, .. }), "{e}");
    // A golden file without a variant is used on every platform.
    std::fs::write(steps.join("t.stdout"), b"same").unwrap();
    check_golden(Mode::Compare, "t", &dir, "golden/steps/t.stdout", b"same").unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
