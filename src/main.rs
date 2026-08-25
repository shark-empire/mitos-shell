mod builtins;
mod parser;
mod process;
mod shell;

use shell::Shell;

fn main() {
    // Industry Standard: Ignore signals that should only affect foreground jobs
    unsafe {
        let _ = nix::sys::signal::signal(nix::sys::signal::Signal::SIGINT, nix::sys::signal::SigHandler::SigIgn);
        let _ = nix::sys::signal::signal(nix::sys::signal::Signal::SIGQUIT, nix::sys::signal::SigHandler::SigIgn);
        let _ = nix::sys::signal::signal(nix::sys::signal::Signal::SIGTSTP, nix::sys::signal::SigHandler::SigIgn);
        let _ = nix::sys::signal::signal(nix::sys::signal::Signal::SIGTTIN, nix::sys::signal::SigHandler::SigIgn);
        let _ = nix::sys::signal::signal(nix::sys::signal::Signal::SIGTTOU, nix::sys::signal::SigHandler::SigIgn);
    }

    let mut shell = Shell::new();
    let exit_code = shell.run();
    std::process::exit(exit_code);
}
