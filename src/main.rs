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
use lexer::Lexer;
use nix::sys::signal::{signal, SigHandler, Signal};
use parser::Parser;
use shell::session::Session;
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};

pub static INTERRUPTED: AtomicBool = AtomicBool::new(false);
pub static TERM_RECEIVED: AtomicBool = AtomicBool::new(false);
pub static HUP_RECEIVED: AtomicBool = AtomicBool::new(false);

extern "C" fn handle_int(_: nix::libc::c_int) {
    INTERRUPTED.store(true, Ordering::SeqCst);
}

extern "C" fn handle_term(_: nix::libc::c_int) {
    TERM_RECEIVED.store(true, Ordering::SeqCst);
}

extern "C" fn handle_hup(_: nix::libc::c_int) {
    HUP_RECEIVED.store(true, Ordering::SeqCst);
}

fn install_signal_handlers() {
    unsafe {
        // Caught (rather than the default terminate-immediately action)
        // so `exec_node`'s `check_pending_signal` gets a chance to run a
        // registered `trap` before the shell decides how to respond.
        let _ = signal(Signal::SIGINT, SigHandler::Handler(handle_int));
        let _ = signal(Signal::SIGTERM, SigHandler::Handler(handle_term));
        let _ = signal(Signal::SIGHUP, SigHandler::Handler(handle_hup));

        // Ctrl-\ shouldn't dump core or kill the shell.
        let _ = signal(Signal::SIGQUIT, SigHandler::SigIgn);

        // Job control: the shell must ignore these itself so its own
        // terminal operations (tcsetpgrp) and sometimes being in a
        // background process group don't stop the shell process itself —
        // only the job actually in the foreground process group receives
        // these from the kernel's TTY line discipline.
        let _ = signal(Signal::SIGTSTP, SigHandler::SigIgn);
        let _ = signal(Signal::SIGTTIN, SigHandler::SigIgn);
        let _ = signal(Signal::SIGTTOU, SigHandler::SigIgn);
    }
}

fn main() {
    install_signal_handlers();

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
                            executor.run_exit_trap();
                            std::process::exit(code);
                        }
                        executor.run_exit_trap();
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
