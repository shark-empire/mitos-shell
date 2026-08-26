// src/config/options.rs
#[derive(Debug, Clone, Default)]
pub struct ShellOptions {
    pub errexit: bool,   // set -e : Exit immediately if a command exits with non-zero status
    pub nounset: bool,   // set -u : Treat unset variables as an error
    pub xtrace: bool,    // set -x : Print commands and their arguments as they are executed
}
