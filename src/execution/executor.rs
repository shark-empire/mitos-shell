use crate::builtins;
use crate::error::Result;
use crate::expansion::expander::Expander;
use crate::parser::ast::{Command, Redirect};
use crate::process::job::{JobStatus, JobTable};
use crate::terminal::tty::TtyManager;

use nix::fcntl::{open, OFlag};
use nix::sys::stat::Mode;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{close, dup2, execvp, fork, pipe, setpgid, ForkResult, Pid};

use std::ffi::CString;
use std::os::unix::io::RawFd;

pub struct Executor {
    tty: Option<TtyManager>,
    jobs: JobTable,
    last_status: i32,
}

impl Executor {
    pub fn new() -> Self {
        Self {
            tty: TtyManager::init(),
            jobs: JobTable::new(),
            last_status: 0,
        }
    }

    fn execute_pipeline(&mut self, commands: Vec<Command>) -> Result<i32> {
        if commands.is_empty() {
            return Ok(0);
        }

        // ------------------------------------------------------------
        // 1. Expand arguments
        // ------------------------------------------------------------

        let expander = Expander::new(self.last_status);

        let commands: Vec<Command> = commands
            .into_iter()
            .map(|mut cmd| {
                cmd.args = expander.expand_args(cmd.args);
                cmd
            })
            .collect();

        if commands.is_empty() {
            return Ok(0);
        }

        // Capture these BEFORE commands is consumed.
        let is_background = commands
            .last()
            .map(|cmd| cmd.background)
            .unwrap_or(false);

        let command_string = commands
            .iter()
            .map(|cmd| cmd.args.join(" "))
            .collect::<Vec<_>>()
            .join(" | ");

        let num_cmds = commands.len();

        // ------------------------------------------------------------
        // 2. Single-command builtins
        // ------------------------------------------------------------

        if num_cmds == 1 && !is_background {
            if let Some(status) = builtins::try_execute(&commands[0].args) {
                self.last_status = status;
                return Ok(status);
            }
        }

        // ------------------------------------------------------------
        // 3. Create pipeline pipes
        // ------------------------------------------------------------

        let mut pipes: Vec<(RawFd, RawFd)> = Vec::new();

        for _ in 0..num_cmds.saturating_sub(1) {
            pipes.push(pipe()?);
        }

        // ------------------------------------------------------------
        // 4. Fork pipeline
        // ------------------------------------------------------------

        let mut children: Vec<Pid> = Vec::with_capacity(num_cmds);

        let mut pgid: Option<Pid> = None;

        for (i, cmd) in commands.iter().enumerate() {
            match unsafe { fork()? } {
                ForkResult::Parent { child } => {
                    // First child becomes process-group leader.
                    if pgid.is_none() {
                        pgid = Some(child);
                    }

                    let group = pgid.unwrap();

                    // Put every pipeline process into the same process
                    // group.
                    let _ = setpgid(child, group);

                    children.push(child);
                }

                ForkResult::Child => {
                    // ------------------------------------------------
                    // Child process
                    // ------------------------------------------------

                    // Determine process group.

                    let child_pgid = pgid.unwrap_or_else(|| Pid::this());

                    let _ = setpgid(Pid::this(), child_pgid);

                    // ------------------------------------------------
                    // Pipe stdin
                    // ------------------------------------------------

                    if i > 0 {
                        let (read_end, _) = pipes[i - 1];

                        dup2(read_end, 0)?;
                    }

                    // ------------------------------------------------
                    // Pipe stdout
                    // ------------------------------------------------

                    if i < num_cmds - 1 {
                        let (_, write_end) = pipes[i];

                        dup2(write_end, 1)?;
                    }

                    // ------------------------------------------------
                    // Close inherited pipe descriptors
                    // ------------------------------------------------

                    for &(read_end, write_end) in &pipes {
                        let _ = close(read_end);
                        let _ = close(write_end);
                    }

                    // ------------------------------------------------
                    // Redirections
                    // ------------------------------------------------

                    for redir in &cmd.redirects {
                        match redir {
                            Redirect::Input(path) => {
                                let fd = open(
                                    path,
                                    OFlag::O_RDONLY,
                                    Mode::empty(),
                                )?;

                                dup2(fd, 0)?;

                                let _ = close(fd);
                            }

                            Redirect::Output(path) => {
                                let fd = open(
                                    path,
                                    OFlag::O_WRONLY
                                        | OFlag::O_CREAT
                                        | OFlag::O_TRUNC,
                                    Mode::from_bits_truncate(0o644),
                                )?;

                                dup2(fd, 1)?;

                                let _ = close(fd);
                            }

                            Redirect::Append(path) => {
                                let fd = open(
                                    path,
                                    OFlag::O_WRONLY
                                        | OFlag::O_CREAT
                                        | OFlag::O_APPEND,
                                    Mode::from_bits_truncate(0o644),
                                )?;

                                dup2(fd, 1)?;

                                let _ = close(fd);
                            }
                        }
                    }

                    // ------------------------------------------------
                    // Execute program
                    // ------------------------------------------------

                    if cmd.args.is_empty() {
                        std::process::exit(0);
                    }

                    let c_args: Vec<CString> = cmd
                        .args
                        .iter()
                        .map(|arg| {
                            CString::new(arg.as_str())
                                .expect("argument contains NUL byte")
                        })
                        .collect();

                    let result = execvp(&c_args[0], &c_args);

                    match result {
                        Ok(_) => unreachable!(),

                        Err(_) => {
                            eprintln!(
                                "mitos: command not found: {}",
                                cmd.args[0]
                            );

                            std::process::exit(127);
                        }
                    }
                }
            }
        }

        let pgid = match pgid {
            Some(pgid) => pgid,
            None => return Ok(1),
        };

        // ------------------------------------------------------------
        // 5. Parent closes pipeline descriptors
        // ------------------------------------------------------------

        for (read_end, write_end) in pipes {
            let _ = close(read_end);
            let _ = close(write_end);
        }

        // ------------------------------------------------------------
        // 6. Register job
        // ------------------------------------------------------------

        let job_id = self.jobs.add(pgid, command_string.clone());

        // ------------------------------------------------------------
        // 7. Background job
        // ------------------------------------------------------------

        if is_background {
            println!(
                "[{}] {}",
                job_id,
                pgid
            );

            self.jobs.update_status(
                pgid,
                JobStatus::Running,
            );

            // IMPORTANT:
            //
            // Do NOT wait here.
            //
            // The shell must immediately return to the prompt.

            return Ok(0);
        }

        // ------------------------------------------------------------
        // 8. Foreground job
        // ------------------------------------------------------------

        if let Some(tty) = &self.tty {
            tty.give_terminal_to(pgid);
        }

        self.jobs.update_status(
            pgid,
            JobStatus::Running,
        );

        // ------------------------------------------------------------
        // 9. Wait for foreground process group
        // ------------------------------------------------------------

        let mut final_status = 0;
        let mut stopped = false;

        loop {
            match waitpid(
                Pid::from_raw(-pgid.as_raw()),
                Some(
                    WaitPidFlag::WUNTRACED
                        | WaitPidFlag::WCONTINUED,
                ),
            )? {
                WaitStatus::Exited(_, status) => {
                    final_status = status;

                    // Continue until all children have been reaped.
                    if children.is_empty() {
                        break;
                    }

                    children.retain(|pid| *pid != pgid);
                }

                WaitStatus::Signaled(_, sig, _) => {
                    final_status = 128 + sig as i32;

                    eprintln!("\n{}", sig);
                }

                WaitStatus::Stopped(_, _) => {
                    stopped = true;

                    self.jobs.update_status(
                        pgid,
                        JobStatus::Stopped,
                    );

                    break;
                }

                WaitStatus::Continued(_) => {
                    self.jobs.update_status(
                        pgid,
                        JobStatus::Running,
                    );
                }

                WaitStatus::StillAlive => {}

                _ => {}
            }
        }

        // ------------------------------------------------------------
        // 10. Take terminal back
        // ------------------------------------------------------------

        if let Some(tty) = &self.tty {
            tty.take_terminal_back();
        }

        // ------------------------------------------------------------
        // 11. Handle stopped job
        // ------------------------------------------------------------

        if stopped {
            println!(
                "\n[{}]+  Stopped           {}",
                job_id,
                command_string
            );

            self.last_status = 128 + 20;

            return Ok(self.last_status);
        }

        // ------------------------------------------------------------
        // 12. Job completed
        // ------------------------------------------------------------

        self.jobs.update_status(
            pgid,
            JobStatus::Exited(final_status),
        );

        self.last_status = final_status;

        Ok(final_status)
    }
}
