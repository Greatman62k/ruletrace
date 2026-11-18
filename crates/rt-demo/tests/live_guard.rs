//! The live-step guard of the demonstration runner (task 10.7; design §8.11.6;
//! Req 33.10, 28.21, 28.11, 33.2).
//!
//! Every expectation is written by hand. The refusal tests give the runner a
//! `ruletrace-ai` that would leave a marker file and connect to a loopback
//! listener if it were ever started. The positive control runs the same fake
//! for a permitted offline step, so the marker and the connection can appear.
//! No real `ruletrace-ai`, credential or provider is used.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    missing_docs
)]

use rt_demo::{
    check_manifest, classify, copy_demo, parse_runbook, run_live_step, run_step, Class, DemoError,
    LiveContext, Manifest, ManifestDocument, Refusal, Step,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

const DOC: &str = "stand-in-rules.md";
const OTHER: &str = "other-rules.md";

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("rt-live-guard-{label}-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn repo_demo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../demo/stand-in-ai")
        .canonicalize()
        .unwrap()
}

fn manifest(documents: &[(&str, bool)]) -> Manifest {
    Manifest {
        name: "test".to_owned(),
        documents: documents
            .iter()
            .map(|(id, permitted)| ManifestDocument {
                id: (*id).to_owned(),
                path: format!("documents/{id}"),
                transmission_permitted: *permitted,
            })
            .collect(),
    }
}

/// A demonstration directory, its work copy and a directory of fake binaries.
struct Fixture {
    _root: TempDir,
    demo: PathBuf,
    work: PathBuf,
    bin: PathBuf,
}

impl Fixture {
    /// `transmission_log` replaces the log path of the shipped Meta example.
    fn new(label: &str, transmission_log: &str) -> Self {
        let root = TempDir::new(label);
        let demo = root.0.join("demo");
        let work = root.0.join("work");
        let bin = root.0.join("bin");
        for dir in ["ai", "golden/steps", "provider", "documents"] {
            fs::create_dir_all(demo.join(dir)).unwrap();
        }
        fs::create_dir_all(&bin).unwrap();
        let real = repo_demo();
        let example = fs::read_to_string(real.join("ai/meta.example.toml")).unwrap();
        let live = example.replace(
            "transmission_log = \"transmission.log.jsonl\"",
            &format!("transmission_log = \"{transmission_log}\""),
        );
        assert!(live.contains(&format!("transmission_log = \"{transmission_log}\"")));
        fs::write(demo.join("ai/live.toml"), live).unwrap();
        fs::copy(
            real.join("ai/mock-replay.toml"),
            demo.join("ai/mock-replay.toml"),
        )
        .unwrap();
        fs::write(demo.join("golden/steps/s02.stdout"), b"golden bytes").unwrap();
        fs::write(demo.join("provider/script.json"), b"{}").unwrap();
        copy_demo(&demo, &work).unwrap();
        Self {
            _root: root,
            demo,
            work,
            bin,
        }
    }

    fn live_step(&self, extra: &[&str]) -> Step {
        let mut argv: Vec<String> = [
            "ruletrace-ai",
            "propose",
            "--config",
            "ai/live.toml",
            "--rulebook",
            "rulebook/seed.json",
            "--source-model",
            "work/source-model.json",
            "--candidates",
            "work/candidates.json",
            "--created-at",
            "2026-01-15T09:00:00Z",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
        argv.extend(extra.iter().map(|s| (*s).to_owned()));
        step(argv)
    }

    fn context<'a>(&'a self, manifest: &'a Manifest) -> LiveContext<'a> {
        LiveContext {
            manifest,
            demo_dir: &self.demo,
            environment: &[],
        }
    }

    fn written(&self) -> Vec<String> {
        let mut out = Vec::new();
        collect(&self.work, &self.work, &mut out);
        out.sort();
        out
    }
}

fn collect(base: &Path, dir: &Path, out: &mut Vec<String>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(base, &path, out);
        } else {
            out.push(
                path.strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
}

fn step(argv: Vec<String>) -> Step {
    Step {
        id: "live".to_owned(),
        profiles: Vec::new(),
        argv,
        expect_exit: 0,
        cwd: ".".to_owned(),
        stdout_golden: String::new(),
        stderr_golden: String::new(),
        outputs: Vec::new(),
    }
}

fn permission_error(subject: &str) -> DemoError {
    DemoError::Manifest {
        subject: subject.to_owned(),
        field: "transmission_permitted".to_owned(),
        problem: "is not true (or the document is not in the manifest); the harness does not \
                  run ruletrace-ai for it"
            .to_owned(),
    }
}

fn refused(step: &Step, refusal: Refusal) -> DemoError {
    DemoError::LiveRefused {
        step: step.id.clone(),
        refusal,
    }
}

#[cfg(unix)]
mod fake {
    use super::*;
    use std::net::TcpListener;

    /// Writes an executable script from a short-lived child process.
    ///
    /// A write descriptor opened in this process is inherited by any child
    /// that another test thread forks, and a file with an open writer cannot
    /// be executed (ETXTBSY). The writer descriptor lives only in the child,
    /// which has exited before this returns. Linking or renaming would not
    /// help, because the restriction is on the inode.
    pub fn write_executable(exe: &Path, script: &str) {
        let status = std::process::Command::new("/bin/sh")
            .args([
                "-c",
                "printf '%s' \"$1\" > \"$2\" && chmod 755 \"$2\"",
                "sh",
            ])
            .arg(script)
            .arg(exe)
            .status()
            .unwrap();
        assert!(status.success(), "writing {} failed", exe.display());
    }

    /// A `ruletrace-ai` that records that it ran and opens one loopback
    /// connection. It is bash because `/dev/tcp` needs it.
    pub struct Probe {
        pub marker: PathBuf,
        listener: TcpListener,
    }

    impl Probe {
        pub fn install(bin: &Path) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let port = listener.local_addr().unwrap().port();
            let marker = bin.join("MARKER");
            let script = format!(
                "#!/bin/bash\nprintf '%s/%s' \"${{MODEL_API_KEY-unset}}\" \"${{OPENAI_API_KEY-unset}}\" > '{}'\n\
                 exec 3<>/dev/tcp/127.0.0.1/{port}\n",
                marker.display()
            );
            write_executable(&bin.join("ruletrace-ai"), &script);
            Self { marker, listener }
        }

        pub fn connections(&self) -> usize {
            let mut n = 0;
            while self.listener.accept().is_ok() {
                n += 1;
            }
            n
        }
    }
}

#[cfg(unix)]
mod fake_install_race {
    use super::fake::write_executable;
    use std::fs;
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering};

