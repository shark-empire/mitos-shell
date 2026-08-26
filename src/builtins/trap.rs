pub fn execute(args: &[String], traps: &mut std::collections::HashMap<String, String>) -> i32 {
    if args.len() < 3 {
        eprintln!("trap: usage: trap COMMAND SIGNAL");
        return 2;
    }

    let command = args[1].clone();
    for signal in args.iter().skip(2) {
        // Normalize signal names (e.g., SIGINT -> INT)
        let sig_name = signal.strip_prefix("SIG").unwrap_or(signal).to_uppercase();
        traps.insert(sig_name, command.clone());
    }
    0
}
