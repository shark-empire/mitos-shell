use std::env;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;

fn main() {
    // The main Read-Eval-Print Loop (REPL)
    loop {
        // 1. Generate the prompt with the current working directory
        let cwd = env::current_dir().unwrap_or_else(|_| Path::new("/").to_path_buf());
        print!("mitos {} > ", cwd.display());
        
        // Ensure the prompt is printed before reading input
        io::stdout().flush().unwrap();

        // 2. Read user input
        let mut input = String::new();
        if io::stdin().read_line(&mut input).unwrap_or(0) == 0 {
            println!();
            break; // EOF reached (e.g., Ctrl+D)
        }

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        // 3. Parse arguments
        let mut parts = input.split_whitespace();
        let command = parts.next().unwrap();
        let args = parts;

        // 4. Handle built-ins or execute external programs
        match command {
            "cd" => {
                // Default to root '/' if no directory is provided
                let new_dir = args.peekable().peek().map_or("/", |x| *x);
                let root = Path::new(new_dir);
                if let Err(e) = env::set_current_dir(&root) {
                    eprintln!("cd: {}: {}", new_dir, e);
                }
            }
            "exit" => {
                println!("Exiting mitos-shell...");
                break;
            }
            _ => {
                // Fork and execute external binaries (e.g., /bin/ls)
                let mut child = Command::new(command)
                    .args(args)
                    .spawn();

                match child {
                    Ok(mut process) => {
                        let _ = process.wait(); // Wait for the child to finish
                    }
                    Err(e) => eprintln!("mitos-shell: command not found or execution failed: {} ({})", command, e),
                }
            }
        }
    }
}
