//! The Request_Identifier of the first live run stays valid (Req 35.23).
//!
//! The prompt preamble and the schema descriptions are not part of the
//! canonical request bytes, so a new prompt profile must not move the
//! identifier. The expected value is the one recorded by the live run, in the
//! stored batch and in its Transmission_Log entry. The dry run sends nothing
//! and reads no credential.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use support::Work;

const LIVE_REQUEST_ID: &str = "1c574c2d1e2658ba060f68b788c2407f9aa712efd697e23c997d8c8c653b54b2";

fn live_inputs() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../rt-ai-tool/tests/fixtures/live-run1-prompt-2")
}

#[test]
fn the_live_request_identifier_is_unchanged_by_the_prompt_profile() {
    let work = Work::new("request-id-pin");
    let dir = live_inputs();
    work.copy_from(&dir, "inputs/meta.toml", "meta.toml");
    work.copy_from(&dir, "inputs/candidates.json", "candidates.json");
    work.copy_from(&dir, "inputs/source-model.json", "source-model.json");
    let seed =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demo/stand-in-ai/rulebook");
    work.copy_from(&seed, "seed.json", "seed.json");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let private = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(work.path("meta.toml"), private).unwrap();
    }
    let ran = work.ruletrace_ai(&[
        "propose",
        "--dry-run",
        "--config",
        &work.arg("meta.toml"),
        "--rulebook",
        &work.arg("seed.json"),
        "--source-model",
        &work.arg("source-model.json"),
        "--candidates",
        &work.arg("candidates.json"),
        "--store",
        &work.arg("store"),
        "--created-at",
        "2026-11-16T00:00:00Z",
        "--consent",
        "stand-in-rules.md",
    ]);
    assert_eq!(ran.code, 0, "{}", ran.stderr);
    assert_eq!(
        ran.stdout.lines().last(),
        Some(LIVE_REQUEST_ID),
        "{}",
        ran.stdout
    );
    assert!(!work.exists("store"), "a dry run writes no store");
}
