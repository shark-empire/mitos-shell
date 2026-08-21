use std::env;
use std::path::PathBuf;

use std::process::{Child, Command};

pub struct Process {
    child: Child,
}

impl Process {
    pub fn spawn(program: &str, args: &[String]) -> Result<Self, String> {
        let child = Command::new(program)
            .args(args)
            .spawn()
            .map_err(|error| error.to_string())?;

        Ok(Self { child })
    }

    pub fn wait(&mut self) -> Result<i32, String> {
        let status = self
            .child
            .wait()
            .map_err(|error| error.to_string())?;

        Ok(status.code().unwrap_or(1))
    }
}

pub fn execute(program: &str, args: &[String]) -> i32 {
    let executable = match find_executable(program) {
        Some(path) => path,

        None => {
            eprintln!(
                "MITOS: {}: command not found",
                program
            );

            return 127;
        }
    };

    let executable_string = executable.to_string_lossy();

    let mut process = match Process::spawn(
        &executable_string,
        args,
    ) {
        Ok(process) => process,

        Err(error) => {
            eprintln!(
                "MITOS: {}: {}",
                program,
                error
            );

            return 126;
        }
    };

    match process.wait() {
        Ok(status) => status,

        Err(error) => {
            eprintln!(
                "MITOS: failed waiting for {}: {}",
                program,
                error
            );

            1
        }
    }
}

pub fn find_executable(program: &str) -> Option<PathBuf> {
    let path = std::path::Path::new(program);

    if path.is_absolute() || program.contains('/') {
        if path.is_file() {
            return Some(path.to_path_buf());
        }

        return None;
    }

    let path_variable = env::var_os("PATH")?;

    for directory in env::split_paths(&path_variable) {
        let candidate = directory.join(program);

        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}
