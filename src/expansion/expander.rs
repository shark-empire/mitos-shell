use std::collections::HashMap;
use std::env;
use glob::glob;
use crate::lexer::token::Token;
use crate::error::{Result, ShellError};
use crate::config::options::ShellOptions;

pub struct Expander {
    last_exit_code: i32,
    positional_args: Vec<String>,
    options: ShellOptions,
    arrays: HashMap<String, Vec<String>>,
}

impl Expander {
    pub fn new(
        last_exit_code: i32,
        positional_args: Vec<String>,
        options: ShellOptions,
        arrays: HashMap<String, Vec<String>>,
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
                    // Double quotes allow variables/arrays, but prevent globbing
                    if s.contains("${") {
                        let expanded = self.expand_braced(&s)?;
                        // In double quotes, arrays usually join into a single string or first element
                        final_args.extend(expanded); 
                    } else {
                        let expanded = self.expand_vars(&s)?;
                        final_args.push(expanded);
                    }
                }
                Token::Word(s) => {
                    // Unquoted words get variables, arrays, tildes, AND globs
                    if s.contains("${") {
                        let expanded = self.expand_braced(&s)?;
                        final_args.extend(expanded); // Arrays can expand to MULTIPLE args!
                    } else {
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
                } // <--- FIXED: Added missing closing brace for Token::Word
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
                } else if let Ok(value) = env::var(&var_name) {
                    result.push_str(&value);
                } else if self.options.nounset {
                    return Err(ShellError::Execution(format!("{}: unbound variable", var_name)));
                }
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

    /// Handles complex braced expansions like ${arr[@]}, ${arr[0]}, and ${#arr[@]}
    fn expand_braced(&self, input: &str) -> Result<Vec<String>> {
        if let Some(start) = input.find("${") {
            if let Some(end_offset) = input[start..].find('}') {
                let end = start + end_offset;
                let inner = &input[start + 2..end];
                let prefix = &input[..start];
                let suffix = &input[end + 1..];
                
                // 1. ${arr[@]} - Expands to multiple words
                if inner.ends_with("[@]") {
                    let name = &inner[..inner.len() - 3];
                    if let Some(arr) = self.arrays.get(name) {
                        if prefix.is_empty() && suffix.is_empty() {
                            return Ok(arr.clone()); 
                        }
                        // If surrounded by text, Bash concatenates to first/last. 
                        // We'll just return the array elements with prefix/suffix attached.
                        return Ok(arr.iter().map(|v| format!("{}{}{}", prefix, v, suffix)).collect());
                    }
                } 
                
                // 2. ${arr[*]} - Expands to single word joined by spaces
                else if inner.ends_with("[*]") {
                    let name = &inner[..inner.len() - 3];
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
