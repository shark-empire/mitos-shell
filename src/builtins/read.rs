use crate::util::set_var;
use std::io::{self, Write};

/// MITOS `read` builtin.
///
/// Supported options:
///   -p PROMPT   Print prompt before reading
///   -s          Silent mode, do not echo input
///   -r          Raw mode, accepted for compatibility
///
/// Examples:
///   read name
///   read -p "Name: " name
///   read -s -p "Password: " password
pub fn execute(args: &[String]) -> i32 {
    let mut prompt: Option<String> = None;
    let mut silent = false;
    let mut raw = false;
    let mut vars: Vec<String> = Vec::new();

    let mut index = 1;

    // Parse options.
    while index < args.len() {
        match args[index].as_str() {
            "-p" => {
                if index + 1 >= args.len() {
                    eprintln!("read: -p requires an argument");
                    return 2;
                }

                prompt = Some(args[index + 1].clone());
                index += 2;
            }

            "-s" => {
                silent = true;
                index += 1;
            }

            "-r" => {
                raw = true;
                index += 1;
            }

            "--" => {
                index += 1;
                break;
            }

            arg if arg.starts_with('-') && arg.len() > 1 && arg != "-" => {
                eprintln!("read: invalid option: {}", arg);
                return 2;
            }

            _ => {
                break;
            }
        }
    }

    // Remaining arguments are variable names.
    while index < args.len() {
        vars.push(args[index].clone());
        index += 1;
    }

    // POSIX-like default variable.
    if vars.is_empty() {
        vars.push("REPLY".to_string());
    }

    // We accept -r for compatibility, but this implementation does not
    // currently perform backslash escape processing.
    let _ = raw;

    // Print prompt.
    if let Some(prompt) = prompt {
        eprint!("{}", prompt);

        if let Err(error) = io::stderr().flush() {
            eprintln!("read: prompt error: {}", error);
            return 1;
        }
    }

    let mut line = String::new();

    let result = if silent {
        read_line_silent(&mut line)
    } else {
        io::stdin().read_line(&mut line)
    };

    if silent {
        // Move to the next line after hidden input.
        println!();
    }

    match result {
        Ok(0) => {
            // EOF.
            assign_variables(&vars, "");
            return 1;
        }

        Ok(_) => {
            let line = line.trim_end_matches(|c| c == '\n' || c == '\r');
            assign_variables(&vars, line);
            0
        }

        Err(error) => {
            eprintln!("read: {}", error);
            1
        }
    }
}

fn assign_variables(vars: &[String], line: &str) {
    if vars.len() == 1 {
        set_var(&vars[0], line);
        return;
    }

    let words: Vec<&str> = line.split_whitespace().collect();

    for (index, name) in vars.iter().enumerate() {
        if index == vars.len() - 1 {
            // Last variable receives the remainder of the line.
            if index < words.len() {
                let remainder = words[index..].join(" ");
                set_var(name, remainder);
            } else {
                set_var(name, "");
            }
        } else {
            let value = words.get(index).copied().unwrap_or("");
            set_var(name, value);
        }
    }
}

/// Reads a line with terminal echo disabled.
///
/// If stdin is not a terminal, it falls back to normal reading.
fn read_line_silent(line: &mut String) -> io::Result<usize> {
    unsafe {
        let mut old_termios: libc::termios = std::mem::zeroed();

        // If we cannot get terminal attributes, stdin is probably not a TTY.
        if libc::tcgetattr(libc::STDIN_FILENO, &mut old_termios) != 0 {
            return io::stdin().read_line(line);
        }

        let mut new_termios = old_termios;

        // Disable echo.
        new_termios.c_lflag &= !libc::ECHO;

        libc::tcsetattr(
            libc::STDIN_FILENO,
            libc::TCSAFLUSH,
            &new_termios,
        );

        let result = io::stdin().read_line(line);

        // Restore original terminal attributes.
        libc::tcsetattr(
            libc::STDIN_FILENO,
            libc::TCSAFLUSH,
            &old_termios,
        );

        result
    }
}
