// src/expansion/pipeline.rs
use super::{arithmetic, command, expander::Expander};
use crate::builtins::alias;
use crate::config::options::ShellOptions;
use crate::lexer::token::Token;
use std::collections::HashMap;

pub struct ExpansionPipeline {
    expander: Expander,
}

impl ExpansionPipeline {
    pub fn new(last_status: i32) -> Self {
        Self {
            expander: Expander::new(
                last_status,
                Vec::new(),
                ShellOptions::default(),
                HashMap::new(),
            ),
        }
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
        arithmetic::expand_arithmetic(&after_cmd_sub)
    }

    /// Expands a tokenized argument list (variables, tilde, globs).
    pub fn expand_args(&self, args: Vec<String>) -> Vec<String> {
        let tokens: Vec<Token> = args.into_iter().map(Token::Word).collect();
        self.expander.expand_tokens(tokens).unwrap_or_default()
    }
}