    // Regression: a script written by this process can fail to execute with
    // ETXTBSY while another thread forks, because the forked child holds the
    // write descriptor until its own exec.
    #[test]
    fn a_freshly_installed_script_runs_while_other_threads_spawn() {
        const INSTALLERS: usize = 4;
        const PER_INSTALLER: usize = 400;
        let dir = std::env::temp_dir().join(format!("rt-demo-etxtbsy-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let stop = AtomicBool::new(false);
        let busy = std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| {
                    while !stop.load(Ordering::Relaxed) {
                        Command::new("/bin/true")
                            .stdout(Stdio::null())
                            .status()
                            .unwrap();
                    }
                });
            }
            let installers: Vec<_> = (0..INSTALLERS)
                .map(|t| {
                    let dir = &dir;
                    scope.spawn(move || {
                        let mut busy = 0;
                        for i in 0..PER_INSTALLER {
                            let exe = dir.join(format!("s{t}-{i}"));
                            write_executable(&exe, "#!/bin/sh\nexit 0\n");
                            match Command::new(&exe).status() {
                                Ok(status) => assert!(status.success()),
                                Err(e) if e.raw_os_error() == Some(26) => busy += 1,
                                Err(e) => panic!("unexpected spawn error: {e}"),
                            }
                        }
                        busy
                    })
                })
                .collect();
            let busy: usize = installers.into_iter().map(|h| h.join().unwrap()).sum();
            stop.store(true, Ordering::Relaxed);
            busy
        });
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(busy, 0, "scripts that failed to execute with ETXTBSY");
    }
}

// Refusals that need no fake: with an empty `bin`, a guard that let the step
// through would fail with `DemoError::Step` (cannot run), not with the
// expected refusal.

#[test]
fn a_document_that_is_not_permitted_is_refused_naming_document_and_field() {
    let f = Fixture::new("not-permitted", "transmission.log.jsonl");
    let m = manifest(&[(DOC, false)]);
    let s = f.live_step(&["--store", "live-store", "--consent", DOC]);
    let before = f.written();
    let e = run_live_step(&s, &f.context(&m), &f.bin, &f.work).unwrap_err();
    assert_eq!(e, permission_error(DOC));
    assert!(e.to_string().contains(DOC));
    assert!(e.to_string().contains("transmission_permitted"));
    assert_eq!(f.written(), before);
    assert!(!f.work.join("live-store").exists());
}

#[test]
fn a_document_missing_from_the_manifest_is_refused() {
    let f = Fixture::new("absent", "transmission.log.jsonl");
    let m = manifest(&[(OTHER, true)]);
    let s = f.live_step(&["--store", "live-store", "--consent", DOC]);
    let e = run_live_step(&s, &f.context(&m), &f.bin, &f.work).unwrap_err();
    assert_eq!(e, permission_error(DOC));
}

#[test]
fn an_empty_manifest_refuses_every_document() {
    let f = Fixture::new("empty", "transmission.log.jsonl");
    let m = manifest(&[]);
    let s = f.live_step(&["--store", "live-store", "--consent", DOC]);
    let e = run_live_step(&s, &f.context(&m), &f.bin, &f.work).unwrap_err();
    assert_eq!(e, permission_error(DOC));
}

#[test]
fn one_unpermitted_document_among_several_refuses_the_step() {
    let f = Fixture::new("several", "transmission.log.jsonl");
    let m = manifest(&[(DOC, true), (OTHER, false)]);
    let s = f.live_step(&[
        "--store",
        "live-store",
        "--consent",
        DOC,
        "--consent",
        OTHER,
    ]);
    let e = run_live_step(&s, &f.context(&m), &f.bin, &f.work).unwrap_err();
    assert_eq!(e, permission_error(OTHER));
    let swapped = f.live_step(&[
        "--store",
        "live-store",
        "--consent",
        OTHER,
        "--consent",
        DOC,
    ]);
    let e = run_live_step(&swapped, &f.context(&m), &f.bin, &f.work).unwrap_err();
    assert_eq!(e, permission_error(OTHER));
}

#[test]
fn a_live_step_that_names_no_consent_is_refused() {
    let f = Fixture::new("no-consent", "transmission.log.jsonl");
    let m = manifest(&[(DOC, true)]);
    let s = f.live_step(&["--store", "live-store"]);
    let e = run_live_step(&s, &f.context(&m), &f.bin, &f.work).unwrap_err();
    assert_eq!(e, refused(&s, Refusal::NoConsent));
}

#[test]
fn a_live_step_with_unreadable_arguments_is_refused() {
    let f = Fixture::new("bad-args", "transmission.log.jsonl");
    let m = manifest(&[(DOC, true)]);
    for extra in [
        &["--store", "live-store", "--consent=stand-in-rules.md"][..],
        &["--store", "live-store", "--consent"],
        &["--store", "live-store", "--consent", "--store"],
        &["--store", "a", "--store", "b", "--consent", DOC],
        &["--store", "live-store", "--consent", DOC, "stray"],
    ] {
        let s = f.live_step(extra);
        let e = run_live_step(&s, &f.context(&m), &f.bin, &f.work).unwrap_err();
        assert!(
            matches!(
                e,
                DemoError::LiveRefused {
                    refusal: Refusal::UnreadableArguments(_) | Refusal::NoConsent,
                    ..
                }
            ),
            "{extra:?}: {e:?}"
        );
    }
}

