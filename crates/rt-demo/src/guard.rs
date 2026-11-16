//! The live-step guard (design §8.11.6, Req 33.10, 28.21, 28.11, 33.2).
//!
//! The guard runs in the runner before any process is started. A
//! `ruletrace-ai` step is offline only when its configuration is provably the
//! `mock-replay` provider. Every other `ruletrace-ai` step is live, and the
//! offline runner refuses it. The live runner starts a live step only when
//! the manifest permits every document the step names with `--consent`, and
//! only when the step writes outside `golden/` and `provider/`.
//!
//! The configuration is read with a deliberately small TOML reader that
//! accepts a strict subset. Anything it does not understand makes the step
//! live, so a configuration the reader misjudges can only be refused, never
//! passed as offline by mistake.

use crate::error::{DemoError, Refusal};
use crate::manifest::Manifest;
use crate::runbook::Step;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// Largest configuration file the guard reads.
const MAX_CONFIG_BYTES: u64 = 64 * 1024;

/// Options of `ruletrace-ai` that take a value. `--out` is not an option of
/// the tool today; the guard still checks it so that a later option cannot
/// bypass the location rules.
const VALUE_OPTIONS: [&str; 11] = [
    "--config",
    "--rulebook",
    "--source-model",
    "--candidates",
    "--store",
    "--created-at",
    "--consent",
    "--revise-rule",
    "--revise-revision",
    "--feedback-file",
    "--out",
];

/// Options that may repeat.
const REPEATABLE: [&str; 2] = ["--source-model", "--consent"];

/// Whether a step may run in the offline runner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// Not a `ruletrace-ai` step, or a `propose` step whose configuration is
    /// the `mock-replay` provider.
    Offline,
    /// Any other `ruletrace-ai` step, including one whose arguments or
    /// configuration cannot be read.
    Live,
}

/// What a live step may do once the guard allows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveAuthorization {
    /// The only environment variable the step may receive: the credential
    /// variable the configuration names.
    pub credential_env_var: Option<String>,
}

#[derive(Debug, Default)]
struct AiArgs {
    command: String,
    config: Vec<String>,
    consent: Vec<String>,
    store: Vec<String>,
    out: Vec<String>,
}

/// Reads `ruletrace-ai` arguments the way the tool does: an option is an
/// exact token and its value is the next token, which must not start with
/// `--`. Anything else is an error, so the guard never guesses.
fn parse_ai_args(argv: &[String]) -> Result<AiArgs, String> {
    let command = argv.get(1).ok_or_else(|| "no command".to_owned())?.clone();
    let mut args = AiArgs {
        command,
        ..AiArgs::default()
    };
    let mut seen: Vec<&str> = Vec::new();
    let mut index = 2;
    while let Some(token) = argv.get(index) {
        if token == "--dry-run" {
            index += 1;
            continue;
        }
        let Some(name) = VALUE_OPTIONS.iter().find(|o| **o == token.as_str()) else {
            return Err(format!("unknown argument '{token}'"));
        };
        let value = argv
            .get(index + 1)
            .filter(|v| !v.starts_with("--") && !v.is_empty())
            .ok_or_else(|| format!("option '{name}' has no usable value"))?;
        if !REPEATABLE.contains(name) && seen.contains(name) {
            return Err(format!("option '{name}' is given more than once"));
        }
        seen.push(name);
        match *name {
            "--config" => args.config.push(value.clone()),
            "--consent" => args.consent.push(value.clone()),
            "--store" => args.store.push(value.clone()),
            "--out" => args.out.push(value.clone()),
            _ => {}
        }
        index += 2;
    }
    Ok(args)
}

#[derive(Debug, PartialEq, Eq)]
enum Val {
    Str(String),
    Other,
}

type Table = BTreeMap<String, Val>;

#[derive(Debug, Default)]
struct Simple {
    top: Table,
    tables: BTreeMap<String, Table>,
}

fn is_bare_key(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn read_value(text: &str) -> Option<Val> {
    let text = text.trim();
    if text.is_empty() || text.contains("\"\"\"") || text.contains("'''") {
        return None;
    }
    if let Some(inner) = text.strip_prefix('"') {
        let inner = inner.strip_suffix('"')?;
        if inner.contains('"') || inner.contains('\\') {
            return None;
        }
        return Some(Val::Str(inner.to_owned()));
    }
    if text.starts_with('\'') || text.starts_with('{') {
        return None;
    }
    if text.starts_with('[') && !text.ends_with(']') {
        return None;
    }
    Some(Val::Other)
}

/// Parses the strict subset: comment lines, `[name]` headers, and
/// `key = value` lines. Returns `None` for anything else.
fn parse_simple(text: &str) -> Option<Simple> {
    let mut simple = Simple::default();
    let mut current: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[') {
            let name = rest.strip_suffix(']')?;
            if !is_bare_key(name) || simple.tables.contains_key(name) {
                return None;
            }
            simple.tables.insert(name.to_owned(), Table::new());
            current = Some(name.to_owned());
            continue;
        }
        let (key, value) = line.split_once('=')?;
        let key = key.trim();
        if !is_bare_key(key) {
            return None;
        }
        let value = read_value(value)?;
        let table = match &current {
            Some(name) => simple.tables.get_mut(name)?,
            None => &mut simple.top,
        };
        if table.insert(key.to_owned(), value).is_some() {
            return None;
        }
    }
    Some(simple)
}

