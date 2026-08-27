// src/expansion/pipeline.rs
use super::{arithmetic, command};
use crate::builtins::alias;

/// Line-level pre-expansion pass, run once on the raw input line before
/// lexing: alias expansion (textual), then command substitution
/// ($(...) / `...`), then arithmetic expansion ($(( expr )) ).
pub fn expand_line(line: &str) -> String {
    let after_alias = alias::expand(line);
    let after_cmd_sub = command::expand_command_substitution(&after_alias);
    arithmetic::expand_arithmetic(&after_cmd_sub)
}