#[test]
fn an_unreadable_configuration_is_live_and_is_refused_permitted_or_not() {
    let f = Fixture::new("no-config", "transmission.log.jsonl");
    let mut s = f.live_step(&["--store", "live-store", "--consent", DOC]);
    s.argv[3] = "ai/missing.toml".to_owned();
    assert_eq!(classify(&s, &f.work), Class::Live);
    let offline = run_step(&s, &f.bin, &f.work).unwrap_err();
    assert_eq!(offline, refused(&s, Refusal::LiveStepInOfflineRunner));
    let denied = manifest(&[(DOC, false)]);
    let e = run_live_step(&s, &f.context(&denied), &f.bin, &f.work).unwrap_err();
    assert_eq!(e, permission_error(DOC));
    let allowed = manifest(&[(DOC, true)]);
    let e = run_live_step(&s, &f.context(&allowed), &f.bin, &f.work).unwrap_err();
    assert!(
        matches!(
            e,
            DemoError::LiveRefused {
                refusal: Refusal::UnreadableConfig(_),
                ..
            }
        ),
        "{e:?}"
    );
}

#[test]
fn an_unknown_protocol_is_live() {
    let f = Fixture::new("unknown-protocol", "transmission.log.jsonl");
    let path = f.work.join("ai/live.toml");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(
        &path,
        text.replace(
            "protocol = \"openai-responses\"",
            "protocol = \"future-protocol\"",
        ),
    )
    .unwrap();
    let s = f.live_step(&["--store", "live-store", "--consent", DOC]);
    assert_eq!(classify(&s, &f.work), Class::Live);
    assert_eq!(
        run_step(&s, &f.bin, &f.work).unwrap_err(),
        refused(&s, Refusal::LiveStepInOfflineRunner)
    );
    let denied = manifest(&[(DOC, false)]);
    assert_eq!(
        run_live_step(&s, &f.context(&denied), &f.bin, &f.work).unwrap_err(),
        permission_error(DOC)
    );
}

#[test]
fn an_unknown_protocol_without_endpoint_or_credential_is_still_live() {
    let f = Fixture::new("bare-unknown", "transmission.log.jsonl");
    for protocol in [
        "future-protocol",
        "Mock-Replay",
        "mock-replay ",
        "openai-responses",
    ] {
        let text = format!(
            "schema_version = 2\nenabled = true\n[provider]\nid = \"x\"\n\
             protocol = \"{protocol}\"\nmodel = \"m\"\n"
        );
        fs::write(f.work.join("ai/bare.toml"), text).unwrap();
        let mut s = f.live_step(&["--store", "live-store", "--consent", DOC]);
        s.argv[3] = "ai/bare.toml".to_owned();
        assert_eq!(classify(&s, &f.work), Class::Live, "{protocol}");
    }
}

#[test]
fn a_configuration_the_reader_does_not_understand_is_live() {
    let f = Fixture::new("odd-config", "transmission.log.jsonl");
    // The multi-line string hides a mock-replay table from a naive reader.
    let hidden = "note = \"\"\"\n[provider]\nprotocol = \"mock-replay\"\n\"\"\"\n\
                  schema_version = 2\n[provider]\nprotocol = \"openai-responses\"\n";
    fs::write(f.work.join("ai/odd.toml"), hidden).unwrap();
    let mut s = f.live_step(&["--store", "live-store", "--consent", DOC]);
    s.argv[3] = "ai/odd.toml".to_owned();
    assert_eq!(classify(&s, &f.work), Class::Live);
}

#[test]
fn the_store_must_lie_outside_golden() {
    let f = Fixture::new("golden-store", "transmission.log.jsonl");
    let m = manifest(&[(DOC, true)]);
    let demo_golden = f.demo.join("golden").join("store").display().to_string();
    for store in [
        "golden",
        "golden/store",
        "work/../golden/store",
        "a/b/../../golden/store",
        "live-store/golden/x",
        "GOLDEN/store",
        demo_golden.as_str(),
    ] {
        let s = f.live_step(&["--store", store, "--consent", DOC]);
        let e = run_live_step(&s, &f.context(&m), &f.bin, &f.work).unwrap_err();
        assert!(
            matches!(
                &e,
                DemoError::LiveRefused {
                    refusal: Refusal::ForbiddenOutput { option, path, .. },
                    ..
                } if option == "--store" && path == store
            ),
            "{store}: {e:?}"
        );
    }
    assert!(!f.work.join("golden").exists());
    assert!(!f.demo.join("golden/store").exists());
}

#[test]
fn an_output_option_must_lie_outside_golden() {
    let f = Fixture::new("golden-out", "transmission.log.jsonl");
    let m = manifest(&[(DOC, true)]);
    let s = f.live_step(&[
        "--store",
        "live-store",
        "--out",
        "../golden/steps/s02.stdout",
        "--consent",
        DOC,
    ]);
    let e = run_live_step(&s, &f.context(&m), &f.bin, &f.work).unwrap_err();
    assert!(
        matches!(
            &e,
            DemoError::LiveRefused {
                refusal: Refusal::ForbiddenOutput { option, .. },
                ..
            } if option == "--out"
        ),
        "{e:?}"
    );
    assert_eq!(
        fs::read(f.demo.join("golden/steps/s02.stdout")).unwrap(),
        b"golden bytes"
    );
}

