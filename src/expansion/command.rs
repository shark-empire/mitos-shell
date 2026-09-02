// src/expansion/command.rs
use std::process::{Command, Stdio};

/// Expands $(...) and `...` command substitutions in a string.
pub fn expand_command_substitution(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut result = String::new();
    let mut i = 0;

    while i < chars.len() {
        // Handle $( ... )
        if i + 1 < chars.len() && chars[i] == '$' && chars[i + 1] == '(' {
            if let Some((command, end_index)) = extract_paren_substitution(&chars, i + 2) {
                let output = run_capture(&command);
                result.push_str(&output);
                i = end_index;
                continue;
            }
        }

        // Handle ` ... `
        if chars[i] == '`' {
            if let Some((command, end_index)) = extract_backtick_substitution(&chars, i + 1) {
                let output = run_capture(&command);
                result.push_str(&output);
                i = end_index;
                continue;
            }
        }

        result.push(chars[i]);
        i += 1;
    }

    result
}

/// Finds the matching ')' for a '$(' starting after the '('.
/// Handles nested substitutions. Returns (inner_command, index_after_closing_paren).
fn extract_paren_substitution(chars: &[char], start: usize) -> Option<(String, usize)> {
    let mut depth = 1;
    let mut i = start;

    while i < chars.len() {
        match chars[i] {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    let inner: String = chars[start..i].iter().collect();
                    return Some((inner, i + 1));
                }
            }
            _ => {}
        }
        i += 1;
    }

    None // Unmatched parenthesis
}

/// Finds the closing backtick for a `...` substitution.
fn extract_backtick_substitution(chars: &[char], start: usize) -> Option<(String, usize)> {
    let mut i = start;
    while i < chars.len() {
        if chars[i] == '`' {
            let inner: String = chars[start..i].iter().collect();
            return Some((inner, i + 1));
        }
        i += 1;
    }
    None
}

/// Runs a command in a subshell and captures stdout, stripping trailing newlines.
fn run_capture(command: &str) -> String {
    if command.trim().is_empty() {
        return String::new();
    }

    // Delegate to `sh -c` rather than naively splitting on whitespace and
    // exec'ing the first word: that would mishandle quoted arguments,
    // couldn't run pipelines/redirects inside the substitution, and
    // wouldn't expand `$VAR` references in the substituted text.
    let output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            // POSIX: strip trailing newlines from command substitution
            stdout.trim_end_matches('\n').to_string()
        }
        Err(_) => String::new(),
    }
}
