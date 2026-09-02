use crate::completion::engine::MitosCompleter;
use crate::lexer::token::Token;
use crate::lexer::tokenize_with_eof;
use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::{Context, Helper};
use std::borrow::Cow;

pub struct MitosHelper;

impl Helper for MitosHelper {}

// ---- Completion ----
// Delegates to the shared completion engine (`MitosCompleter`) instead of
// re-implementing command/file/variable completion here.
impl Completer for MitosHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &Context<'_>,
    ) -> Result<(usize, Vec<Pair>), rustyline::error::ReadlineError> {
        MitosCompleter.complete(line, pos, ctx)
    }
}

// ---- Multi-line validation ----
impl Validator for MitosHelper {
    fn validate(&self, ctx: &mut ValidationContext) -> rustyline::Result<ValidationResult> {
        Ok(if is_complete(ctx.input()) {
            ValidationResult::Valid(None)
        } else {
            ValidationResult::Incomplete
        })
    }
}

impl Hinter for MitosHelper {
    type Hint = String;
    fn hint(&self, _line: &str, _pos: usize, _ctx: &Context<'_>) -> Option<String> {
        None
    }
}

impl Highlighter for MitosHelper {
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        _default: bool,
    ) -> Cow<'b, str> {
        Cow::Owned(prompt.to_string())
    }
}

/// Heuristic completeness check: balanced if/fi, while|for/done, {/}, (/),
/// closed quotes, and no trailing continuation operator.
fn is_complete(input: &str) -> bool {
    let trimmed = input.trim_end();
    if trimmed.ends_with('\\') {
        return false;
    }
    if trimmed.ends_with("&&") || trimmed.ends_with("||") || trimmed.ends_with('|') {
        return false;
    }

    let (mut if_n, mut done_n, mut brace, mut paren) = (0i32, 0i32, 0i32, 0i32);
    for tok in tokenize_with_eof(input) {
        match tok {
            Token::Word(w) => match w.as_str() {
                "if" => if_n += 1,
                "fi" => if_n -= 1,
                "while" | "for" => done_n += 1,
                "done" => done_n -= 1,
                _ => {}
            },
            Token::LeftBrace => brace += 1,
            Token::RightBrace => brace -= 1,
            Token::LeftParen => paren += 1,
            Token::RightParen => paren -= 1,
            _ => {}
        }
    }

    // Quote balance scan.
    let (mut single, mut double) = (false, false);
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            '\'' if !double => single = !single,
            '"' if !single => double = !double,
            _ => {}
        }
    }
    if single || double {
        return false;
    }

    if_n <= 0 && done_n <= 0 && brace <= 0 && paren <= 0
}
