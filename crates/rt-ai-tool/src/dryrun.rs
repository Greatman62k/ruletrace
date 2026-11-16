//! The send announcement of Req 23.24 and `propose --dry-run`.

use crate::config::AiConfig;
use crate::out::Out;
use crate::plan::{PlannedRequest, RunPlan};
use rt_foundation::escape_controls;
use std::io::{self, Write};

/// The line written to standard error before a request is sent: provider
/// identifier, endpoint host, document identifiers, clause count and the
/// byte size of the request.
#[must_use]
pub fn announcement(config: &AiConfig, request: &PlannedRequest) -> String {
    let host = config
        .endpoint
        .as_ref()
        .map_or("(none)", |e| e.host.as_str());
    let documents = request
        .log_context()
        .documents
        .iter()
        .map(|(id, _)| format!("'{}'", escape_controls(id.as_str())))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "request {}: provider {}, endpoint host {}, documents [{}], clauses {}, bytes {}\n",
        request.id().as_str(),
        config.profile.id,
        escape_controls(host),
        documents,
        request.clauses().len(),
        request.bytes().len()
    )
}

/// Announces every request of `plan` on standard error and writes each
/// Request_Identifier to standard output, one per line. The function takes
/// no adapter and no Transmission_Log, so it sends and records nothing.
///
/// # Errors
/// The error of the underlying writer.
pub fn dry_run<O: Write, E: Write>(
    config: &AiConfig,
    plan: &RunPlan,
    out: &mut Out<O, E>,
) -> io::Result<()> {
    for request in plan.requests() {
        out.stderr(&format!(
            "dry run, nothing is sent: {}",
            announcement(config, request)
        ))?;
        out.stdout(&format!("{}\n", request.id().as_str()))?;
    }
    Ok(())
}
