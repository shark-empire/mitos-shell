use crate::builtins;
use crate::process;

use std::env;
use std::io::{self, Write};
use std::path::PathBuf;

pub struct Shell {
    last_status: i32,
}

struct CommandLine {
    command: String,
    args: Vec<String>,
    stdin: Option<String>,
    stdout: Option<String>,
    append_stdout: bool,
}

impl Shell {
    pub fn new() -> Self {
        Self {
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

            let tokens = match parse_command(&expanded) {
                Ok(tokens) => tokens,

                Err(error) => {
                    eprintln!("mitos-shell: {}", error);
                    self.last_status = 2;
                    continue;
                }
            };

            let command_line = match parse_redirection(tokens) {
                Ok(command_line) => command_line,

                Err(error) => {
                    eprintln!("mitos-shell: {}", error);
                    self.last_status = 2;
                    continue;
                }
            };

            if command_line.command.is_empty() {
                continue;
            }

            let command = &command_line.command;
            let arguments = &command_line.args;

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
                    self.last_status =
                        self.execute_external(&command_line);
                }
            }
        }
    }

    fn print_prompt(&self) {
        let cwd = env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"));

        print!("MITOS {} > ", cwd.display());

        if let Err(error) = io::stdout().flush() {
            eprintln!("mitos-shell: prompt error: {}", error);
        }
    }

    fn execute_external(
        &self,
        command_line: &CommandLine,
    ) -> i32 {
        let stdin = match &command_line.stdin {
            Some(path) => {
                match process::open_input(path) {
                    Ok(stdin) => Some(stdin),

                    Err(error) => {
                        eprintln!("MITOS: {}", error);
                        return 1;
                    }
                }
            }

            None => None,
        };

        let stdout = match &command_line.stdout {
            Some(path) => {
                match process::open_output(
                    path,
                    command_line.append_stdout,
                ) {
                    Ok(stdout) => Some(stdout),

                    Err(error) => {
                        eprintln!("MITOS: {}", error);
                        return 1;
                    }
                }
            }

            None => None,
        };

        process::execute_with_io(
            &command_line.command,
            &command_line.args,
            stdin,
            stdout,
            None,
        )
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
                    &self.last_status.to_string(),
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

fn parse_command(
    input: &str,
) -> Result<Vec<String>, &'static str> {
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

fn parse_redirection(
    tokens: Vec<String>,
) -> Result<CommandLine, &'static str> {
    let mut command = String::new();
    let mut args = Vec::new();

    let mut stdin = None;
    let mut stdout = None;
    let mut append_stdout = false;

    let mut index = 0;

    while index < tokens.len() {
        match tokens[index].as_str() {
            "<" => {
                index += 1;

                if index >= tokens.len() {
                    return Err(
                        "expected file after '<'",
                    );
                }

                stdin = Some(tokens[index].clone());
            }

            ">" => {
                index += 1;

                if index >= tokens.len() {
                    return Err(
                        "expected file after '>'",
                    );
                }

                stdout = Some(tokens[index].clone());
                append_stdout = false;
            }

            ">>" => {
                index += 1;

                if index >= tokens.len() {
                    return Err(
                        "expected file after '>>'",
                    );
                }

                stdout = Some(tokens[index].clone());
                append_stdout = true;
            }

            token => {
                if command.is_empty() {
                    command = token.to_string();
                } else {
                    args.push(token.to_string());
                }
            }
        }

        index += 1;
    }

    Ok(CommandLine {
        command,
        args,
        stdin,
        stdout,
        append_stdout,
    })
}
