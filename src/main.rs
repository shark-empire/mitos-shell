mod error;
mod lexer;
mod parser;
mod execution;
mod builtins;
mod shell;
mod process;
mod terminal;


use shell::session::Session;
use nix::sys::signal::{signal, SigHandler, Signal};

fn main() {
    // Crucial: Ignore SIGINT (Ctrl+C) and SIGQUIT in the parent shell.
    // This ensures that when the user presses Ctrl+C, it only kills the 
    // foreground child process (like `ping`), not the MITOS shell itself.
    unsafe {
        let _ = signal(Signal::SIGINT, SigHandler::SigIgn);
        let _ = signal(Signal::SIGQUIT, SigHandler::SigIgn);
    }

    match Session::init() {
        Ok(mut session) => session.run(),
        Err(e) => eprintln!("Failed to initialize MITOS shell: {}", e),
    }
}
