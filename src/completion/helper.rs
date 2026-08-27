use crate::lexer::lexer::Lexer;
use crate::lexer::token::Token;
use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::{Context, Helper};
use std::borrow::Cow;

pub struct MitosHelper;

impl Helper for MitosHelper {}

// ---- Completion (from Phase 4) ----
impl Completer for MitosHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> Result<(usize, Vec<Pair>), rustyline::error::ReadlineError> {
        let (start, word) = extract_word(line, pos);

        if let Some(var_prefix) = word.strip_prefix('$') {
            return Ok((start, complete_variables(var_prefix)));
        }

        let is_first = line[..start].trim().is_empty()
            || line[..start].trim_end().ends_with([';', '|']);
        if is_first {
            let mut c = complete_commands(&word);
            c.extend(complete_files(&word));
            return Ok((start, c));
        }

        Ok((start, complete_files(&word)))
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
    for tok in Lexer::new(input) {
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

// ---------- completion sources (Phase 4) ----------
fn extract_word(line: &str, pos: usize) -> (usize, String) {
    let before = &line[..pos];
    let start = before
        .rfind(|c: char| c.is_whitespace() || "|;<>(){}!".contains(c))
        .map(|i| i + 1)
        .unwrap_or(0);
    (start, before[start..].to_string())
}

fn complete_commands(prefix: &str) -> Vec<Pair> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for e in entries.flatten() {
                    let name = e.file_name().to_string_lossy().into_owned();
                    if name.starts_with(prefix) && seen.insert(name.clone()) {
                        out.push(Pair {
                            display: name.clone(),
                            replacement: name,
                        });
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| a.display.cmp(&b.display));
    out
}

fn complete_files(prefix: &str) -> Vec<Pair> {
    let mut out = Vec::new();
    let (dir, partial) = split_path(prefix);
    let search = if dir.is_empty() {
        std::path::Path::new(".")
    } else {
        std::path::Path::new(&dir)
    };
    if let Ok(entries) = std::fs::read_dir(search) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with(&partial) {
                let mut repl = if dir.is_empty() {
                    name.clone()
                } else {
                    format!("{}/{}", dir.trim_end_matches('/'), name)
                };
                if e.path().is_dir() {
                    repl.push('/');
                }
                out.push(Pair {
                    display: name,
                    replacement: repl,
                });
            }
        }
    }
    out.sort_by(|a, b| a.display.cmp(&b.display));
    out
}

fn split_path(prefix: &str) -> (String, String) {
    match prefix.rfind('/') {
        Some(i) => (prefix[..=i].to_string(), prefix[i + 1..].to_string()),
        None => (String::new(), prefix.to_string()),
    }
}

fn complete_variables(prefix: &str) -> Vec<Pair> {
    std::env::vars()
        .filter(|(k, _)| k.starts_with(prefix))
        .map(|(k, _)| Pair {
            display: format!("${}", k),
            replacement: format!("${}", k),
        })
        .collect()
}
