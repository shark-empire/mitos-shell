use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::os::unix::process::CommandExt;
use crate::parser::{Command as PCommand, RedirOp};

pub fn find_executable(program: &str) -> Option<PathBuf> {
    let path = Path::new(program);
    if path.is_absolute() || program.contains('/') {
        if path.is_file() { return Some(path.to_path_buf()); }
        return None;
    }
    let path_variable = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path_variable) {
        let candidate = directory.join(program);
        if candidate.is_file() { return Some(candidate); }
    }
    None
}

pub fn execute_with_redirs(program: &str, args: &[String], redirs: &[(RedirOp, String)], background: bool) -> i32 {
    let executable = match find_executable(program) {
        Some(path) => path,
        None => { eprintln!("mitos: {}: command not found", program); return 127; }
    };

    let mut cmd = Command::new(executable);
    cmd.args(args);
    cmd.process_group(0); // Job Control: Put child in its own process group

    let mut stdin_set = false; let mut stdout_set = false; let mut stderr_set = false;
    for (op, file) in redirs {
        match op {
            RedirOp::In => if let Ok(f) = File::open(file) { cmd.stdin(f); stdin_set = true; } else { return 1; },
            RedirOp::Out => if let Ok(f) = File::create(file) { cmd.stdout(f); stdout_set = true; } else { return 1; },
            RedirOp::Append => if let Ok(f) = std::fs::OpenOptions::new().append(true).create(true).open(file) { cmd.stdout(f); stdout_set = true; },
            RedirOp::ErrOut => if let Ok(f) = File::create(file) { cmd.stderr(f); stderr_set = true; },
            RedirOp::ErrApp => if let Ok(f) = std::fs::OpenOptions::new().append(true).create(true).open(file) { cmd.stderr(f); stderr_set = true; },
            RedirOp::AllOut => if let Ok(f) = File::create(file) { cmd.stdout(f.try_clone().unwrap()); cmd.stderr(f); stdout_set = true; stderr_set = true; },
            RedirOp::AllApp => if let Ok(f) = std::fs::OpenOptions::new().append(true).create(true).open(file) { cmd.stdout(f.try_clone().unwrap()); cmd.stderr(f); stdout_set = true; stderr_set = true; }
        }
    }

    if !stdin_set { cmd.stdin(Stdio::inherit()); }
    if !stdout_set { cmd.stdout(Stdio::inherit()); }
    if !stderr_set { cmd.stderr(Stdio::inherit()); }

    match cmd.spawn() {
        Ok(mut child) => {
            if background { return 0; }
            match child.wait() {
                Ok(status) => status.code().unwrap_or(1),
                Err(e) => { eprintln!("mitos: wait error: {}", e); 1 }
            }
        }
        Err(e) => { eprintln!("mitos: {}: {}", program, e); 126 }
    }
}

pub fn execute_pipeline(commands: &[PCommand], background: bool) -> i32 {
    let mut children = Vec::new();
    let mut previous_stdout = None;

    for (i, cmd) in commands.iter().enumerate() {
        let executable = match find_executable(&cmd.args[0]) {
            Some(path) => path,
            None => { eprintln!("mitos: {}: command not found", cmd.args[0]); return 127; }
        };

        let mut c = Command::new(executable);
        c.args(&cmd.args[1..]);
        c.process_group(0); // Job Control

        if let Some(prev) = previous_stdout.take() { c.stdin(prev); } 
        else { c.stdin(Stdio::inherit()); }

        if i == commands.len() - 1 { c.stdout(Stdio::inherit()); c.stderr(Stdio::inherit()); }
        else { c.stdout(Stdio::piped()); c.stderr(Stdio::inherit()); }

        match c.spawn() {
            Ok(mut child) => { previous_stdout = child.stdout.take(); children.push(child); }
            Err(e) => { eprintln!("mitos: {}: {}", cmd.args[0], e); return 126; }
        }
    }

    let mut final_status = 0;
    for mut child in children {
        match child.wait() {
            Ok(status) => final_status = status.code().unwrap_or(1),
            Err(e) => eprintln!("mitos: wait error: {}", e),
        }
    }
    final_status
}
