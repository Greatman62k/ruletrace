//! `ruletrace-ai propose` (design 3.9.2, 6.1; Req 16.16, 16.17, 27.3).
//!
//! The stages run in the order of Req 16.17 and the first failing stage
//! decides the exit code: arguments, configuration, credential and consent,
//! input load, then the call loop, whose failures carry the codes of the
//! request steps. Standard output and standard error are written only
//! through [`Out`]. The function opens no ledger file and writes neither
//! the Rulebook nor any file but the Proposal_Store and the
//! Transmission_Log. It reads no clock: the time is an argument.

use crate::cli::{parse, ProposeArgs};
use crate::config::{load, load_config, AiConfig, ProviderKind};
use crate::credential::{Credential, EnvReader};
use crate::dryrun::{announcement, dry_run};
use crate::fs::StoreFs;
use crate::inputs::{
    input_id, input_location, load_inputs, load_script, read_bounded, Inputs, MAX_CONFIG_BYTES,
};
use crate::out::Out;
use crate::perm::{file_mode, permission_warning};
use crate::plan::{build_plan, PlanInput, RunPlan};
use crate::redact::Redactor;
use crate::run::{run_requests, RunInput};
use crate::store::StoreMeta;
use crate::store_check::check_store;
use crate::txlog::{FileLog, TransmissionLog};
use crate::vocab::Ud32Option;
use rt_ai_adapter::{AiAdapter, AttemptReport, MonotonicClock, ProviderRequest};
use rt_ai_mock::ReplayProvider;
use rt_foundation::{CliLocation, Code, Diagnostic, DocumentId, ExitCode, Location, Severity};
use rt_ir::Identity;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

/// The exit code of a failed stage: that of the first blocking Diagnostic.
fn exit_of(diagnostics: &[Diagnostic]) -> ExitCode {
    diagnostics
        .iter()
        .find(|d| d.severity() != Severity::Warning)
        .map_or(ExitCode::Internal, |d| d.code().exit_code())
}

struct Reporter<'a, O: Write, E: Write> {
    out: &'a mut Out<O, E>,
    write_failed: bool,
}

impl<O: Write, E: Write> Reporter<'_, O, E> {
    fn diagnostics(&mut self, diagnostics: &[Diagnostic]) {
        let mut sorted = diagnostics.to_vec();
        sorted.sort();
        for d in &sorted {
            self.write_failed |= self.out.diagnostic(d).is_err();
        }
    }

    // A failed stage: its Diagnostics are written and its code decides.
    fn fail(&mut self, diagnostics: &[Diagnostic]) -> ExitCode {
        self.diagnostics(diagnostics);
        exit_of(diagnostics)
    }
}

fn command_at() -> Location {
    Location::Cli(CliLocation::Command("propose".to_owned()))
}

fn read_config(path: &Path) -> Result<String, Diagnostic> {
    let bytes = read_bounded(path, MAX_CONFIG_BYTES)?;
    if u64::try_from(bytes.len()).map_or(true, |n| n > MAX_CONFIG_BYTES) {
        return Err(Diagnostic::new(
            Code::AiConfigInvalid,
            "the configuration file is larger than 1 MiB".to_owned(),
            input_location(path),
        ));
    }
    String::from_utf8(bytes).map_err(|_| {
        Diagnostic::new(
            Code::AiConfigInvalid,
            "the configuration file is not valid UTF-8".to_owned(),
            input_location(path),
        )
    })
}

// The documents the run would send. When the inputs did not load, the
// consent set stands in, so that a bad input is reported at its own stage
// and not as a consent failure.
fn run_documents(
    inputs: &Result<Inputs, Vec<Diagnostic>>,
    consented: &BTreeSet<DocumentId>,
) -> BTreeSet<DocumentId> {
    match inputs {
        Ok(found) => found
            .candidates
            .iter()
            .map(|c| c.span().doc().clone())
            .collect(),
        Err(_) => consented.clone(),
    }
}

fn identities(config: &AiConfig, path: &Path) -> Result<Vec<Identity>, Diagnostic> {
    config
        .ai_identities
        .iter()
        .map(|name| {
            Identity::new(name.as_str()).map_err(|e| {
                Diagnostic::new(
                    Code::AiConfigInvalid,
                    format!("field `ai_identities` has an invalid identity: {e}"),
                    input_location(path),
                )
            })
        })
        .collect()
}

// Writes the announcement of Req 23.24 to standard error before an attempt
// reaches the provider. The Transmission_Log entry is already written.
struct Announcing<'a, O: Write, E: Write> {
    inner: Box<dyn AiAdapter + 'a>,
    plan: &'a RunPlan,
    config: &'a AiConfig,
    out: &'a mut Out<O, E>,
    write_failed: bool,
}

