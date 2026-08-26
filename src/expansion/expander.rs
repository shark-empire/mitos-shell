use std::env;
use glob::glob;
use crate::lexer::token::Token;
use crate::error::{Result, ShellError};
use crate::config::options::ShellOptions;

pub struct Expander {
    last_exit_code: i32,
    positional_args: Vec<String>,
    options: ShellOptions, // Added to support `set -u` (nounset)
    arrays: HashMap<String, Vec<String>>,
}

impl Expander {
    pub fn new(
        last_exit_code: i32,
        positional_args: Vec<String>,
        options: ShellOptions,
        arrays: HashMap<String, Vec<String>>, // Add this
    ) -> Self {
        Self {
            last_exit_code,
            positional_args,
            options,
            arrays,
        }
    }

    /// Expands variables, tildes, and globs for a list of tokens
    pub fn expand_tokens(&self, tokens: Vec<Token>) -> Result<Vec<String>> {
        let mut final_args = Vec::new();

        for token in tokens {
            match token {
                Token::SingleQuoted(s) => {
                    // Single quotes prevent ALL expansion
                    final_args.push(s); 
                }
                Token::DoubleQuoted(s) => {
                    // Double quotes allow variables, but prevent globbing
                    let expanded = self.expand_vars(&s)?;
                    final_args.push(expanded);
                }
                Token::Word(s) => {
            // If it contains ${, we need special array handling
            if s.contains("${") {
                let expanded = self.expand_braced(&s)?;
                final_args.extend(expanded); // Arrays can expand to MULTIPLE args!
            } else {
                    // Unquoted words get variables, tildes, AND globs
                    let mut expanded = self.expand_vars(&s)?;
                    expanded = self.expand_tilde(&expanded);
                    
                    // Globbing
                    if expanded.contains('*') || expanded.contains('?') || expanded.contains('[') {
                        if let Ok(paths) = glob(&expanded) {
                            let matches: Vec<_> = paths.filter_map(|p| p.ok()).collect();
                            if matches.is_empty() {
                                final_args.push(expanded);
                            } else {
                                for p in matches { 
                                    final_args.push(p.to_string_lossy().into_owned()); 
                                }
                            }
                        } else { 
                            final_args.push(expanded); 
                        }
                    } else {
                        final_args.push(expanded);
                    }
                }
                _ => {} // Ignore structural tokens here
            }
        }
        Ok(final_args)
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

                // If there is no variable name (just a stray '$'), keep it
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
                    // $1, $2, $3...
                    if index > 0 && index <= self.positional_args.len() {
                        result.push_str(&self.positional_args[index - 1]);
                    }
                } else if let Ok(value) = env::var(&var_name) {
                    result.push_str(&value);
                } else if self.options.nounset {
                    // `set -u` triggered: Unbound variable error!
                    return Err(ShellError::Execution(format!("{}: unbound variable", var_name)));
                }
                // If not found and nounset is false, it safely expands to an empty string.
            } else {
                result.push(c);
            }
        }
        
        Ok(result)
    }

    fn expand_tilde(&self, input: &str) -> String {
        if input == "~" {
            env::var("HOME").unwrap_or_else(|_| "/".to_string())
        } else if input.starts_with("~/") {
            let home = env::var("HOME").unwrap_or_else(|_| "/".to_string());
            format!("{}{}", home, &input[1..])
        } else {
            input.to_string()
        }
    }

    
    fn expand_braced(&self, input: &str) -> Result<Vec<String>> {
        // Simplified parser for ${name[@]}
        if let Some(start) = input.find("${") {
            if let Some(end) = input.find('}') {
                let inner = &input[start+2..end];
                if inner.ends_with("[@]") {
                    let name = &inner[..inner.len()-3];
                    if let Some(arr) = self.arrays.get(name) {
                        return Ok(arr.clone()); // Expands to multiple words!
                    }
                } else if inner.ends_with("[*]") {
                    let name = &inner[..inner.len()-3];
                    if let Some(arr) = self.arrays.get(name) {
                        return Ok(vec![arr.join(" ")]); // Expands to single word
                    }
                } else if inner.starts_with("#") && inner.ends_with("[@]") {
                    let name = &inner[1..inner.len()-3];
                    if let Some(arr) = self.arrays.get(name) {
                        return Ok(vec![arr.len().to_string()]);
                    }
                }
            }
        }
        Ok(vec![input.to_string()])
    }
}


