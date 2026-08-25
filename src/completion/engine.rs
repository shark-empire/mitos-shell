// src/completion/engine.rs
use rustyline::completion::{Completer, Pair};
use rustyline::Context;
use rustyline::error::ReadlineError;
use std::fs;
use std::path::Path;

pub struct MitosCompleter;

impl Completer for MitosCompleter {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> Result<(usize, Vec<Pair>), ReadlineError> {
        let (start, word) = extract_word(line, pos);

        // Variable completion: word starts with $
        if let Some(var_prefix) = word.strip_prefix('$') {
            let candidates = complete_variables(var_prefix);
            return Ok((start, candidates));
        }

        // Command completion: first word on the line
        let is_first_word = !line[..start].trim().contains(' ')
            || line[..start].trim().ends_with(|c| c == ';' || c == '|');
        if is_first_word {
            let mut candidates = complete_commands(&word);
            candidates.extend(complete_files(&word));
            return Ok((start, candidates));
        }

        // Argument position: file/directory completion
        Ok((start, complete_files(&word)))
    }
}

/// Finds the word being typed and its start index.
fn extract_word(line: &str, pos: usize) -> (usize, String) {
    let before = &line[..pos];
    let start = before
        .rfind(|c: char| c.is_whitespace() || c == '|' || c == ';' || c == '<' || c == '>')
        .map(|i| i + 1)
        .unwrap_or(0);
    (start, before[start..].to_string())
}

/// Completes against executables found in $PATH.
fn complete_commands(prefix: &str) -> Vec<Pair> {
    let mut seen = std::collections::HashSet::new();
    let mut candidates = Vec::new();

    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name.starts_with(prefix) && seen.insert(name.clone()) {
                        candidates.push(Pair {
                            display: name.clone(),
                            replacement: name,
                        });
                    }
                }
            }
        }
    }

    candidates.sort_by(|a, b| a.display.cmp(&b.display));
    candidates
}

/// Completes file and directory names relative to the current directory.
fn complete_files(prefix: &str) -> Vec<Pair> {
    let mut candidates = Vec::new();
    let (dir, partial) = split_path(prefix);
    let search_dir = if dir.is_empty() { Path::new(".") } else { Path::new(&dir) };

    if let Ok(entries) = fs::read_dir(search_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(&partial) {
                let mut replacement = if dir.is_empty() {
                    name.clone()
                } else {
                    format!("{}/{}", dir.trim_end_matches('/'), name)
                };

                // Append '/' to directories for easier navigation
                if entry.path().is_dir() {
                    replacement.push('/');
                }

                candidates.push(Pair { display: name, replacement });
            }
        }
    }

    candidates.sort_by(|a, b| a.display.cmp(&b.display));
    candidates
}

fn split_path(prefix: &str) -> (String, String) {
    match prefix.rfind('/') {
        Some(idx) => (prefix[..=idx].to_string(), prefix[idx + 1..].to_string()),
        None => (String::new(), prefix.to_string()),
    }
}

/// Completes environment variable names after '$'.
fn complete_variables(prefix: &str) -> Vec<Pair> {
    std::env::vars()
        .filter(|(k, _)| k.starts_with(prefix))
        .map(|(k, _)| Pair {
            display: format!("${}", k),
            replacement: format!("${}", k),
        })
        .collect()
}
