use crate::error::{Result, ShellError};
use crate::parser::ast::{Node, Command, Redirect};
use crate::builtins;
use nix::unistd::{pipe, dup2, close, ForkResult, fork, Pid};
use nix::fcntl::{open, OFlag};
use nix::sys::stat::Mode;
use nix::sys::wait::{waitpid, WaitStatus};
use std::os::unix::io::RawFd;
use std::ffi::CString;

pub struct Executor;

impl Executor {
    pub fn new() -> Self { Self }

    pub fn execute(&self, node: Node) -> Result<i32> {
        match node {
            Node::Pipeline(commands) => self.execute_pipeline(commands),
        }
    }

    fn execute_pipeline(&self, commands: Vec<Command>) -> Result<i32> {
        let num_cmds = commands.len();
        let mut last_status = 0;
        
        // 1. Create pipes for inter-process communication
        let mut pipes: Vec<(RawFd, RawFd)> = Vec::new();
        for _ in 0..num_cmds.saturating_sub(1) {
            pipes.push(pipe()?);
        }

        let mut children: Vec<Pid> = Vec::new();

        // 2. Fork and Exec each command in the pipeline
        for (i, cmd) in commands.into_iter().enumerate() {
            // Intercept builtins (must run in parent process if it's a single command)
            if num_cmds == 1 {
                if let Some(status) = builtins::try_execute(&cmd.args) {
                    return Ok(status);
                }
            }

            match unsafe { fork()? } {
                ForkResult::Parent { child } => {
                    children.push(child);
                }
                ForkResult::Child => {
                    // Wire up Pipes
                    if i > 0 {
                        let (read_end, _) = pipes[i - 1];
                        dup2(read_end, 0)?; // stdin
                    }
                    if i < num_cmds - 1 {
                        let (_, write_end) = pipes[i];
                        dup2(write_end, 1)?; // stdout
                    }

                    // Close all pipe ends in child to prevent deadlocks
                    for (r, w) in &pipes {
                        let _ = close(*r);
                        let _ = close(*w);
                    }

                    // Wire up Redirections (<, >, >>)
                    for redir in &cmd.redirects {
                        match redir {
                            Redirect::Input(path) => {
                                let fd = open(path, OFlag::O_RDONLY, Mode::empty())?;
                                dup2(fd, 0)?;
                                let _ = close(fd);
                            }
                            Redirect::Output(path) => {
                                let fd = open(path, OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_TRUNC, Mode::from_bits(0o644).unwrap())?;
                                dup2(fd, 1)?;
                                let _ = close(fd);
                            }
                            Redirect::Append(path) => {
                                let fd = open(path, OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_APPEND, Mode::from_bits(0o644).unwrap())?;
                                dup2(fd, 1)?;
                                let _ = close(fd);
                            }
                        }
                    }

                    // Execute the binary
                    let c_args: Vec<CString> = cmd.args.iter()
                        .map(|s| CString::new(s.as_str()).unwrap())
                        .collect();
                    
                    if c_args.is_empty() { std::process::exit(0); }

                    // execvp replaces the child process image. If it fails, the binary wasn't found.
                    let _ = nix::unistd::execvp(&c_args[0], &c_args);
                    eprintln!("mitos: command not found: {}", cmd.args[0]);
                    std::process::exit(127);
                }
            }
        }

        // 3. Parent closes all pipe ends
        for (r, w) in pipes {
            let _ = close(r);
            let _ = close(w);
        }

        // 4. Wait for all children to finish (prevents zombie processes)
        for child in children {
            match waitpid(child, None)? {
                WaitStatus::Exited(_, status) => last_status = status,
                WaitStatus::Signaled(_, sig, _) => last_status = 128 + sig as i32,
                _ => {}
            }
        }

        Ok(last_status)
    }
}
