use std::env;
use glob::glob;

pub struct Expander {
    last_exit_code: i32,
    positional_args: Vec<String>,
}

impl Expander {
    pub fn new(
        last_exit_code: i32,
        positional_args: Vec<String>,
    ) -> Self {
        Self {
            last_exit_code,
            positional_args,
        }
    }



    /// Expands variables, tildes, and globs for a list of arguments
    pub fn expand_args(&self, args: Vec<String>) -> Vec<String> {
        let mut final_args = Vec::new();

        for arg in args {
            let mut expanded = self.expand_vars(&arg);
            expanded = self.expand_tilde(&expanded);
            
            // Globbing (Wildcard Expansion)
            if expanded.contains('*') || expanded.contains('?') || expanded.contains('[') {
                if let Ok(paths) = glob(&expanded) {
                    let matches: Vec<_> = paths.filter_map(Result::ok).collect();
                    if matches.is_empty() {
                        final_args.push(expanded); // Fallback to literal if no match
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
        final_args
    }

    fn expand_vars(&self, input: &str) -> String {
        let mut result = String::new();
        let mut chars = input.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '$' {
                let mut var_name = String::new();
                while let Some(&next) = chars.peek() {
                    if next.is_alphanumeric() || next == '_' {
                        var_name.push(next);
                        chars.next();
                    } else {
                        break;
                    }
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
} else if let Ok(value) = std::env::var(&var_name) {
                    result.push_str(&value);
                 }

                } 
        result
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
}