/// What the guard needs from a configuration.
#[derive(Debug, PartialEq, Eq)]
struct Facts {
    mock_replay: bool,
    transmission_log: Option<String>,
    credential_env_var: Option<String>,
}

fn string_of(table: &Table, key: &str) -> Option<String> {
    match table.get(key) {
        Some(Val::Str(s)) => Some(s.clone()),
        _ => None,
    }
}

fn facts_of(simple: &Simple) -> Facts {
    let provider_table = simple.tables.get("provider");
    let protocol = match (provider_table, simple.top.get("provider")) {
        (Some(table), None) => string_of(table, "protocol"),
        (None, Some(Val::Str(legacy))) => Some(legacy.clone()),
        _ => None,
    };
    let has_network_field = simple
        .top
        .keys()
        .chain(simple.tables.values().flat_map(BTreeMap::keys))
        .any(|k| k == "endpoint" || k == "credential_env_var");
    Facts {
        mock_replay: protocol.as_deref() == Some("mock-replay") && !has_network_field,
        transmission_log: string_of(&simple.top, "transmission_log"),
        credential_env_var: provider_table.and_then(|t| string_of(t, "credential_env_var")),
    }
}

fn read_facts(path: &Path) -> Result<Facts, String> {
    let meta = fs::metadata(path).map_err(|e| format!("'{}': {e}", path.display()))?;
    if !meta.is_file() || meta.len() > MAX_CONFIG_BYTES {
        return Err(format!(
            "'{}' is not a regular file of at most {MAX_CONFIG_BYTES} bytes",
            path.display()
        ));
    }
    let bytes = fs::read(path).map_err(|e| format!("'{}': {e}", path.display()))?;
    let text = String::from_utf8(bytes).map_err(|_| "the file is not UTF-8".to_owned())?;
    parse_simple(&text)
        .map(|s| facts_of(&s))
        .ok_or_else(|| "the file is outside the subset the guard reads".to_owned())
}

fn config_path(step: &Step, work_dir: &Path, name: &str) -> PathBuf {
    work_dir.join(&step.cwd).join(name)
}

/// Classifies a step. Only a `propose` step with exactly one readable
/// `--config` that is the `mock-replay` provider is offline among the
/// `ruletrace-ai` steps. Steps of other executables are offline.
#[must_use]
pub fn classify(step: &Step, work_dir: &Path) -> Class {
    if step.argv.first().map(String::as_str) != Some("ruletrace-ai") {
        return Class::Offline;
    }
    let offline = parse_ai_args(&step.argv).ok().is_some_and(|args| {
        args.command == "propose"
            && matches!(args.config.as_slice(), [config]
                if read_facts(&config_path(step, work_dir, config)).is_ok_and(|f| f.mock_replay))
    });
    if offline {
        Class::Offline
    } else {
        Class::Live
    }
}

/// Resolves a path without requiring it to exist: `..` and symbolic links are
/// followed component by component, as the operating system would.
fn resolve(path: &Path) -> PathBuf {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut current = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => current.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                current.pop();
            }
            Component::Normal(name) => {
                current.push(name);
                if let Ok(real) = fs::canonicalize(&current) {
                    current = real;
                }
            }
        }
    }
    current
}

fn has_golden_component(path: &Path) -> bool {
    path.components()
        .any(|c| c.as_os_str().eq_ignore_ascii_case("golden"))
}

/// The protected location `path` lies at or under, if any.
fn protected_location(path: &Path, demo_dir: &Path, work_dir: &Path) -> Option<String> {
    let resolved = resolve(path);
    let bases = [resolve(demo_dir), resolve(work_dir)];
    for base in &bases {
        for name in ["golden", "provider"] {
            let root = base.join(name);
            if resolved.starts_with(&root) {
                return Some(root.display().to_string());
            }
        }
    }
    let tail = bases
        .iter()
        .find_map(|b| resolved.strip_prefix(b).ok())
        .unwrap_or(&resolved);
    has_golden_component(tail).then(|| "a golden/ directory".to_owned())
}

fn refuse(step: &Step, refusal: Refusal) -> DemoError {
    DemoError::LiveRefused {
        step: step.id.clone(),
        refusal,
    }
}

