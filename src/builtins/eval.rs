// src/builtins/eval.rs
pub fn execute(args: &[String]) -> Option<String> {
    if args.len() < 2 { return None; }
    Some(args[1..].join(" "))
}
