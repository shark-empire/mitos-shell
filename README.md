<p align="center">
  <img src="https://img.shields.io/badge/Rust-1.70+-orange?logo=rust" alt="Rust">
  <img src="https://img.shields.io/badge/License-MIT-blue.svg" alt="License">
  <img src="https://img.shields.io/github/actions/workflow/status/yourusername/mitos-shell/ci.yml?branch=main&label=CI" alt="CI Status">
</p>

<h1 align="center">MITOS OS</h1>
<p align="center">
  <strong>A complete, from-scratch Unix-like userspace built entirely in Rust.</strong><br>
  Featuring a POSIX-compliant shell, a custom PID 1 init system, and a native privilege manager.
</p>

---

## 🚀 Overview

**MITOS** is not just a shell; it is a complete foundational userspace for a custom Linux distribution. Built from the ground up in Rust, it replaces core system utilities (`bash`, `sudo`, `systemd`/`sysvinit`) with memory-safe, high-performance native binaries.

The project consists of three core pillars:
1. **`mitos`**: A fully-featured, POSIX-compliant command-line interpreter with advanced scripting capabilities, job control, and intelligent tab-completion.
2. **`mitos-init`**: A lightweight, robust PID 1 init system that mounts virtual filesystems and spawns the MITOS shell on `tty1`.
3. **`lala`**: A custom privilege manager (replacement for `sudo`) that uses native Linux `setuid` and `crypt(3)` to securely escalate privileges.

---

## ✨ Core Features

### 🐚 MITOS Shell (`/usr/bin/mitos`)
* **Advanced Parsing:** Hand-written recursive-descent parser with a custom AST.
* **Scripting Language:** Full support for `if/else`, `while`, `for`, `case/esac`, and functions.
* **Data Structures:** Native 1D arrays (`arr=(a b c)`) and complex expansions (`${arr[@]}`, `${#arr[@]}`).
* **Job Control:** True POSIX job control. TTY handoff, process groups, `Ctrl+Z` suspension, `fg`, and `bg`.
* **Robustness:** `set -e` (errexit), `set -u` (nounset), and `set -x` (xtrace).
* **Advanced I/O:** Here-Documents (`<<EOF`), Here-Strings (`<<<`), and `read` with timeouts (`-t`) and silent modes (`-s`).
* **Interactive UX:** Intelligent tab-completion (commands, files, variables), syntax validation, and persistent command history.

### ⚡ MITOS Init (`/usr/bin/mitos-init`)
* **True PID 1:** Acts as the ancestor of all processes.
* **Boot Sequence:** Automatically mounts `proc`, `sysfs`, `devtmpfs`, `devpts`, and `tmpfs`.
* **Terminal Management:** Spawns the MITOS shell on `/dev/tty1` and automatically respawns it if the user logs out.
* **Zombie Reaping:** Constantly reaps orphaned child processes to prevent system resource leaks.

### 🛡️ Lala Privilege Manager (`/usr/bin/lala`)
* **Drop-in `sudo` Replacement:** Execute commands as root via `lala apt update`.
* **Native Authentication:** Reads `/etc/shadow` and verifies passwords using the system's `crypt(3)` library.
* **Silent Prompts:** Disables terminal echo securely using `termios` during password entry.
* **Setuid Architecture:** Relies on the Linux kernel's `setuid` bit for secure, auditable privilege escalation.

---

## 🏗️ Architecture

```text
mitos-shell/
├── src/
│   ├── main.rs               # Entry point (REPL vs Script mode)
│   ├── lexer/                # Tokenization (Quotes, Here-Docs, Arrays)
│   ├── parser/               # AST generation (Control flow, pipelines)
│   ├── expansion/            # Variable, Glob, Tilde, and Array expansion
│   ├── execution/            # Fork/exec engine, pipes, redirections, TTY handoff
│   ├── builtins/             # Native commands (cd, export, test, read, eval, trap)
│   ├── completion/           # Rustyline integration (Tab completion, syntax validation)
│   ├── process/              # Job table, background process tracking
│   ├── terminal/             # TTY manager (tcsetpgrp handoffs)
│   ├── config/               # Shell options (set -e) and startup files (~/.mitosrc)
│   └── bin/
│       ├── lala.rs           # The custom privilege manager binary
│       └── mitos-init.rs     # The PID 1 init system binary
├── debian/                   # OS integration scripts (postinst, prerm, skel)
└── Cargo.toml                # Dependencies (nix, rustyline, libc, glob)
