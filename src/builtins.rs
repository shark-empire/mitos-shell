use std::env;
use std::io::{self, Write};
use std::path::PathBuf;

pub enum BuiltinResult {
    NotBuiltin,
    Continue(i32),
    Exit(i32),
}

pub fn execute(command: &str, args: &[String], last_status: i32) -> BuiltinResult {
    match command {
        "cd" => BuiltinResult::Continue(cd(args)),

        "pwd" => BuiltinResult::Continue(pwd()),

        "echo" => BuiltinResult::Continue(echo(args)),

        "export" => BuiltinResult::Continue(export(args)),

        "clear" => BuiltinResult::Continue(clear()),

        "help" => BuiltinResult::Continue(help()),

        "exit" => BuiltinResult::Exit(exit(args, last_status)),

        _ => BuiltinResult::NotBuiltin,
    }
}

fn cd(args: &[String]) -> i32 {
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
        match env::var("OLDPWD") {
            Ok(previous) => PathBuf::from(previous),

            Err(_) => {
                eprintln!("cd: OLDPWD not set");
                return 1;
            }
        }
    } else {
        expand_home(&args[0])
    };

    if let Err(error) = env::set_current_dir(&target) {
        eprintln!("cd: {}: {}", target.display(), error);
        return 1;
    }

    unsafe {
        env::set_var("OLDPWD", current_dir);
    }

    if let Ok(new_dir) = env::current_dir() {
        unsafe {
            env::set_var("PWD", &new_dir);
        }
    }

    0
}

fn pwd() -> i32 {
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

fn echo(args: &[String]) -> i32 {
    println!("{}", args.join(" "));
    0
}

fn export(args: &[String]) -> i32 {
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

fn clear() -> i32 {
    print!("\x1B[2J\x1B[H");

    if let Err(error) = io::stdout().flush() {
        eprintln!("clear: {}", error);
        return 1;
    }

    0
}

fn help() -> i32 {
    println!("MITOS Shell");
    println!();
    println!("Built-in commands:");
    println!("  cd [DIR]       Change directory");
    println!("  cd -           Return to previous directory");
    println!("  pwd            Print current directory");
    println!("  echo [TEXT]    Print text");
    println!("  export VAR=V   Set environment variable");
    println!("  clear          Clear terminal");
    println!("  help           Show help");
    println!("  exit [STATUS]  Exit shell");
    println!();
    println!("External commands are searched using PATH.");

    0
}

fn exit(args: &[String], last_status: i32) -> i32 {
    if args.is_empty() {
        return last_status;
    }

    match args[0].parse::<i32>() {
        Ok(status) => status,

        Err(_) => {
            eprintln!("exit: numeric argument required");
            2
        }
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
