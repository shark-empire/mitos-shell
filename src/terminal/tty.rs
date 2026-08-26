use nix::libc;
use nix::unistd::Pid;
use std::os::unix::io::RawFd;

pub struct TtyManager {
    tty_fd: RawFd,
    shell_pgid: Pid,
}

impl TtyManager {
    /// Initializes the shell as the foreground process group leader
    pub fn init() -> Option<Self> {
        // Open the controlling terminal
        let tty_fd = nix::fcntl::open(
            "/dev/tty",
            nix::fcntl::OFlag::O_RDWR,
            nix::sys::stat::Mode::empty(),
        )
        .ok()?;

        let pid = nix::unistd::getpid();

        // Put the shell in its own process group
        let _ = nix::unistd::setpgid(pid, pid);

        // Take control of the terminal
        unsafe {
            let _ = libc::tcsetpgrp(tty_fd, pid.as_raw());
        }

        Some(Self {
            tty_fd,
            shell_pgid: pid,
        })
    }

    /// Hands terminal control to a child process group
    pub fn give_terminal_to(&self, pgid: Pid) {
        unsafe {
            libc::tcsetpgrp(self.tty_fd, pgid.as_raw());
        }
    }

    /// Reclaims terminal control for the shell
    pub fn take_terminal_back(&self) {
        unsafe {
            libc::tcsetpgrp(self.tty_fd, self.shell_pgid.as_raw());
        }
    }
}
