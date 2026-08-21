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



pub fn execute_pipeline(
    commands: &[Vec<String>],
) -> i32 {
    if commands.is_empty() {
        return 0;
    }

    let mut children: Vec<Process> = Vec::new();

    let mut previous_stdout = None;

    for (index, command) in commands.iter().enumerate() {
        if command.is_empty() {
            continue;
        }

        let program = &command[0];
        let args = &command[1..];

        let stdin = previous_stdout
            .take()
            .map(Stdio::from);

        let stdout = if index < commands.len() - 1 {
            Some(Stdio::piped())
        } else {
            None
        };

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

        let executable_string =
            executable.to_string_lossy();

        let mut command_process = Command::new(
            executable_string.as_ref()
        );

        command_process
            .args(args);

        if let Some(stdin) = stdin {
            command_process.stdin(stdin);
        }

        if let Some(stdout) = stdout {
            command_process.stdout(stdout);
        }

        let mut child = match command_process.spawn() {
            Ok(child) => child,

            Err(error) => {
                eprintln!(
                    "MITOS: {}: {}",
                    program,
                    error
                );

                return 126;
            }
        };

        previous_stdout = child.stdout
            .take();

        children.push(Process {
            child,
        });
    }

    let mut final_status = 0;

    for mut process in children {
        match process.wait() {
            Ok(status) => {
                final_status = status;
            }

            Err(error) => {
                eprintln!(
                    "MITOS: pipeline wait failed: {}",
                    error
                );

                final_status = 1;
            }
        }
    }

    final_status
}
