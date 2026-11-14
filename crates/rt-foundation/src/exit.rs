//! Process exit codes of the executables (Req 16).

/// An exit code of `ruletrace` or `ruletrace-ai`. The catalogue gives each code exactly one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExitCode {
    /// 0: every stage succeeded. Warnings never change it.
    Success,
    /// 2: invalid input (file, command line, ledger, configuration, store).
    InvalidInput,
    /// 3: a validation, compilation, review or proposal check found a blocking Diagnostic.
    Blocked,
    /// 4: evaluation produced an Indeterminate Outcome or an execution error.
    Indeterminate,
    /// 5: the AI provider call failed.
    Provider,
    /// 6: the provider response was rejected.
    Response,
    /// 70: internal error or output failure.
    Internal,
}

impl ExitCode {
    /// The numeric process exit status.
    #[must_use]
    pub fn value(self) -> u8 {
        match self {
            ExitCode::Success => 0,
            ExitCode::InvalidInput => 2,
            ExitCode::Blocked => 3,
            ExitCode::Indeterminate => 4,
            ExitCode::Provider => 5,
            ExitCode::Response => 6,
            ExitCode::Internal => 70,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ExitCode;

    #[test]
    fn values_are_the_documented_statuses() {
        let all = [
            (ExitCode::Success, 0),
            (ExitCode::InvalidInput, 2),
            (ExitCode::Blocked, 3),
            (ExitCode::Indeterminate, 4),
            (ExitCode::Provider, 5),
            (ExitCode::Response, 6),
            (ExitCode::Internal, 70),
        ];
        for (code, value) in all {
            assert_eq!(code.value(), value);
        }
    }
}
