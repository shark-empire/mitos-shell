mod builtins;
mod completion;
mod config;
mod error;
mod execution;
mod expansion;
mod lexer;
mod parser;
mod process;
mod shell;
mod terminal;
mod util;

use execution::executor::Executor;
use lexer::lexer::Lexer;
use nix::sys::signal::{signal, SigHandler, Signal};
use parser::parser::Parser;
use shell::session::Session;
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};

pub static INTERRUPTED: AtomicBool = AtomicBool::new(false);

fn main() {
    // Register signal handler
    ctrlc::set_handler(move || {
        INTERRUPTED.store(true, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");
    // Ignore interactive signals if running a script
    unsafe {
        let _ = signal(Signal::SIGINT, SigHandler::SigIgn);
        let _ = signal(Signal::SIGQUIT, SigHandler::SigIgn);
    }

    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 {
        // SCRIPT MODE: mitos script.sh arg1 arg2
        let script_path = &args[1];
        let script_args = args[2..].to_vec();

        match fs::read_to_string(script_path) {
            Ok(content) => {
                let mut executor = Executor::new();
                // Push script args into the base context
                executor.push_context(script_args);

                let tokens: Vec<_> = Lexer::new(&content).collect();
                match Parser::new(tokens).parse() {
                    Ok(ast) => {
                        if let Ok(Some(code)) = executor.execute(ast) {
                            std::process::exit(code);
                        }
                        std::process::exit(executor.last_status());
                    }
                    Err(e) => {
                        eprintln!("mitos: {}: syntax error: {}", script_path, e);
                        std::process::exit(2);
                    }
                }
            }
            Err(e) => {
                eprintln!("mitos: {}: {}", script_path, e);
                std::process::exit(1);
            }
        }
    } else {
        // INTERACTIVE MODE: Drop into REPL
        match Session::init() {
            Ok(mut session) => {
                let code = session.run();
                std::process::exit(code);
            }
            Err(e) => {
                eprintln!("Failed to initialize MITOS shell: {}", e);
                std::process::exit(1);
            }
        }
    }
}
