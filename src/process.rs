use std::process::{Child, Command};

pub struct Process {
    child: Child,
}

impl Process {
    pub fn spawn(program: &str, args: &[String]) -> Result<Self, String> {
        let child = Command::new(program)
            .args(args)
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

pub fn execute(program: &str, args: &[String]) -> i32 {
    let mut process = match Process::spawn(program, args) {
        Ok(process) => process,

        Err(error) => {
            eprintln!(
                "MITOS: {}: {}",
                program,
                error
            );

            return 127;
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
