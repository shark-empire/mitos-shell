use std::env;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;
use crate::builtins;

pub struct Shell {
    previous_dir: Option<PathBuf>,
    last_status: i32,
}

impl Shell {
    pub fn new() -> Self {
        Self {
            previous_dir: None,
            last_status: 0,
        }
    }

    pub fn run(&mut self) -> i32 {
        loop {
            self.print_prompt();

            let mut input = String::new();

            match io::stdin().read_line(&mut input) {
                Ok(0) => {
                    println!();
                    return self.last_status;
                }

                Ok(_) => {}

                Err(error) => {
                    eprintln!("mitos-shell: input error: {}", error);
                    return 1;
                }
            }

            let input = input.trim();

            if input.is_empty() {
                continue;
            }

            let expanded = self.expand_variables(input);

            let args = match parse_command(&expanded) {
                Ok(args) => args,

                Err(error) => {
                    eprintln!("mitos-shell: {}", error);
                    self.last_status = 2;
                    continue;
                }
            };

            if args.is_empty() {
                continue;
            }
let command = &args[0];
let arguments = &args[1..];

match builtins::execute(
    command,
    arguments,
    self.last_status,
) {
    builtins::BuiltinResult::Continue(status) => {
        self.last_status = status;
    }

    builtins::BuiltinResult::Exit(status) => {
        return status;
    }

    builtins::BuiltinResult::NotBuiltin => {
        self.last_status = self.execute_external(&args);
       }
      }
    }
}

    fn print_prompt(&self) {
        let cwd = env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"));

        print!("MITOS {} > ", cwd.display());

        let _ = io::stdout().flush();
    }

    fn cd(&mut self, args: &[String]) -> i32 {
        let current_dir = match env::current_dir() {
            Ok(path) => path,

            Err(error) => {
                eprintln!("cd: {}", error);
                return 1;
            }
        };

        let target = if args.is_empty() {
            match env::var("HOME") {
                Ok(home) => PathBuf::from(home),
                Err(_) => PathBuf::from("/"),
            }
        } else if args[0] == "-" {
            match &self.previous_dir {
                Some(path) => path.clone(),

                None => {
                    eprintln!("cd: OLDPWD not set");
                    return 1;
                }
            }
        } else {
            expand_home(&args[0])
        };

        if let Err(error) = env::set_current_dir(&target) {
            eprintln!(
                "cd: {}: {}",
                target.display(),
                error
            );

            return 1;
        }

        self.previous_dir = Some(current_dir);

        0
    }

    fn pwd(&self) -> i32 {
        match env::current_dir() {
            Ok(path) => {
                println!("{}", path.display());
                0
            }

            Err(error) => {
                eprintln!("pwd: {}", error);
                1
            }
        }
    }

    fn echo(&self, args: &[String]) -> i32 {
        println!("{}", args.join(" "));
        0
    }

    fn export(&self, args: &[String]) -> i32 {
        if args.is_empty() {
            for (key, value) in env::vars() {
                println!("{}={}", key, value);
            }

            return 0;
        }

        for assignment in args {
            let Some((key, value)) = assignment.split_once('=') else {
                eprintln!(
                    "export: invalid assignment: {}",
                    assignment
                );

                return 2;
            };

            if key.is_empty() {
                eprintln!("export: empty variable name");
                return 2;
            }

            unsafe {
                env::set_var(key, value);
            }
        }

        0
    }

    fn clear(&self) -> i32 {
        print!("\x1B[2J\x1B[H");

        let _ = io::stdout().flush();

        0
    }

    fn help(&self) -> i32 {
        println!("MITOS Shell");
        println!();
        println!("Built-in commands:");
        println!("  cd [DIR]       Change directory");
        println!("  cd -           Previous directory");
        println!("  pwd            Print current directory");
        println!("  echo [TEXT]    Print text");
        println!("  export VAR=V   Set environment variable");
        println!("  clear          Clear terminal");
        println!("  help           Show help");
        println!("  exit [STATUS]  Exit shell");
        println!();

        0
    }

    fn exit(&self, args: &[String]) -> i32 {
        if args.is_empty() {
            return self.last_status;
        }

        match args[0].parse::<i32>() {
            Ok(status) => status,

            Err(_) => {
                eprintln!("exit: numeric argument required");
                2
            }
        }
    }

    fn execute_external(&self, args: &[String]) -> i32 {
        let command = &args[0];

        let mut child = match Command::new(command)
            .args(&args[1..])
            .spawn()
        {
            Ok(child) => child,

            Err(error) => {
                eprintln!(
                    "MITOS: {}: {}",
                    command,
                    error
                );

                return 127;
            }
        };

        match child.wait() {
            Ok(status) => status.code().unwrap_or(1),

            Err(error) => {
                eprintln!(
                    "MITOS: failed waiting for {}: {}",
                    command,
                    error
                );

                1
            }
        }
    }

    fn expand_variables(&self, input: &str) -> String {
        let mut result = String::new();
        let mut chars = input.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch != '$' {
                result.push(ch);
                continue;
            }

            if chars.peek() == Some(&'?') {
                chars.next();

                result.push_str(
                    &self.last_status.to_string()
                );

                continue;
            }

            let mut name = String::new();

            while let Some(&next) = chars.peek() {
                if next.is_alphanumeric() || next == '_' {
                    name.push(next);
                    chars.next();
                } else {
                    break;
                }
            }

            if name.is_empty() {
                result.push('$');
                continue;
            }

            if let Ok(value) = env::var(&name) {
                result.push_str(&value);
            }
        }

        result
    }
}

fn expand_home(input: &str) -> PathBuf {
    if input == "~" {
        return env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/"));
    }

    if let Some(rest) = input.strip_prefix("~/") {
        if let Ok(home) = env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }

    PathBuf::from(input)
}

fn parse_command(input: &str) -> Result<Vec<String>, &'static str> {
    let mut args = Vec::new();
    let mut current = String::new();

    let mut chars = input.chars().peekable();

    let mut single_quotes = false;
    let mut double_quotes = false;

    while let Some(ch) = chars.next() {
        match ch {
            '\'' if !double_quotes => {
                single_quotes = !single_quotes;
            }

            '"' if !single_quotes => {
                double_quotes = !double_quotes;
            }

            '\\' if !single_quotes => {
                match chars.next() {
                    Some(next) => current.push(next),
                    None => return Err("unfinished escape"),
                }
            }

            ch if ch.is_whitespace()
                && !single_quotes
                && !double_quotes =>
            {
                if !current.is_empty() {
                    args.push(
                        std::mem::take(&mut current)
                    );
                }
            }

            ch => {
                current.push(ch);
            }
        }
    }

    if single_quotes || double_quotes {
        return Err("unterminated quote");
    }

    if !current.is_empty() {
        args.push(current);
    }

    Ok(args)
}