impl<O: Write, E: Write> AiAdapter for Announcing<'_, O, E> {
    fn attempt(&mut self, request: &ProviderRequest, timeout: Duration) -> AttemptReport {
        let planned = request.request_id().and_then(|id| {
            let text = id.as_str();
            self.plan
                .requests()
                .iter()
                .find(|p| p.id().as_str() == text)
        });
        if let Some(planned) = planned {
            let line = announcement(self.config, planned);
            self.write_failed |= self.out.stderr(&line).is_err();
        }
        self.inner.attempt(request, timeout)
    }

    fn clock(&self) -> &dyn MonotonicClock {
        self.inner.clock()
    }
}

fn unavailable_provider(config: &AiConfig) -> Diagnostic {
    Diagnostic::new(
        Code::AiProviderUnknown,
        format!(
            "this build links no integration for the provider kind '{}'",
            config.provider.name()
        ),
        config.provider_at.clone(),
    )
}

/// Where a run gets its provider adapter and the adapter version that the
/// stored batches record.
pub trait Providers {
    /// Whether this build can build an adapter for `kind`. It is asked
    /// at configuration time, so an unlinked kind is `RT-AI-0006` before
    /// consent, credential and input checks (Req 16.17, 28.17).
    fn is_linked(&self, _kind: ProviderKind) -> bool {
        true
    }

    /// The adapter for `config`. `credential` is present exactly when the
    /// provider kind needs one.
    ///
    /// # Errors
    /// A Diagnostic when no adapter can be built; it ends the run before any
    /// request is made.
    fn adapter<'a>(
        &self,
        config: &AiConfig,
        credential: Option<&Credential>,
    ) -> Result<(Box<dyn AiAdapter + 'a>, String), Diagnostic>;
}

/// The providers this build links: the offline replay of a recorded script.
/// No integration crate for a hosted provider is linked yet.
#[derive(Clone, Copy, Debug, Default)]
pub struct BuiltInProviders;

impl Providers for BuiltInProviders {
    fn is_linked(&self, kind: ProviderKind) -> bool {
        kind == ProviderKind::MockReplay
    }

    fn adapter<'a>(
        &self,
        config: &AiConfig,
        _credential: Option<&Credential>,
    ) -> Result<(Box<dyn AiAdapter + 'a>, String), Diagnostic> {
        match (config.provider, config.script.as_deref()) {
            (ProviderKind::MockReplay, Some(script_path)) => {
                let script = load_script(Path::new(script_path))?;
                let version = script.adapter_version.clone();
                Ok((Box::new(ReplayProvider::new(script)), version))
            }
            _ => Err(unavailable_provider(config)),
        }
    }
}

/// Runs `ruletrace-ai` with `args`, which exclude the program name, and
/// returns the exit code. Standard output and standard error are written
/// through `out`; the credential is read only through `env`.
pub fn run<O: Write, E: Write>(
    args: &[OsString],
    env: &dyn EnvReader,
    fs: &dyn StoreFs,
    out: &mut Out<O, E>,
) -> ExitCode {
    run_with(args, env, &BuiltInProviders, fs, out)
}

/// Like [`run`], with the adapter taken from `providers`.
pub fn run_with<O: Write, E: Write>(
    args: &[OsString],
    env: &dyn EnvReader,
    providers: &dyn Providers,
    fs: &dyn StoreFs,
    out: &mut Out<O, E>,
) -> ExitCode {
    let mut report = Reporter {
        out,
        write_failed: false,
    };
    let code = propose(args, env, providers, fs, &mut report);
    match (code, report.write_failed) {
        (ExitCode::Success, true) => ExitCode::Internal,
        (code, _) => code,
    }
}

