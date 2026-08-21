mod builtins;
mod shell;

use shell::Shell;

fn main() {
    let mut shell = Shell::new();

    let exit_code = shell.run();

    std::process::exit(exit_code);
}
