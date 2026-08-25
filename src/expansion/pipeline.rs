// src/expansion/pipeline.rs
use super::{expander::Expander, command, arithmetic};
use crate::builtins::alias;

pub struct ExpansionPipeline {
    expander: Expander,
}

impl ExpansionPipeline {
    pub fn new(last_status: i32) -> Self {
        Self { expander: Expander::new(last_status) }
    }

    /// Full POSIX expansion order:
    /// 1. Alias expansion (before tokenization)
    /// 2. Command substitution
    /// 3. Arithmetic expansion
    /// 4. Variable expansion
    /// 5. Tilde expansion
    /// 6. Pathname (glob) expansion
    pub fn expand_line(&self, line: &str) -> String {
        let after_alias = alias::expand(line);
        let after_cmd_sub = command::expand_command_substitution(&after_alias);
        let after_arith = arithmetic::expand_arithmetic(&after_cmd_sub);
        after_arith
    }

    /// Expands a tokenized argument list (variables, tilde, globs).
    pub fn expand_args(&self, args: Vec<String>) -> Vec<String> {
        self.expander.expand_args(args)
    }
}