/// Decides whether a live step may start. Nothing is started or written.
///
/// Order: executable, class, arguments, consent, manifest permission for each
/// named document, then the output locations (`--store`, `--out`, and the
/// `transmission_log` of the configuration).
///
/// # Errors
/// [`DemoError::Manifest`] naming the document and `transmission_permitted`
/// when a named document is not permitted or not in the manifest, otherwise
/// [`DemoError::LiveRefused`].
pub fn authorize_live(
    step: &Step,
    manifest: &Manifest,
    demo_dir: &Path,
    work_dir: &Path,
) -> Result<LiveAuthorization, DemoError> {
    if step.argv.first().map(String::as_str) != Some("ruletrace-ai") {
        return Err(refuse(step, Refusal::NotRuletraceAi));
    }
    if classify(step, work_dir) != Class::Live {
        return Err(refuse(step, Refusal::NotLive));
    }
    let args =
        parse_ai_args(&step.argv).map_err(|p| refuse(step, Refusal::UnreadableArguments(p)))?;
    if args.consent.is_empty() {
        return Err(refuse(step, Refusal::NoConsent));
    }
    for document in &args.consent {
        manifest.require_transmission(document)?;
    }
    let [config] = args.config.as_slice() else {
        return Err(refuse(
            step,
            Refusal::UnreadableConfig("exactly one --config is required".to_owned()),
        ));
    };
    let facts = read_facts(&config_path(step, work_dir, config))
        .map_err(|p| refuse(step, Refusal::UnreadableConfig(p)))?;
    let [store] = args.store.as_slice() else {
        return Err(refuse(step, Refusal::NoStore));
    };
    let log = facts.transmission_log.ok_or_else(|| {
        refuse(
            step,
            Refusal::UnreadableConfig("no string field transmission_log".to_owned()),
        )
    })?;
    let base = work_dir.join(&step.cwd);
    let mut locations = vec![
        ("--store", store.as_str()),
        ("transmission_log", log.as_str()),
    ];
    locations.extend(args.out.iter().map(|o| ("--out", o.as_str())));
    for (option, written) in locations {
        if let Some(protected) = protected_location(&base.join(written), demo_dir, work_dir) {
            return Err(refuse(
                step,
                Refusal::ForbiddenOutput {
                    option: option.to_owned(),
                    path: written.to_owned(),
                    protected,
                },
            ));
        }
    }
    Ok(LiveAuthorization {
        credential_env_var: facts.credential_env_var,
    })
}

#[cfg(test)]
mod reader_tests {
    use super::*;

    fn facts(text: &str) -> Option<Facts> {
        parse_simple(text).map(|s| facts_of(&s))
    }

    #[test]
    fn mock_replay_v2_is_offline_and_a_provider_with_an_endpoint_is_not() {
        let offline =
            "schema_version = 2\n[provider]\nid = \"mock-replay\"\nprotocol = \"mock-replay\"\n";
        assert!(facts(offline).unwrap().mock_replay);
        let with_endpoint = format!("{offline}endpoint = \"https://x\"\n");
        assert!(!facts(&with_endpoint).unwrap().mock_replay);
    }

    #[test]
    fn mock_replay_v1_is_offline() {
        assert!(
            facts("schema_version = 1\nprovider = \"mock-replay\"\n")
                .unwrap()
                .mock_replay
        );
    }

    #[test]
    fn a_protocol_in_another_table_does_not_count() {
        let text =
            "[other]\nprotocol = \"mock-replay\"\n[provider]\nprotocol = \"openai-responses\"\n";
        assert!(!facts(text).unwrap().mock_replay);
    }

    #[test]
    fn syntax_outside_the_subset_is_not_read() {
        for text in [
            "x = \"\"\"\n[provider]\nprotocol = \"mock-replay\"\n\"\"\"\n",
            "provider = { protocol = \"mock-replay\" }\n",
            "[provider]\nprotocol = \"mock-replay\"\nprotocol = \"mock-replay\"\n",
            "[provider]\n[provider]\n",
            "[a.b]\n",
            "a.b = \"x\"\n",
            "[provider]\nprotocol = \"mock-\\u0072eplay\"\n",
            "x = [\n 1,\n]\n",
        ] {
            assert!(facts(text).is_none(), "{text}");
        }
    }

    #[test]
    fn the_credential_variable_and_log_are_read() {
        let text = "transmission_log = \"t.jsonl\"\n[provider]\nprotocol = \"openai-responses\"\ncredential_env_var = \"K\"\n";
        let f = facts(text).unwrap();
        assert!(!f.mock_replay);
        assert_eq!(f.transmission_log.as_deref(), Some("t.jsonl"));
        assert_eq!(f.credential_env_var.as_deref(), Some("K"));
    }

    #[test]
    fn arguments_are_read_like_the_tool_reads_them() {
        let v = |s: &[&str]| s.iter().map(|x| (*x).to_owned()).collect::<Vec<_>>();
        let ok = parse_ai_args(&v(&[
            "ruletrace-ai",
            "propose",
            "--consent",
            "a",
            "--consent",
            "b",
            "--dry-run",
            "--config",
            "c.toml",
        ]))
        .unwrap();
        assert_eq!(ok.consent, ["a", "b"]);
        assert_eq!(ok.config, ["c.toml"]);
        for bad in [
            &["ruletrace-ai", "propose", "--consent=a"][..],
            &["ruletrace-ai", "propose", "--consent"],
            &["ruletrace-ai", "propose", "--consent", "--store"],
            &["ruletrace-ai", "propose", "--config", "a", "--config", "b"],
            &["ruletrace-ai", "propose", "stray"],
            &["ruletrace-ai"],
        ] {
            assert!(parse_ai_args(&v(bad)).is_err(), "{bad:?}");
        }
    }
}
