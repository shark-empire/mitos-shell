use crate::config::options::ShellOptions;
use crate::error::{Result, ShellError};
use crate::expansion::arithmetic;
use crate::expansion::command;
use crate::lexer::token::Token;
use glob::glob;
use std::collections::HashMap;
use std::env;

pub struct Expander {
    last_exit_code: i32,
    positional_args: Vec<String>,
    options: ShellOptions,
    arrays: HashMap<String, Vec<String>>,
    locals: HashMap<String, String>,
}

impl Expander {
    pub fn new(
        last_exit_code: i32,
        positional_args: Vec<String>,
        options: ShellOptions,
        arrays: HashMap<String, Vec<String>>,
        locals: HashMap<String, String>,
    ) -> Self {
        Self {
            last_exit_code,
            positional_args,
            options,
            arrays,
            locals,
        }
    }

    /// Expands variables, tildes, and globs for a list of tokens.
    ///
    /// `split` controls IFS field-splitting of the result of an unquoted
    /// `Token::Word` (e.g. `$LIST` where `$LIST` contains spaces becoming
    /// several arguments instead of one) — the way POSIX splits an
    /// unquoted expansion into fields. Pass `true` for places a shell
    /// actually field-splits (command arguments, `for`-loop word lists,
    /// array literals); pass `false` where a single value is required
    /// (assignment values, redirect targets, case subjects) — splitting
    /// those would silently truncate to the first field.
    pub fn expand_tokens(&self, tokens: Vec<Token>, split: bool) -> Result<Vec<String>> {
        let mut final_args = Vec::new();

        for token in tokens {
            match token {
                Token::SingleQuoted(s) => {
                    // Single quotes prevent ALL expansion
                    final_args.push(s);
                }
                Token::DoubleQuoted(s) => {
                    // Double quotes allow variables/arrays/command-substitution/
                    // arithmetic, but prevent globbing and field-splitting
                    if s.contains("${") {
                        let expanded = self.expand_braced(&s)?;
                        // In double quotes, arrays usually join into a single string or first element
                        final_args.extend(expanded);
                    } else {
                        let expanded = self.expand_string(&s)?;
                        final_args.push(expanded);
                    }
                }
                Token::Word(s) => {
                    // Unquoted words get variables, arrays, command
                    // substitution, arithmetic, tildes, field-splitting,
                    // AND globs
                    if s.contains("${") {
                        let expanded = self.expand_braced(&s)?;
                        final_args.extend(expanded); // Arrays can expand to MULTIPLE args!
                    } else {
                        // `expand_word` (rather than a plain expand_string)
                        // handles words that quoted part of themselves —
                        // e.g. `FOO="bar baz"` or `prefix"suffix"` — since
                        // the lexer only splits a quote into its own token
                        // when it *starts* the word.
                        let (raw_expanded, had_quotes) = self.expand_word(&s)?;
                        let tilded = self.expand_tilde(&raw_expanded);

                        // Field-splitting only applies to words that
                        // didn't quote any part of themselves — quoted
                        // content stays one field regardless of internal
                        // whitespace, same rule real shells use.
                        let fields: Vec<String> = if !split || had_quotes {
                            vec![tilded]
                        } else {
                            self.split_fields(&tilded)
                        };

                        for field in fields {
                            // Pathname expansion is likewise skipped for
                            // words that quoted part of themselves:
                            // correctly globbing only the *unquoted*
                            // metacharacters within one mixed word needs
                            // pattern-escaping this doesn't attempt yet.
                            let should_glob = !had_quotes
                                && (field.contains('*')
                                    || field.contains('?')
                                    || field.contains('['));

                            if should_glob {
                                if let Ok(paths) = glob(&field) {
                                    let matches: Vec<_> = paths.filter_map(|p| p.ok()).collect();
                                    if matches.is_empty() {
                                        final_args.push(field);
                                    } else {
                                        for p in matches {
                                            final_args.push(p.to_string_lossy().into_owned());
                                        }
                                    }
                                } else {
                                    final_args.push(field);
                                }
                            } else {
                                final_args.push(field);
                            }
                        }
                    }
                }
                _ => {} // Ignore structural tokens here
            }
        }
        Ok(final_args)
    }

