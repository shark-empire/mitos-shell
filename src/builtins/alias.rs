// src/builtins/alias.rs
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

// No lazy_static dependency is declared in Cargo.toml, so use a OnceLock instead.
fn aliases() -> &'static Mutex<HashMap<String, String>> {
    static ALIASES: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    ALIASES.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn set(name: &str, value: &str) {
    aliases()
        .lock()
        .unwrap()
        .insert(name.to_string(), value.to_string());
}

pub fn get(name: &str) -> Option<String> {
    aliases().lock().unwrap().get(name).cloned()
}

pub fn remove(name: &str) -> bool {
    aliases().lock().unwrap().remove(name).is_some()
}

pub fn list() {
    let aliases = aliases().lock().unwrap();
    for (name, value) in aliases.iter() {
        println!("alias {}='{}'", name, value);
    }
}

/// Expands aliases in a command line. Prevents infinite recursion by
/// tracking already-expanded aliases in the current pass.
pub fn expand(line: &str) -> String {
    let mut tokens: Vec<String> = line.split_whitespace().map(|s| s.to_string()).collect();

    if tokens.is_empty() {
        return line.to_string();
    }

    let mut expanded = std::collections::HashSet::new();
    let mut changed = true;

    // Repeatedly expand the first token if it's an alias (handles chained aliases)
    while changed {
        changed = false;
        if let Some(first) = tokens.first() {
            if !expanded.contains(first) {
                if let Some(replacement) = get(first) {
                    expanded.insert(first.clone());
                    let new_tokens: Vec<String> = replacement
                        .split_whitespace()
                        .map(|s| s.to_string())
                        .chain(tokens.iter().skip(1).cloned())
                        .collect();
                    tokens = new_tokens;
                    changed = true;
                }
            }
        }
    }

    tokens.join(" ")
}
