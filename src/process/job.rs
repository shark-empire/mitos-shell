use nix::sys::signal::Signal;
use nix::unistd::Pid;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JobStatus {
    Running,
    Stopped,
    Exited(i32),
    Signaled(Signal),
}

pub struct Job {
    pub id: usize,
    pub pgid: Pid,
    pub command: String,
    pub status: JobStatus,
}

pub struct JobTable {
    pub jobs: Vec<Job>,
    next_id: usize,
}

impl JobTable {
    pub fn new() -> Self {
        Self {
            jobs: Vec::new(),
            next_id: 1,
        }
    }

    pub fn add(&mut self, pgid: Pid, command: String) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.jobs.push(Job {
            id,
            pgid,
            command,
            status: JobStatus::Running,
        });
        id
    }

    pub fn update_status(&mut self, pgid: Pid, status: JobStatus) {
        if let Some(job) = self.jobs.iter_mut().find(|j| j.pgid == pgid) {
            job.status = status;
        }
    }

    pub fn cleanup_finished(&mut self) {
        self.jobs
            .retain(|j| matches!(j.status, JobStatus::Running | JobStatus::Stopped));
    }
}
