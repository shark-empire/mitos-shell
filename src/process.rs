use std::env;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

pub struct Process {
    child: Child,
}

impl Process {
    pub fn spawn(
        program: &str,
        args: &[String],
        stdin: Option<Stdio>,
        stdout: Option<Stdio>,
        stderr: Option<Stdio>,
    ) -> Result<Self, String> {
        let mut command = Command::new(program);

        command.args(args);

        if let Some(stdin) = stdin {
            command.stdin(stdin);
        }

        if let Some(stdout) = stdout {
            command.stdout(stdout);
        }

        if let Some(stderr) = stderr {
            command.stderr(stderr);
        }

        let child = command
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

pub fn find_executable(program: &str) -> Option<PathBuf> {
    let path = Path::new(program);

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

pub fn execute(program: &str, args: &[String]) -> i32 {
    execute_with_io(program, args, None, None, None)
}

pub fn execute_with_io(
    program: &str,
    args: &[String],
    stdin: Option<Stdio>,
    stdout: Option<Stdio>,
    stderr: Option<Stdio>,
) -> i32 {
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
        stdin,
        stdout,
        stderr,
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

pub fn spawn_pipeline_process(
    program: &str,
    args: &[String],
    stdin: Option<Stdio>,
    stdout: Option<Stdio>,
) -> Result<Process, String> {
    let executable = find_executable(program)
        .ok_or_else(|| {
            format!(
                "{}: command not found",
                program
            )
        })?;

    let executable_string = executable.to_string_lossy();

    Process::spawn(
        &executable_string,
        args,
        stdin,
        stdout,
        None,
    )
}

pub fn open_input(path: &str) -> Result<Stdio, String> {
    let file = File::open(path)
        .map_err(|error| {
            format!("{}: {}", path, error)
        })?;

    Ok(Stdio::from(file))
}

pub fn open_output(
    path: &str,
    append: bool,
) -> Result<Stdio, String> {
    let file = if append {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|error| {
                format!("{}: {}", path, error)
            })?
    } else {
        File::create(path)
            .map_err(|error| {
                format!("{}: {}", path, error)
            })?
    };

    Ok(Stdio::from(file))
}

pub fn pipe() -> Result<(Stdio, Stdio), String> {
    Err(
        "pipe creation requires direct process-pipe support"
            .to_string()
    )
}