    /// Expands a raw string the way a double-quoted word is expanded:
    /// arithmetic expansion (`$((expr))`), then command substitution
    /// (`$(cmd)` / `` `cmd` ``), then parameter/variable expansion.
    /// Arithmetic must run before command substitution — a `$((` prefix
    /// would otherwise look like a command substitution whose "command"
    /// is `(expr)`, corrupting the expression instead of evaluating it.
    ///
    /// This is shared by command-line word expansion (via
    /// [`Expander::expand_tokens`]/[`Expander::expand_word`]) and by
    /// heredoc bodies, which undergo the same expansions but are never
    /// split into shell tokens.
    pub fn expand_string(&self, input: &str) -> Result<String> {
        let after_arith = arithmetic::expand_arithmetic(input);
        let after_cmd = command::expand_command_substitution(&after_arith);
        self.expand_vars(&after_cmd)
    }

    /// Expands a word whose raw text may contain quote regions that
    /// don't start the word — e.g. `FOO="bar baz"` or `prefix"suffix"` —
    /// which the lexer keeps together as a single `Token::Word` (a quote
    /// only gets split into its own `SingleQuoted`/`DoubleQuoted` token
    /// when it's the very first character of the word; see the lexer).
    ///
    /// Splits the text into bare/double/single-quoted regions, expands
    /// and strips each according to its own quoting rules (bare and
    /// double-quoted regions get [`Expander::expand_string`]; single
    /// quotes suppress all expansion, with no escapes), and concatenates
    /// the results. Returns the expanded value together with whether the
    /// word contained any quoting, so callers can skip field-splitting
    /// and pathname expansion for it — getting those fully right for a
    /// mixed word (splitting/globbing only the *unquoted* portions)
    /// needs machinery this keeps out of scope for now.
    fn expand_word(&self, input: &str) -> Result<(String, bool)> {
        enum State {
            Bare,
            Single,
            Double,
        }

        let chars: Vec<char> = input.chars().collect();
        let mut state = State::Bare;
        let mut had_quotes = false;
        let mut result = String::new();
        let mut region = String::new();
        let mut i = 0;

        while i < chars.len() {
            let c = chars[i];
            match state {
                State::Bare => match c {
                    '\'' => {
                        result.push_str(&self.expand_string(&region)?);
                        region.clear();
                        had_quotes = true;
                        state = State::Single;
                    }
                    '"' => {
                        result.push_str(&self.expand_string(&region)?);
                        region.clear();
                        had_quotes = true;
                        state = State::Double;
                    }
                    _ => region.push(c),
                },
                State::Single => {
                    // Single quotes suppress all expansion, with no escapes.
                    if c == '\'' {
                        result.push_str(&region);
                        region.clear();
                        state = State::Bare;
                    } else {
                        region.push(c);
                    }
                }
                State::Double => {
                    if c == '"' {
                        result.push_str(&self.expand_string(&region)?);
                        region.clear();
                        state = State::Bare;
                    } else if c == '\\' && i + 1 < chars.len() {
                        region.push(chars[i + 1]);
                        i += 1;
                    } else {
                        region.push(c);
                    }
                }
            }
            i += 1;
        }

        // An unterminated quote at the end of input: flush whatever's
        // left using the same rule as its region (only Bare gets
        // expanded).
        match state {
            State::Bare => result.push_str(&self.expand_string(&region)?),
            _ => result.push_str(&region),
        }

        Ok((result, had_quotes))
    }

    /// Splits an already-expanded, unquoted value into fields on `$IFS`
    /// (default: space/tab/newline), the way POSIX field-splits the
    /// result of an unquoted expansion. An empty `$IFS` disables
    /// splitting entirely. Whitespace IFS characters collapse runs and
    /// trim the edges (matching the common default); any other IFS
    /// character is treated as an exact per-occurrence delimiter (so
    /// e.g. `IFS=,` on `"a,,b"` yields three fields, matching real
    /// shells) rather than collapsing like whitespace does.
    fn split_fields(&self, value: &str) -> Vec<String> {
        let ifs = self
            .locals
            .get("IFS")
            .cloned()
            .or_else(|| env::var("IFS").ok())
            .unwrap_or_else(|| " \t\n".to_string());

        // An empty $IFS disables splitting: the whole value is one field.
        if ifs.is_empty() {
            return vec![value.to_string()];
        }
        // An empty value splits into zero fields (matches `X=""; echo
        // $X` producing no argument at all).
        if value.is_empty() {
            return Vec::new();
        }

        let is_ifs = |c: char| ifs.contains(c);
        let all_whitespace_ifs = ifs.chars().all(|c| c == ' ' || c == '\t' || c == '\n');

        if all_whitespace_ifs {
            value
                .split(is_ifs)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect()
        } else {
            value.split(is_ifs).map(|s| s.to_string()).collect()
        }
    }