fn propose<O: Write, E: Write>(
    args: &[OsString],
    env: &dyn EnvReader,
    providers: &dyn Providers,
    fs: &dyn StoreFs,
    report: &mut Reporter<'_, O, E>,
) -> ExitCode {
    let args = match parse(args) {
        Ok(args) => args,
        Err(problems) => return report.fail(&problems),
    };
    if args.revise.is_some() {
        return report.fail(&[Diagnostic::new(
            Code::CliArgumentInvalid,
            "proposing a revision of a Rule_Revision is not implemented in this build".to_owned(),
            command_at(),
        )]);
    }
    let text = match read_config(&args.config) {
        Ok(text) => text,
        Err(d) => return report.fail(&[d]),
    };
    let config_input = match input_id(&args.config) {
        Ok(input) => input,
        Err(d) => return report.fail(&[d]),
    };
    let inputs = load_inputs(&args);
    if args.dry_run {
        return dry(&args, &text, &config_input, inputs, report);
    }
    let consented = args.consent.named_documents();
    let documents = run_documents(&inputs, &consented);
    match load_config(&config_input, &text) {
        Ok(config) if !providers.is_linked(config.provider) => {
            return report.fail(&[unavailable_provider(&config)]);
        }
        Ok(_) => {}
        Err(found) => return report.fail(&found),
    }
    let authorized = match load(&config_input, &text, &args.consent, &documents, env) {
        Ok(authorized) => authorized,
        Err(found) => return report.fail(&found),
    };
    let config = authorized.config;
    let redactor = authorized
        .credential
        .as_ref()
        .map_or_else(Redactor::no_credential, Redactor::new);
    report.out.set_redactor(redactor.clone());
    if let Some(w) = permission_warning(&config_input, file_mode(&args.config)) {
        report.diagnostics(&[w]);
    }
    let inputs = match inputs {
        Ok(inputs) => inputs,
        Err(found) => return report.fail(&found),
    };
    report.diagnostics(&inputs.warnings);
    let ai_identities = match identities(&config, &args.config) {
        Ok(found) => found,
        Err(d) => return report.fail(&[d]),
    };
    let plan = match build_plan(&PlanInput {
        config: &config,
        models: &inputs.models,
        candidates: &inputs.candidates,
        rulebook: &inputs.rulebook,
        option: Ud32Option::CONFIRMED,
        candidates_at: input_location(&args.candidates),
    }) {
        Ok(plan) => plan,
        Err(found) => return report.fail(&found),
    };
    if let Err(found) = check_store(fs, &args.store) {
        return report.fail(&found);
    }
    let (adapter, adapter_version) =
        match providers.adapter(&config, authorized.credential.as_ref()) {
            Ok(found) => found,
            Err(d) => return report.fail(&[d]),
        };
    let meta = StoreMeta {
        provider: config.provider.name().to_owned(),
        model: config.model.clone(),
        adapter_version,
        ai_identities,
        created_at: args.created_at.clone(),
    };
    let mut log = TransmissionLog::new(
        FileLog::new(Path::new(&config.transmission_log).to_path_buf()),
        redactor.clone(),
        config.transmission_log_at.clone(),
    );
    let mut announcing = Announcing {
        inner: adapter,
        plan: &plan,
        config: &config,
        out: &mut *report.out,
        write_failed: false,
    };
    let done = run_requests(
        &RunInput {
            config: &config,
            plan: &plan,
            models: &inputs.models,
            declarations: &inputs.rulebook,
            redactor: &redactor,
            store_root: &args.store,
            meta: &meta,
            log_time: &args.created_at,
        },
        &mut announcing,
        &mut log,
        fs,
    );
    let announce_failed = announcing.write_failed;
    drop(announcing);
    report.write_failed |= announce_failed;
    report.diagnostics(&done.diagnostics);
    let summary = done.summary.render(&redactor);
    report.write_failed |= report.out.stdout(&summary).is_err();
    done.exit
}

// `--dry-run`: the configuration checks without consent and credential,
// the same inputs and plan as a run, then the identifiers. Nothing is
// sent, no credential is read and no file is written.
fn dry<O: Write, E: Write>(
    args: &ProposeArgs,
    text: &str,
    config_input: &rt_foundation::InputId,
    inputs: Result<Inputs, Vec<Diagnostic>>,
    report: &mut Reporter<'_, O, E>,
) -> ExitCode {
    let config = match load_config(config_input, text) {
        Ok(config) => config,
        Err(found) => return report.fail(&found),
    };
    if let Some(w) = permission_warning(config_input, file_mode(&args.config)) {
        report.diagnostics(&[w]);
    }
    let inputs = match inputs {
        Ok(inputs) => inputs,
        Err(found) => return report.fail(&found),
    };
    report.diagnostics(&inputs.warnings);
    let plan = match build_plan(&PlanInput {
        config: &config,
        models: &inputs.models,
        candidates: &inputs.candidates,
        rulebook: &inputs.rulebook,
        option: Ud32Option::CONFIRMED,
        candidates_at: input_location(&args.candidates),
    }) {
        Ok(plan) => plan,
        Err(found) => return report.fail(&found),
    };
    match dry_run(&config, &plan, report.out) {
        Ok(()) => ExitCode::Success,
        Err(_) => ExitCode::Internal,
    }
}
