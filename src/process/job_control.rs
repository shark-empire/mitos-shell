// Reserved: a single-command spawn helper with proper POSIX process-group
// setup (setpgid), intended to back real job control (fg/bg/Ctrl-Z).
// `Executor::fork_pipeline` currently does its own inline forking for
// multi-command pipelines (redirects, heredocs, N-way pipe wiring) and does
// not yet delegate to this; wiring the two together is a larger follow-up.
#![allow(dead_code)]

use crate::error::Result;
use nix::unistd::{close, dup2, execvp, fork, setpgid, ForkResult, Pid};
use std::ffi::CString;
use std::os::unix::io::RawFd;

pub struct JobControl;

impl JobControl {
    /// Spawns a process with proper POSIX process group management.
    pub fn spawn_process(
        args: &[String],
        pgid: Pid,        // Process Group ID (0 means create new group)
        stdin: RawFd,     // File descriptor for standard input
        stdout: RawFd,    // File descriptor for standard output
        foreground: bool, // Whether to give this process control of the TTY
    ) -> Result<Pid> {
        match unsafe { fork()? } {
            ForkResult::Parent { child } => {
                // Set the child's process group. If pgid is 0, the child becomes the leader.
                let pgrp = if pgid == Pid::from_raw(0) {
                    child
                } else {
                    pgid
                };
                let _ = setpgid(child, pgrp);

                if foreground {
                    // In a full implementation, you would call tcsetpgrp(0, pgrp) here
                    // to give the child process control of the terminal.
                }
                Ok(child)
            }
            ForkResult::Child => {
                let pgrp = if pgid == Pid::from_raw(0) {
                    nix::unistd::getpid()
                } else {
                    pgid
                };
                let _ = setpgid(nix::unistd::getpid(), pgrp);

                // Wire up standard streams
                if stdin != 0 {
                    dup2(stdin, 0)?;
                    close(stdin)?;
                }
                if stdout != 1 {
                    dup2(stdout, 1)?;
                    close(stdout)?;
                }

                // Convert Rust Strings to CStrings for execvp
                let c_args: Vec<CString> = args
                    .iter()
                    .map(|s| CString::new(s.as_str()).unwrap())
                    .collect();

                // Replace child process image with the target executable
                execvp(&c_args[0], &c_args)?;
                unreachable!();
            }
        }
    }
}
