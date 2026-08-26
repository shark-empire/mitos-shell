// src/builtins/set.rs
use crate::config::options::ShellOptions;

pub fn execute(args: &[String], options: &mut ShellOptions) -> i32 {
    for arg in args.iter().skip(1) {
        match arg.as_str() {
            "-e" => options.errexit = true,
            "+e" => options.errexit = false,
            "-u" => options.nounset = true,
            "+u" => options.nounset = false,
            "-x" => options.xtrace = true,
            "+x" => options.xtrace = false,
            _ => {
                eprintln!("set: invalid option: {}", arg);
                return 2;
            }
        }
    }
    0
}
