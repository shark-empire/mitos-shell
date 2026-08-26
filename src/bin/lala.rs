// src/bin/lala.rs
use nix::unistd::{execvp, Gid, Uid, User};
use std::env;
use std::ffi::CString;
use std::fs;
use std::io::{self, Read, Write};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: lala <command> [args...]");
        std::process::exit(1);
    }

    if Uid::effective().is_root() {
        exec_command(&args[1..]);
    }

    let current_user = User::from_uid(Uid::current()).unwrap().unwrap();
    eprint!("[lala] password for {}: ", current_user.name);
    io::stderr().flush().unwrap();

    let password = read_password_silently();
    println!();

    if !verify_password(&current_user.name, &password) {
        eprintln!("lala: authentication failure");
        std::process::exit(1);
    }

    nix::unistd::setgid(Gid::from_raw(0)).expect("lala: failed to set GID (is it setuid root?)");
    nix::unistd::setuid(Uid::from_raw(0)).expect("lala: failed to set UID (is it setuid root?)");

    exec_command(&args[1..]);
}

fn exec_command(args: &[String]) -> ! {
    let prog = CString::new(args[0].as_str()).unwrap();
    let c_args: Vec<CString> = args
        .iter()
        .map(|s| CString::new(s.as_str()).unwrap())
        .collect();

    if let Err(e) = execvp(&prog, &c_args) {
        eprintln!("lala: failed to execute {}: {}", args[0], e);
        std::process::exit(127);
    }
    unreachable!()
}

fn read_password_silently() -> String {
    unsafe {
        let mut old_termios: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(0, &mut old_termios) != 0 {
            let mut pwd = String::new();
            io::stdin().read_line(&mut pwd).unwrap();
            return pwd.trim().to_string();
        }

        let mut new_termios = old_termios;
        new_termios.c_lflag &= !libc::ECHO;
        libc::tcsetattr(0, libc::TCSAFLUSH, &new_termios);

        let mut pwd = String::new();
        io::stdin().read_line(&mut pwd).unwrap();

        libc::tcsetattr(0, libc::TCSAFLUSH, &old_termios);
        pwd.trim().to_string()
    }
}

fn verify_password(username: &str, password: &str) -> bool {
    let shadow_content = match fs::read_to_string("/etc/shadow") {
        Ok(c) => c,
        Err(_) => {
            eprintln!("lala: cannot read /etc/shadow (is /usr/bin/lala setuid root?)");
            return false;
        }
    };

    let mut expected_hash = "";
    for line in shadow_content.lines() {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() >= 2 && parts[0] == username {
            expected_hash = parts[1];
            break;
        }
    }

    if expected_hash.is_empty() || expected_hash == "!" || expected_hash == "*" {
        return false;
    }

    let c_pass = CString::new(password).unwrap();
    let c_salt = CString::new(expected_hash).unwrap();

    unsafe {
        let result = libc::crypt(c_pass.as_ptr(), c_salt.as_ptr());
        if result.is_null() {
            return false;
        }
        let hashed = std::ffi::CStr::from_ptr(result).to_str().unwrap_or("");
        hashed == expected_hash
    }
}
