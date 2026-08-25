pub mod alias;

use nix::sys::signal::{killpg, Signal};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use crate::execution::executor::Executor;

use crate::process::job::JobStatus;

pub fn try_execute(args: &[String], executor: &mut Executor) -> Option<i32> {
    if args.is_empty() { return None; }
    
    match args[0].as_str() {
        "cd" => {
            let dir = args.get(1).map(|s| s.as_str()).unwrap_or("~");
            let path = if dir == "~" {
                dirs::home_dir().unwrap_or_default()
            } else {
                std::path::PathBuf::from(dir)
            };
            
            if let Err(e) = std::env::set_current_dir(path) {
                eprintln!("cd: {}", e);
                Some(1)
            } else {
                Some(0)
            }
        }
        "exit" => {
            let code = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            std::process::exit(code);
        }
        "pwd" => {
            println!("{}", std::env::current_dir().unwrap_or_default().display());
            Some(0)
        }
        "jobs" => {
            for job in &executor.jobs.jobs {
                let state = match job.status {
                    JobStatus::Running => "Running",
                    JobStatus::Stopped => "Stopped",
                    _ => "Done",
                };
                println!("[{}]  {}  {}", job.id, state, job.command);
            }
            Some(0)
        }
        "fg" => {
            // Bring most recently stopped job to foreground
            if let Some(job) = executor.jobs.jobs.iter_mut().rev().find(|j| j.status == JobStatus::Stopped) {
                job.status = JobStatus::Running;
                killpg(job.pgid, Signal::SIGCONT).unwrap();
                
                if let Some(tty) = &executor.tty {
                    tty.give_terminal_to(job.pgid);
                }
                
                // Wait for it to finish or stop again
                loop {
                    match waitpid(job.pgid, Some(WaitPidFlag::WUNTRACED)) {
                        Ok(WaitStatus::Exited(_, _)) | Ok(WaitStatus::Signaled(_, _, _)) => {
                            job.status = JobStatus::Done;
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
                Some(0)
            } else {
                eprintln!("fg: no current job");
                Some(1)
            }
        }
        "bg" => {
            // Resume stopped job in background
            if let Some(job) = executor.jobs.jobs.iter_mut().rev().find(|j| j.status == JobStatus::Stopped) {
                job.status = JobStatus::Running;
                killpg(job.pgid, Signal::SIGCONT).unwrap();
                println!("[{}]+ {} &", job.id, job.command);
                Some(0)
            } else {
                eprintln!("bg: no current job");
                Some(1)
            }
        }

        // Add to src/builtins/mod.rs inside try_execute()
"alias" => {
    if args.len() == 1 {
        alias::list();
    } else if let Some(def) = args.get(1) {
        if let Some((name, value)) = def.split_once('=') {
            alias::set(name, value.trim_matches('\'').trim_matches('"'));
        } else {
            eprintln!("alias: expected name=value");
            return Some(1);
        }
    }
    Some(0)
}
"unalias" => {
    if let Some(name) = args.get(1) {
        if !alias::remove(name) {
            eprintln!("unalias: {}: not found", name);
            return Some(1);
        }
    }
    Some(0)
}

        _ => None // Not a builtin, fallback to external execution
    }
}
