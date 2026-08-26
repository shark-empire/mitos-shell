#[derive(Debug, Clone, PartialEq)]
pub enum ExecOutcome {
    Status(i32),
    Break,
    Return(i32),
    Continue,
    Exit(i32),
    Eval(String), // NEW: Tells executor to parse and run this string
}

impl ExecOutcome {
    pub fn status_or_zero(self) -> i32 {
        match self {
            ExecOutcome::Status(s) | ExecOutcome::Return(s) | ExecOutcome::Exit(s) => s,
            _ => 0,
        }
    }
}
