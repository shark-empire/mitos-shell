pub mod alias;
pub mod eval;
pub mod read;
pub mod set;
pub mod test;
pub mod trap;

use crate::execution::executor::Executor;
use crate::execution::outcome::ExecOutcome;
use crate::process::job::JobStatus;
use crate::util::set_var;
use nix::sys::signal::{killpg, Signal};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use std::io::{self, Write};

pub fn try_execute(executor: &mut Executor, args: &[String]) -> Option<ExecOutcome> {
    let name = args.first()?;
    match name.as_str() {
        // Control flow
        "break" => Some(ExecOutcome::Break),
        "continue" => Some(ExecOutcome::Continue),
        "return" => {
            let code = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            Some(ExecOutcome::Return(code))
        }
        "exit" => {
            let code = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            Some(ExecOutcome::Exit(code))
        }
        "set" => {
            // Note: We need mutable access to options, so we handle 'set'
            // directly in the Executor, or pass a mutable reference.
            // For simplicity, let's return a special outcome or handle it in Executor.
            None
        }

        // Status builtins
        "true" | ":" => Some(ExecOutcome::Status(0)),
        "false" => Some(ExecOutcome::Status(1)),

        // Input/output.
        "echo" => Some(ExecOutcome::Status(builtin_echo(args))),
        "read" => Some(ExecOutcome::Status(read::execute(args))),
        // Directory / env
        "cd" => Some(ExecOutcome::Status(builtin_cd(args))),
        "pwd" => match std::env::current_dir() {
            Ok(path) => {
                println!("{}", path.display());
                Some(ExecOutcome::Status(0))
            }

            Err(error) => {
                eprintln!("pwd: {}", error);
                Some(ExecOutcome::Status(1))
            }
        },
        "export" => Some(ExecOutcome::Status(builtin_export(args))),
        "test" | "[" => Some(ExecOutcome::Status(test::execute_test(args))),
        "jobs" => {
            for job in &executor.jobs.jobs {
                let state = match job.status {
                    JobStatus::Running => "Running",
                    JobStatus::Stopped => "Stopped",
                    _ => "Done",
                };
                println!("[{}]  {}  {}", job.id, state, job.command);
            }
            // Finished jobs are shown once as "Done", then pruned so they
            // don't linger in subsequent `jobs` listings.
            executor.jobs.cleanup_finished();
            Some(ExecOutcome::Status(0))
        }
        "fg" => {
            // Bring most recently stopped job to foreground
            if let Some(job) = executor
                .jobs
                .jobs
                .iter_mut()
                .rev()
                .find(|j| j.status == JobStatus::Stopped)
            {
                job.status = JobStatus::Running;
                killpg(job.pgid, Signal::SIGCONT).unwrap();

                if let Some(tty) = &executor.tty {
                    tty.give_terminal_to(job.pgid);
                }

                // Wait for it to finish or stop again
                loop {
                    match waitpid(job.pgid, Some(WaitPidFlag::WUNTRACED)) {
                        Ok(WaitStatus::Exited(_, code)) => {
                            job.status = JobStatus::Exited(code);
                            break;
                        }
                        Ok(WaitStatus::Signaled(_, sig, _)) => {
                            job.status = JobStatus::Signaled(sig);
                            break;
                        }
                        Ok(WaitStatus::Stopped(_, _)) => {
                            job.status = JobStatus::Stopped;
                            println!("\n[{}] Stopped          {}", job.id, job.command);
                            break;
                        }
                        Err(err) => {
                            eprintln!("waitpid error: {}", err);
                            break;
                        }
                        _ => {}
                    }
                }

                if let Some(tty) = &executor.tty {
                    tty.take_terminal_back();
                }
                Some(ExecOutcome::Status(0))
            } else {
                eprintln!("fg: no current job");
                Some(ExecOutcome::Status(1))
            }
        }
        "bg" => {
            // Resume stopped job in background
            if let Some(job) = executor
                .jobs
                .jobs
                .iter_mut()
                .rev()
                .find(|j| j.status == JobStatus::Stopped)
            {
                job.status = JobStatus::Running;
                killpg(job.pgid, Signal::SIGCONT).unwrap();
                println!("[{}]+ {} &", job.id, job.command);
                Some(ExecOutcome::Status(0))
            } else {
                eprintln!("bg: no current job");
                Some(ExecOutcome::Status(1))
            }
        }

        // Aliases.
        "alias" => {
            if args.len() == 1 {
                alias::list();
            } else if let Some(definition) = args.get(1) {
                if let Some((name, value)) = definition.split_once('=') {
                    alias::set(name, value.trim_matches('\'').trim_matches('"'));
                } else {
                    eprintln!("alias: expected name=value");
                    return Some(ExecOutcome::Status(1));
                }
            }

            Some(ExecOutcome::Status(0))
        }
        "unalias" => {
            let removed = args.get(1).map(|name| alias::remove(name)).unwrap_or(false);

            if removed {
                Some(ExecOutcome::Status(0))
            } else {
                eprintln!("unalias: not found");
                Some(ExecOutcome::Status(1))
            }
        }

        "trap" => Some(ExecOutcome::Status(trap::execute(
            args,
            &mut executor.traps,
        ))),

        _ => None, // Not a builtin, fallback to external execution
    }
}

fn builtin_echo(args: &[String]) -> i32 {
    let mut newline = true;
    let mut start = 1;

    if args.get(1).map(|value| value.as_str()) == Some("-n") {
        newline = false;
        start = 2;
    }

    let text = args[start..].join(" ");

    if newline {
        println!("{}", text);
    } else {
        print!("{}", text);

        if let Err(error) = io::stdout().flush() {
            eprintln!("echo: {}", error);
            return 1;
        }
    }

    0
}

fn builtin_cd(args: &[String]) -> i32 {
    let dir = args.get(1).map(|value| value.as_str()).unwrap_or("~");

    let path = if dir == "~" {
        dirs::home_dir().unwrap_or_default()
    } else {
        std::path::PathBuf::from(dir)
    };

    match std::env::set_current_dir(path) {
        Ok(_) => 0,

        Err(error) => {
            eprintln!("cd: {}", error);
            1
        }
    }
}

fn builtin_export(args: &[String]) -> i32 {
    if args.len() == 1 {
        for (key, value) in std::env::vars() {
            println!("{}={}", key, value);
        }

        return 0;
    }

    for assignment in args.iter().skip(1) {
        if let Some((key, value)) = assignment.split_once('=') {
            if key.is_empty() {
                eprintln!("export: empty variable name");
                return 1;
            }

            set_var(key, value);
        } else {
            eprintln!("export: invalid assignment: {}", assignment);
            return 1;
        }
    }

    0
}