    fn expand_vars(&self, input: &str) -> Result<String> {
        let mut result = String::new();
        let mut chars = input.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '$' {
                let mut var_name = String::new();

                // Collect the variable name
                while let Some(&next) = chars.peek() {
                    if next.is_alphanumeric() || next == '_' {
                        var_name.push(next);
                        chars.next();
                    } else {
                        break;
                    }
                }

                if var_name.is_empty() {
                    result.push('$');
                    continue;
                }

                if var_name == "?" {
                    result.push_str(&self.last_exit_code.to_string());
                } else if var_name == "#" {
                    result.push_str(&self.positional_args.len().to_string());
                } else if var_name == "@" || var_name == "*" {
                    result.push_str(&self.positional_args.join(" "));
                } else if let Ok(index) = var_name.parse::<usize>() {
                    if index > 0 && index <= self.positional_args.len() {
                        result.push_str(&self.positional_args[index - 1]);
                    }
                } else if let Some(value) = self.locals.get(&var_name) {
                    result.push_str(value);
                } else if let Ok(value) = env::var(&var_name) {
                    result.push_str(&value);
                } else if self.options.nounset {
                    return Err(ShellError::Execution(format!(
                        "{}: unbound variable",
                        var_name
                    )));
                }
            } else {
                result.push(c);
            }
        }

        Ok(result)
    }

    fn expand_tilde(&self, input: &str) -> String {
        let home = || {
            self.locals
                .get("HOME")
                .cloned()
                .or_else(|| env::var("HOME").ok())
                .unwrap_or_else(|| "/".to_string())
        };
        if input == "~" {
            home()
        } else if input.starts_with("~/") {
            format!("{}{}", home(), &input[1..])
        } else {
            input.to_string()
        }
    }

    /// Handles complex braced expansions like ${arr[@]}, ${arr[0]}, and ${#arr[@]}
    fn expand_braced(&self, input: &str) -> Result<Vec<String>> {
        if let Some(start) = input.find("${") {
            if let Some(end_offset) = input[start..].find('}') {
                let end = start + end_offset;
                let inner = &input[start + 2..end];
                let prefix = &input[..start];
                let suffix = &input[end + 1..];

                // 1. ${arr[@]} - Expands to multiple words
                if let Some(name) = inner.strip_suffix("[@]") {
                    if let Some(arr) = self.arrays.get(name) {
                        if prefix.is_empty() && suffix.is_empty() {
                            return Ok(arr.clone());
                        }
                        // If surrounded by text, Bash concatenates to first/last.
                        // We'll just return the array elements with prefix/suffix attached.
                        return Ok(arr
                            .iter()
                            .map(|v| format!("{}{}{}", prefix, v, suffix))
                            .collect());
                    }
                }
                // 2. ${arr[*]} - Expands to single word joined by spaces
                else if let Some(name) = inner.strip_suffix("[*]") {
                    if let Some(arr) = self.arrays.get(name) {
                        let joined = arr.join(" ");
                        return Ok(vec![format!("{}{}{}", prefix, joined, suffix)]);
                    }
                }
                // 3. ${#arr[@]} - Array length
                else if inner.starts_with('#') && inner.ends_with("[@]") {
                    let name = &inner[1..inner.len() - 3];
                    if let Some(arr) = self.arrays.get(name) {
                        return Ok(vec![format!("{}{}{}", prefix, arr.len(), suffix)]);
                    }
                }
                // 4. ${arr[index]} - Scalar array access (FIXED)
                else if inner.contains('[') && inner.ends_with(']') {
                    let bracket_pos = inner.find('[').unwrap();
                    let name = &inner[..bracket_pos];
                    let index_str = &inner[bracket_pos + 1..inner.len() - 1];

                    if let Some(arr) = self.arrays.get(name) {
                        if let Ok(idx) = index_str.parse::<usize>() {
                            let val = arr.get(idx).cloned().unwrap_or_default();
                            return Ok(vec![format!("{}{}{}", prefix, val, suffix)]);
                        }
                    }
                }
            }
        }

        // Fallback: Not an array expansion, treat as normal string
        Ok(vec![input.to_string()])
    }
}
