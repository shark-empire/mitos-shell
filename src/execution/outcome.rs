#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExecOutcome {
    /// Normal completion with an exit status.
    Status(i32),
    /// `break` — unwind to the nearest enclosing loop.
    Break,
    /// `continue` — jump to the next loop iteration.
    Return(i32),
    /// `return` — exit the current function.
    Continue,
    /// `exit` — terminate the whole shell.
    Exit(i32),
}

impl ExecOutcome {
    pub fn status_or_zero(self) -> i32 {
        match self {
            ExecOutcome::Status(s) | ExecOutcome::Return(s) | ExecOutcome::Exit(s) => s,
            _ => 0,
        }
    }
}
