pub fn try_execute(args: &[String]) -> Option<i32> {
    if args.is_empty() { return None; }
    
    match args[0].as_str() {
        "cd" => {
            let dir = args.get(1).map(|s| s.as_str()).unwrap_or("~");
            let path = if dir == "~" {
                dirs::home_dir().unwrap_or_default()
            } else {
                std::path::PathBuf::from(dir)
            };
            
            if let Err(e) = std::env::set_current_dir(path) {
                eprintln!("cd: {}", e);
                Some(1)
            } else {
                Some(0)
            }
        }
        "exit" => {
            let code = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            std::process::exit(code);
        }
        "pwd" => {
            println!("{}", std::env::current_dir().unwrap_or_default().display());
            Some(0)
        }
        _ => None // Not a builtin, fallback to external execution
    }
}