#[test]
fn the_store_and_out_must_lie_outside_provider() {
    let f = Fixture::new("provider-out", "transmission.log.jsonl");
    let m = manifest(&[(DOC, true)]);
    let demo_provider = f
        .demo
        .join("provider")
        .join("script.json")
        .display()
        .to_string();
    for (option, path) in [
        ("--store", "provider"),
        ("--store", "provider/store"),
        ("--store", "x/../provider/store"),
        ("--out", "provider/authored-script.json"),
        (
            "--out",
            "provider/recorded-meta-muse-spark-1.3-prompt-2.json",
        ),
        ("--out", demo_provider.as_str()),
    ] {
        let mut extra = vec!["--consent", DOC];
        let mut store = "live-store";
        if option == "--store" {
            store = path;
        } else {
            extra.extend(["--out", path]);
        }
        extra.extend(["--store", store]);
        let s = f.live_step(&extra);
        let e = run_live_step(&s, &f.context(&m), &f.bin, &f.work).unwrap_err();
        assert!(
            matches!(
                &e,
                DemoError::LiveRefused {
                    refusal: Refusal::ForbiddenOutput { option: o, path: p, .. },
                    ..
                } if o == option && p == path
            ),
            "{option} {path}: {e:?}"
        );
    }
    assert_eq!(
        fs::read(f.demo.join("provider/script.json")).unwrap(),
        b"{}"
    );
}

#[test]
fn the_transmission_log_of_the_configuration_must_lie_outside_golden_and_provider() {
    for log in [
        "golden/log.jsonl",
        "../golden/log.jsonl",
        "provider/log.jsonl",
        "../demo/golden/log.jsonl",
        "../demo/provider/recorded.json",
    ] {
        let f = Fixture::new("log", log);
        let m = manifest(&[(DOC, true)]);
        let s = f.live_step(&["--store", "live-store", "--consent", DOC]);
        let e = run_live_step(&s, &f.context(&m), &f.bin, &f.work).unwrap_err();
        assert!(
            matches!(
                &e,
                DemoError::LiveRefused {
                    refusal: Refusal::ForbiddenOutput { option, path, .. },
                    ..
                } if option == "transmission_log" && path == log
            ),
            "{log}: {e:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_symbolic_link_into_golden_does_not_hide_the_store() {
    let f = Fixture::new("symlink", "transmission.log.jsonl");
    std::os::unix::fs::symlink(f.demo.join("golden"), f.work.join("innocent")).unwrap();
    let m = manifest(&[(DOC, true)]);
    let s = f.live_step(&["--store", "innocent/store", "--consent", DOC]);
    let e = run_live_step(&s, &f.context(&m), &f.bin, &f.work).unwrap_err();
    assert!(
        matches!(
            e,
            DemoError::LiveRefused {
                refusal: Refusal::ForbiddenOutput { .. },
                ..
            }
        ),
        "{e:?}"
    );
}

#[test]
fn a_live_step_for_another_executable_or_an_offline_step_is_not_a_live_step() {
    let f = Fixture::new("not-live", "transmission.log.jsonl");
    let m = manifest(&[(DOC, true)]);
    let other = step(vec!["ruletrace".to_owned(), "ingest".to_owned()]);
    assert_eq!(
        run_live_step(&other, &f.context(&m), &f.bin, &f.work).unwrap_err(),
        refused(&other, Refusal::NotRuletraceAi)
    );
    let mut offline = f.live_step(&["--store", "live-store", "--consent", DOC]);
    offline.argv[3] = "ai/mock-replay.toml".to_owned();
    assert_eq!(classify(&offline, &f.work), Class::Offline);
    assert_eq!(
        run_live_step(&offline, &f.context(&m), &f.bin, &f.work).unwrap_err(),
        refused(&offline, Refusal::NotLive)
    );
}

#[test]
fn the_offline_runner_refuses_a_step_with_a_live_configuration() {
    let f = Fixture::new("hidden-live", "transmission.log.jsonl");
    let s = f.live_step(&["--store", "live-store", "--consent", DOC]);
    assert_eq!(classify(&s, &f.work), Class::Live);
    assert_eq!(
        run_step(&s, &f.bin, &f.work).unwrap_err(),
        refused(&s, Refusal::LiveStepInOfflineRunner)
    );
}

#[test]
fn only_the_credential_variable_of_the_configuration_may_be_passed() {
    let f = Fixture::new("env", "transmission.log.jsonl");
    let m = manifest(&[(DOC, true)]);
    let s = f.live_step(&["--store", "live-store", "--consent", DOC]);
    for name in [
        "PATH",
        "HOME",
        "MODEL_API_KEY_OTHER",
        "RULETRACE_OPENAI_API_KEY",
    ] {
        let environment = [(name.to_owned(), "x".to_owned())];
        let context = LiveContext {
            manifest: &m,
            demo_dir: &f.demo,
            environment: &environment,
        };
        assert_eq!(
            run_live_step(&s, &context, &f.bin, &f.work).unwrap_err(),
            refused(&s, Refusal::EnvironmentNotAllowed(name.to_owned())),
            "{name}"
        );
    }
}

#[test]
fn a_permitted_document_with_a_store_outside_golden_passes_the_guard() {
    // The bin directory is empty, so the guard's acceptance shows as the
    // runner's own "cannot run" error, never a refusal.
    let f = Fixture::new("accepted-no-bin", "transmission.log.jsonl");
    let m = manifest(&[(DOC, true), (OTHER, true)]);
    let s = f.live_step(&[
        "--store",
        "live-store",
        "--consent",
        DOC,
        "--consent",
        OTHER,
    ]);
    let e = run_live_step(&s, &f.context(&m), &f.bin, &f.work).unwrap_err();
    assert!(
        matches!(&e, DemoError::Step { step, problem }
            if step == "live" && problem.contains("cannot run")),
        "{e:?}"
    );
}

#[test]
fn argv0_is_recognised_as_ruletrace_ai_by_file_name_only() {
    let f = Fixture::new("argv0", "transmission.log.jsonl");
    let m = manifest(&[(DOC, true)]);
    for argv0 in [
        "./ruletrace-ai",
        "/abs/dir/ruletrace-ai",
        "bin/ruletrace-ai",
        "RuleTrace-AI.EXE",
        "ruletrace-ai.exe",
        "C:\\tools\\ruletrace-ai.Exe",
        "..\\ruletrace-ai",
        "./RULETRACE-AI",
    ] {
        let mut s = f.live_step(&["--store", "live-store", "--consent", DOC]);
        s.argv[0] = argv0.to_owned();
        assert_eq!(classify(&s, &f.work), Class::Live, "{argv0}");
        assert_eq!(
            run_step(&s, &f.bin, &f.work).unwrap_err(),
            refused(&s, Refusal::LiveStepInOfflineRunner),
            "{argv0}"
        );
        // The live runner treats it as ruletrace-ai too: it reaches the
        // output-location rules instead of stopping at NotRuletraceAi.
        let mut golden = f.live_step(&["--store", "golden/x", "--consent", DOC]);
        golden.argv[0] = argv0.to_owned();
        let e = run_live_step(&golden, &f.context(&m), &f.bin, &f.work).unwrap_err();
        assert!(
            matches!(
                &e,
                DemoError::LiveRefused {
                    refusal: Refusal::ForbiddenOutput { .. },
                    ..
                }
            ),
            "{argv0}: {e:?}"
        );
    }
}

#[test]
fn other_file_names_are_not_ruletrace_ai() {
    let f = Fixture::new("argv0-other", "transmission.log.jsonl");
    for argv0 in [
        "ruletrace",
        "ruletrace-ai-x",
        "xruletrace-ai",
        "ruletrace-ai.exe.bak",
        "ruletrace-aié",
        "ruletrace-ai/",
        "",
    ] {
        let mut s = f.live_step(&["--store", "live-store", "--consent", DOC]);
        s.argv[0] = argv0.to_owned();
        let class = classify(&s, &f.work);
        // A trailing separator names the directory entry itself, which the
        // guard also treats as the tool.
        let expected = if argv0 == "ruletrace-ai/" {
            Class::Live
        } else {
            Class::Offline
        };
        assert_eq!(class, expected, "{argv0:?}");
    }
}

#[test]
fn an_offline_config_stays_offline_whatever_the_path_of_argv0() {
    let f = Fixture::new("argv0-offline", "transmission.log.jsonl");
    let mut s = f.live_step(&["--store", "live-store", "--consent", DOC]);
    s.argv[3] = "ai/mock-replay.toml".to_owned();
    s.argv[0] = "./ruletrace-ai".to_owned();
    assert_eq!(classify(&s, &f.work), Class::Offline);
}

#[test]
fn a_manifest_that_lists_an_id_twice_needs_every_entry_permitted() {
    let f = Fixture::new("duplicate-ids", "transmission.log.jsonl");
    let s = f.live_step(&["--store", "live-store", "--consent", DOC]);
    for entries in [
        [(DOC, false), (DOC, true)],
        [(DOC, true), (DOC, false)],
        [(DOC, false), (DOC, false)],
    ] {
        let m = manifest(&entries);
        assert!(!m.may_transmit(DOC), "{entries:?}");
        let e = run_live_step(&s, &f.context(&m), &f.bin, &f.work).unwrap_err();
        assert_eq!(e, permission_error(DOC), "{entries:?}");
    }
    let m = manifest(&[(DOC, true), (DOC, true)]);
    assert!(m.may_transmit(DOC));
    let e = run_live_step(&s, &f.context(&m), &f.bin, &f.work).unwrap_err();
    assert!(
        matches!(&e, DemoError::Step { problem, .. } if problem.contains("cannot run")),
        "{e:?}"
    );
}

#[test]
fn golden_and_provider_are_matched_without_regard_to_ascii_case() {
    let f = Fixture::new("case", "transmission.log.jsonl");
    let m = manifest(&[(DOC, true)]);
    for path in [
        "GOLDEN/x",
        "Golden/x",
        "Provider/x",
        "PROVIDER/x",
        "pRoViDeR",
        "../demo/Provider/x",
        "a/../PROVIDER/x",
    ] {
        let s = f.live_step(&["--store", path, "--consent", DOC]);
        let e = run_live_step(&s, &f.context(&m), &f.bin, &f.work).unwrap_err();
        assert!(
            matches!(
                &e,
                DemoError::LiveRefused {
                    refusal: Refusal::ForbiddenOutput { option, path: p, .. },
                    ..
                } if option == "--store" && p == path
            ),
            "{path}: {e:?}"
        );
    }
    for log in ["Provider/log.jsonl", "GOLDEN/log.jsonl"] {
        let f = Fixture::new("case-log", log);
        let s = f.live_step(&["--store", "live-store", "--consent", DOC]);
        let e = run_live_step(&s, &f.context(&m), &f.bin, &f.work).unwrap_err();
        assert!(
            matches!(
                &e,
                DemoError::LiveRefused {
                    refusal: Refusal::ForbiddenOutput { option, .. },
                    ..
                } if option == "transmission_log"
            ),
            "{log}: {e:?}"
        );
    }
    let s = f.live_step(&[
        "--out",
        "Provider/o.json",
        "--store",
        "ok",
        "--consent",
        DOC,
    ]);
    assert!(matches!(
        run_live_step(&s, &f.context(&m), &f.bin, &f.work).unwrap_err(),
        DemoError::LiveRefused {
            refusal: Refusal::ForbiddenOutput { option, .. },
            ..
        } if option == "--out"
    ));
    // A name that only starts like a protected one is allowed.
    let s = f.live_step(&["--store", "providers/x", "--consent", DOC]);
    let e = run_live_step(&s, &f.context(&m), &f.bin, &f.work).unwrap_err();
    assert!(matches!(e, DemoError::Step { .. }), "{e:?}");
}

#[test]
fn the_placeholder_model_is_refused_naming_the_field() {
    let f = Fixture::new("placeholder", "transmission.log.jsonl");
    let m = manifest(&[(DOC, true)]);
    let example = fs::read_to_string(repo_demo().join("ai/openai.example.toml")).unwrap();
    assert!(example.contains("model = \"REPLACE_WITH_YOUR_MODEL_ID\""));
    fs::write(f.work.join("ai/openai.toml"), &example).unwrap();
    let mut s = f.live_step(&["--store", "live-store", "--consent", DOC]);
    s.argv[3] = "ai/openai.toml".to_owned();
    assert_eq!(classify(&s, &f.work), Class::Live);
    let e = run_live_step(&s, &f.context(&m), &f.bin, &f.work).unwrap_err();
    assert_eq!(
        e,
        refused(&s, Refusal::PlaceholderModel("provider.model".to_owned()))
    );
    assert!(e.to_string().contains("provider.model"));
    // With a real-looking model the same file passes the guard.
    let replaced = example.replace("REPLACE_WITH_YOUR_MODEL_ID", "some-model-1");
    fs::write(f.work.join("ai/openai.toml"), replaced).unwrap();
    let e = run_live_step(&s, &f.context(&m), &f.bin, &f.work).unwrap_err();
    assert!(
        matches!(&e, DemoError::Step { problem, .. } if problem.contains("cannot run")),
        "{e:?}"
    );
}

#[cfg(unix)]
#[test]
fn an_output_path_that_cannot_be_resolved_is_refused() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new("unresolvable", "transmission.log.jsonl");
    let m = manifest(&[(DOC, true)]);
    symlink("/nonexistent-rt-target", f.work.join("dangling")).unwrap();
    symlink("loop-b", f.work.join("loop-a")).unwrap();
    symlink("loop-a", f.work.join("loop-b")).unwrap();
    for store in [
        "dangling",
        "dangling/store",
        "loop-a/store",
        "x/../dangling/s",
    ] {
        let s = f.live_step(&["--store", store, "--consent", DOC]);
        let e = run_live_step(&s, &f.context(&m), &f.bin, &f.work).unwrap_err();
        assert!(
            matches!(
                &e,
                DemoError::LiveRefused {
                    refusal: Refusal::UnresolvablePath { option, path, .. },
                    ..
                } if option == "--store" && path == store
            ),
            "{store}: {e:?}"
        );
    }
    let s = f.live_step(&["--out", "dangling", "--store", "ok", "--consent", DOC]);
    let e = run_live_step(&s, &f.context(&m), &f.bin, &f.work).unwrap_err();
    assert!(
        matches!(
            &e,
            DemoError::LiveRefused {
                refusal: Refusal::UnresolvablePath { option, .. },
                ..
            } if option == "--out"
        ),
        "{e:?}"
    );
    let g = Fixture::new("unresolvable-log", "dangling-log.jsonl");
    symlink("/nonexistent-rt-target", g.work.join("dangling-log.jsonl")).unwrap();
    let s = g.live_step(&["--store", "live-store", "--consent", DOC]);
    let e = run_live_step(&s, &g.context(&m), &g.bin, &g.work).unwrap_err();
    assert!(
        matches!(
            &e,
            DemoError::LiveRefused {
                refusal: Refusal::UnresolvablePath { option, .. },
                ..
            } if option == "transmission_log"
        ),
        "{e:?}"
    );
    // The refusal does not name a non-existent path that is merely absent.
    let s = f.live_step(&["--store", "absent/deeper", "--consent", DOC]);
    let e = run_live_step(&s, &f.context(&m), &f.bin, &f.work).unwrap_err();
    assert!(matches!(e, DemoError::Step { .. }), "{e:?}");
}

#[cfg(unix)]
mod before_spawn {
    use super::fake::Probe;
    use super::*;
    use rt_demo::{run_runbook_recorded, Mode};

    #[test]
    fn positive_control_the_fake_runs_for_a_permitted_offline_step() {
        let f = Fixture::new("control", "transmission.log.jsonl");
        let probe = Probe::install(&f.bin);
        let mut s = f.live_step(&["--store", "live-store", "--consent", DOC]);
        s.argv[3] = "ai/mock-replay.toml".to_owned();
        assert_eq!(classify(&s, &f.work), Class::Offline);
        let c = run_step(&s, &f.bin, &f.work).unwrap();
        assert_eq!(c.exit, 0);
        assert_eq!(fs::read_to_string(&probe.marker).unwrap(), "unset/unset");
        assert_eq!(probe.connections(), 1);
    }

    #[test]
    fn a_refused_live_step_never_starts_ruletrace_ai() {
        let manifests = [
            manifest(&[(DOC, false)]),
            manifest(&[(OTHER, true)]),
            manifest(&[]),
        ];
        for m in &manifests {
            let f = Fixture::new("refused-before-spawn", "transmission.log.jsonl");
            let probe = Probe::install(&f.bin);
            let s = f.live_step(&["--store", "live-store", "--consent", DOC]);
            let before = f.written();
            let e = run_live_step(&s, &f.context(m), &f.bin, &f.work).unwrap_err();
            assert_eq!(e, permission_error(DOC));
            assert!(!probe.marker.exists(), "ruletrace-ai was started");
            assert_eq!(probe.connections(), 0);
            assert_eq!(f.written(), before);
        }
    }

    #[test]
    fn every_other_refusal_also_happens_before_the_process_starts() {
        let f = Fixture::new("other-refusals", "transmission.log.jsonl");
        let probe = Probe::install(&f.bin);
        let m = manifest(&[(DOC, true)]);
        let steps = [
            f.live_step(&["--store", "live-store"]),
            f.live_step(&["--store", "golden/store", "--consent", DOC]),
            f.live_step(&["--store", "provider/store", "--consent", DOC]),
            f.live_step(&["--store", "live-store", "--consent=stand-in-rules.md"]),
        ];
        for s in &steps {
            assert!(run_live_step(s, &f.context(&m), &f.bin, &f.work).is_err());
        }
        let hidden = f.live_step(&["--store", "live-store", "--consent", DOC]);
        assert!(run_step(&hidden, &f.bin, &f.work).is_err());
        assert!(!probe.marker.exists(), "ruletrace-ai was started");
        assert_eq!(probe.connections(), 0);
    }

    #[test]
    fn a_permitted_live_step_runs_and_receives_only_the_credential_variable() {
        let f = Fixture::new("accepted", "transmission.log.jsonl");
        let probe = Probe::install(&f.bin);
        let m = manifest(&[(DOC, true)]);
        let s = f.live_step(&["--store", "live-store", "--consent", DOC]);
        let environment = [(
            "OPENAI_API_KEY".to_owned(),
            "canary-not-a-credential".to_owned(),
        )];
        let context = LiveContext {
            manifest: &m,
            demo_dir: &f.demo,
            environment: &environment,
        };
        // The Meta example names MODEL_API_KEY, so the OpenAI variable is
        // refused and the probe stays absent.
        let e = run_live_step(&s, &context, &f.bin, &f.work).unwrap_err();
        assert_eq!(
            e,
            refused(
                &s,
                Refusal::EnvironmentNotAllowed("OPENAI_API_KEY".to_owned())
            )
        );
        assert!(!probe.marker.exists());
        let environment = [("MODEL_API_KEY".to_owned(), "canary".to_owned())];
        let context = LiveContext {
            manifest: &m,
            demo_dir: &f.demo,
            environment: &environment,
        };
        let c = run_live_step(&s, &context, &f.bin, &f.work).unwrap();
        assert_eq!(c.exit, 0);
        assert_eq!(fs::read_to_string(&probe.marker).unwrap(), "canary/unset");
        assert_eq!(probe.connections(), 1);
        assert!(!String::from_utf8_lossy(&c.stdout).contains("canary"));
    }

    #[test]
    fn a_path_spelling_of_ruletrace_ai_is_refused_before_the_process_starts() {
        let f = Fixture::new("argv0-spawn", "transmission.log.jsonl");
        let probe = Probe::install(&f.bin);
        let absolute = f.bin.join("ruletrace-ai").display().to_string();
        for argv0 in ["./ruletrace-ai", absolute.as_str()] {
            let mut s = f.live_step(&["--store", "live-store", "--consent", DOC]);
            s.argv[0] = argv0.to_owned();
            assert_eq!(
                run_step(&s, &f.bin, &f.work).unwrap_err(),
                refused(&s, Refusal::LiveStepInOfflineRunner),
                "{argv0}"
            );
        }
        assert!(!probe.marker.exists(), "ruletrace-ai was started");
        assert_eq!(probe.connections(), 0);
    }

    #[test]
    fn positive_control_the_fake_runs_through_a_path_spelling_when_offline() {
        let f = Fixture::new("argv0-control", "transmission.log.jsonl");
        let probe = Probe::install(&f.bin);
        let mut s = f.live_step(&["--store", "live-store", "--consent", DOC]);
        s.argv[3] = "ai/mock-replay.toml".to_owned();
        s.argv[0] = "./ruletrace-ai".to_owned();
        assert_eq!(run_step(&s, &f.bin, &f.work).unwrap().exit, 0);
        assert!(probe.marker.exists());
        assert_eq!(probe.connections(), 1);
    }

    #[test]
    fn the_runbook_runner_refuses_an_unmarked_live_step_before_it_starts() {
        let f = Fixture::new("runbook-hidden", "transmission.log.jsonl");
        let probe = Probe::install(&f.bin);
        let text = runbook_text(
            "{\"id\":\"s02\",\"argv\":[\"ruletrace-ai\",\"propose\",\"--config\",\"ai/live.toml\",\
             \"--store\",\"live-store\",\"--consent\",\"stand-in-rules.md\"],\
             \"expect_exit\":0,\"stdout_golden\":\"golden/steps/s02.stdout\",\
             \"stderr_golden\":\"golden/steps/s02.stderr\"}",
        );
        let rb = parse_runbook(&text).unwrap();
        let work = f.work.parent().unwrap().join("work2");
        let e = run_runbook_recorded(&rb, "ai", &f.demo, &work, &f.bin, Mode::Compare).unwrap_err();
        assert!(
            matches!(
                e,
                DemoError::LiveRefused {
                    refusal: Refusal::LiveStepInOfflineRunner,
                    ..
                }
            ),
            "{e:?}"
        );
        assert!(!probe.marker.exists());
        assert_eq!(probe.connections(), 0);
    }

    #[test]
    fn the_runbook_runner_skips_a_step_declared_live() {
        let f = Fixture::new("runbook-live", "transmission.log.jsonl");
        let probe = Probe::install(&f.bin);
        fs::write(f.demo.join("golden/steps/s02.stderr"), b"").unwrap();
        let text = runbook_text(
            "{\"id\":\"s02\",\"live\":true,\"argv\":[\"ruletrace-ai\",\"propose\",\"--config\",\
             \"ai/live.toml\",\"--store\",\"live-store\",\"--consent\",\"stand-in-rules.md\"],\
             \"expect_exit\":0,\"stdout_golden\":\"golden/steps/s02.stdout\",\
             \"stderr_golden\":\"golden/steps/s02.stderr\"}",
        );
        let rb = parse_runbook(&text).unwrap();
        assert!(rb.steps_for("ai").unwrap().is_empty());
        assert_eq!(rb.live_steps_for("ai").unwrap().len(), 1);
        let work = f.work.parent().unwrap().join("work2");
        let records =
            run_runbook_recorded(&rb, "ai", &f.demo, &work, &f.bin, Mode::Compare).unwrap();
        assert!(records.is_empty());
        assert!(!probe.marker.exists());
        assert_eq!(probe.connections(), 0);
    }
}

fn runbook_text(step: &str) -> String {
    format!(
        "# t\n\n```ruletrace-runbook\n{{\"format\":\"ruletrace-runbook\",\"version\":1,\
         \"profiles\":{{\"ai\":{{}}}},\"steps\":[{step}]}}\n```\n"
    )
}

#[test]
fn a_live_step_may_only_be_ruletrace_ai_and_has_no_golden_outputs() {
    let other = runbook_text(
        "{\"id\":\"s\",\"live\":true,\"argv\":[\"ruletrace\",\"ingest\"],\"expect_exit\":0,\
         \"stdout_golden\":\"g\",\"stderr_golden\":\"g\"}",
    );
    let e = parse_runbook(&other).unwrap_err().to_string();
    assert!(e.contains("step 's' field 'live'"), "{e}");
    let with_output = runbook_text(
        "{\"id\":\"s\",\"live\":true,\"argv\":[\"ruletrace-ai\",\"propose\"],\"expect_exit\":0,\
         \"stdout_golden\":\"g\",\"stderr_golden\":\"g\",\
         \"outputs\":[{\"path\":\"work/o\",\"golden\":\"golden/o\"}]}",
    );
    let e = parse_runbook(&with_output).unwrap_err().to_string();
    assert!(e.contains("step 's' field 'outputs'"), "{e}");
    let not_bool = runbook_text(
        "{\"id\":\"s\",\"live\":\"true\",\"argv\":[\"ruletrace-ai\",\"propose\"],\"expect_exit\":0,\
         \"stdout_golden\":\"g\",\"stderr_golden\":\"g\"}",
    );
    let e = parse_runbook(&not_bool).unwrap_err().to_string();
    assert!(e.contains("step 's' field 'live'"), "{e}");
}

#[test]
fn the_committed_configurations_are_classified_by_hand_written_expectation() {
    let demo = repo_demo();
    let case = |config: &str| {
        step(
            ["ruletrace-ai", "propose", "--config", config]
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
        )
    };
    for offline in [
        "ai/mock-replay.toml",
        "ai/meta-replay.toml",
        "ai/meta-replay-prompt-3.toml",
    ] {
        assert_eq!(classify(&case(offline), &demo), Class::Offline, "{offline}");
    }
    for live in ["ai/meta.example.toml", "ai/openai.example.toml"] {
        assert_eq!(classify(&case(live), &demo), Class::Live, "{live}");
    }
    let models = step(
        ["ruletrace-ai", "models", "--config", "ai/mock-replay.toml"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
    );
    assert_eq!(classify(&models, &demo), Class::Live);
    let no_config = step(
        ["ruletrace-ai", "propose"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
    );
    assert_eq!(classify(&no_config, &demo), Class::Live);
}

#[test]
fn a_manifest_permission_that_is_not_the_boolean_true_is_not_a_permission() {
    let files = |p: &str| match p {
        "documents/d.md" => Some(b"x".to_vec()),
        _ => None,
    };
    let hash = rt_hash::sha256(b"x").to_string();
    let manifest_with = |value: &str| {
        format!(
            "{{\"format\":\"ruletrace-demo-manifest\",\"version\":1,\"name\":\"n\",\
             \"documents\":[{{\"id\":\"d\",\"path\":\"documents/d.md\",\"title\":\"t\",\
             \"publisher\":\"p\",\"address\":\"a\",\"retrieved\":\"r\",\"sha256\":\"{hash}\",\
             \"licence\":\"l\",\"licence_reference\":\"l\",\"transmission_permitted\":{value},\
             \"ud34_confirmation\":\"u\",\"original\":null,\"coverage_note\":\"c\"}}],\
             \"data\":[],\"provider_script\":null}}"
        )
    };
    let ok = check_manifest(manifest_with("true").as_bytes(), &files).unwrap();
    assert!(ok.may_transmit("d"));
    for value in ["\"true\"", "\"yes\"", "1", "\"True\"", "null", "[true]"] {
        let e = check_manifest(manifest_with(value).as_bytes(), &files).unwrap_err();
        assert_eq!(
            e,
            DemoError::Manifest {
                subject: "d".to_owned(),
                field: "transmission_permitted".to_owned(),
                problem: "field is not a boolean".to_owned(),
            },
            "{value}"
        );
    }
    let denied = check_manifest(manifest_with("false").as_bytes(), &files).unwrap();
    assert!(!denied.may_transmit("d"));
}

#[test]
fn the_runbook_section_on_live_steps_is_parsed_and_runs_nothing() {
    let text = fs::read_to_string(repo_demo().join("RUNBOOK.md")).unwrap();
    let rb = parse_runbook(&text).unwrap();
    assert!(rb.live.is_empty());
    let section = text
        .split("## Live steps and document transmission")
        .nth(1)
        .expect("the section exists");
    let fences: Vec<&str> = section.split("```").skip(1).step_by(2).collect();
    assert_eq!(fences.len(), 3);
    assert!(fences.iter().all(|f| f.starts_with("sh live\n")));
    let commands = fences.join("\n");
    for needed in [
        "ruletrace-ai propose --dry-run",
        "--consent stand-in-rules.md",
        "meta.example.toml",
    ] {
        assert!(commands.contains(needed), "{needed}");
    }
    assert!(!section.contains("_KEY="), "no credential assignment");
    assert!(section.contains("transmission_permitted"));
}

#[test]
fn an_unmarked_live_command_in_the_section_would_be_rejected_by_the_parser() {
    let text = fs::read_to_string(repo_demo().join("RUNBOOK.md")).unwrap();
    let unmarked = text.replace("```sh live", "```sh");
    let e = parse_runbook(&unmarked).unwrap_err().to_string();
    assert!(e.contains("unmarked command"), "{e}");
}
