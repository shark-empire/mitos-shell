use crate::builtins;
use crate::error::{Result, ShellError};
use crate::execution::outcome::ExecOutcome;
use crate::expansion::expander::Expander;
use crate::lexer::token::Token;
use crate::lexer::Lexer;
use crate::parser::ast::*;
use crate::parser::Parser;
use crate::process::job::{JobStatus, JobTable};
use crate::process::job_control::JobControl;
use crate::terminal::tty::TtyManager;
use crate::util::{command_exists, set_var};
use nix::poll::{poll, PollFd, PollFlags, PollTimeout};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::read as nix_read;
use nix::unistd::{fork, ForkResult, Pid};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::os::fd::{BorrowedFd, IntoRawFd};

pub struct Executor {
    pub tty: Option<TtyManager>,
    pub jobs: JobTable,
    pub last_status: i32,
    functions: HashMap<String, FunctionDef>,
    context_stack: Vec<Vec<String>>,
    pub options: crate::config::options::ShellOptions,
    pub traps: HashMap<String, String>,
    pub arrays: HashMap<String, Vec<String>>,
}

impl Executor {
    pub fn new() -> Self {
        Self {
            tty: TtyManager::init(),
            jobs: JobTable::new(),
            last_status: 0,
            functions: HashMap::new(),
            context_stack: vec![Vec::new()],
            options: crate::config::options::ShellOptions::default(),
            traps: HashMap::new(),
            arrays: HashMap::new(),
        }
    }

    pub fn push_context(&mut self, args: Vec<String>) {
        self.context_stack.push(args);
    }

    fn pop_context(&mut self) {
        if self.context_stack.len() > 1 {
            self.context_stack.pop();
        }
    }

    pub fn current_args(&self) -> &[String] {
        self.context_stack
            .last()
            .map(|values| values.as_slice())
            .unwrap_or(&[])
    }

    pub fn last_status(&self) -> i32 {
        self.last_status
    }

    pub fn source_file(&mut self, path: &str, args: &[String]) -> Result<ExecOutcome> {
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(error) => {
                eprintln!("mitos: {}: {}", path, error);
                return Ok(ExecOutcome::Status(1));
            }
        };

        let tokens: Vec<_> = Lexer::new(&content).collect();
        let ast = match Parser::new(tokens).parse() {
            Ok(ast) => ast,
            Err(error) => {
                eprintln!("mitos: {}: syntax error: {}", path, error);
                return Ok(ExecOutcome::Status(2));
            }
        };

        let pushed_context = !args.is_empty();
        if pushed_context {
            self.push_context(args.to_vec());
        }

        let result = self.exec_node(&ast);

        if pushed_context {
            self.pop_context();
        }

        match result {
            Ok(outcome) => Ok(outcome),
            Err(error) => {
                eprintln!("mitos: {}: {}", path, error);
                Ok(ExecOutcome::Status(1))
            }
        }
    }

    pub fn execute(&mut self, node: Node) -> Result<Option<i32>> {
        self.reap_children();
        match self.exec_node(&node)? {
            ExecOutcome::Eval(code) => {
                let tokens: Vec<_> = Lexer::new(&code).collect();
                let ast = Parser::new(tokens).parse()?;
                self.execute(ast)
            }
            ExecOutcome::Exit(code) => Ok(Some(code)),
            other => {
                self.last_status = other.status_or_zero();
                Ok(None)
            }
        }
    }

    fn exec_node(&mut self, node: &Node) -> Result<ExecOutcome> {
        if crate::INTERRUPTED.load(std::sync::atomic::Ordering::SeqCst) {
            crate::INTERRUPTED.store(false, std::sync::atomic::Ordering::SeqCst);

            if let Some(cmd) = self.traps.get("INT").cloned() {
                let tokens: Vec<_> = Lexer::new(&cmd).collect();
                if let Ok(ast) = Parser::new(tokens).parse() {
                    let _ = self.execute(ast);
                }
            } else {
                eprintln!();
                return Ok(ExecOutcome::Exit(130));
            }
        }

        match node {
            Node::Pipeline(p) => self.exec_pipeline(p),
            Node::Sequence(l, r) => match self.exec_node(l)? {
                ExecOutcome::Status(_) => self.exec_node(r),
                other => Ok(other),
            },
            Node::AndOr(l, op, r) => match self.exec_node(l)? {
                ExecOutcome::Status(s) => {
                    let run_right = match op {
                        ListOp::And => s == 0,
                        ListOp::Or => s != 0,
                    };
                    if run_right {
                        self.exec_node(r)
                    } else {
                        Ok(ExecOutcome::Status(s))
                    }
                }
                other => Ok(other),
            },
            Node::Background(inner) => self.exec_background(inner),
            Node::Subshell(inner) => self.exec_subshell(inner),
            Node::BraceGroup(inner) => self.exec_node(inner),
            Node::If(c) => self.exec_if(c),
            Node::While(c) => self.exec_while(c),
            Node::For(c) => self.exec_for(c),
            Node::Case(c) => self.exec_case(c),
            Node::Function(f) => {
                self.functions.insert(f.name.clone(), f.clone());
                Ok(ExecOutcome::Status(0))
            }
        }
    }

    fn exec_pipeline(&mut self, pipeline: &Pipeline) -> Result<ExecOutcome> {
        if pipeline.commands.len() == 1
            && pipeline.commands[0].args.first().map(|s| s.as_str()) == Some("set")
        {
            let status = builtins::set::execute(&pipeline.commands[0].args, &mut self.options);
            return Ok(ExecOutcome::Status(status));
        }

        if pipeline.commands.len() == 1
            && pipeline.commands[0].args.first().map(|s| s.as_str()) == Some("eval")
        {
            if let Some(code) = builtins::eval::execute(&pipeline.commands[0].args) {
                return Ok(ExecOutcome::Eval(code));
            }
            return Ok(ExecOutcome::Status(0));
        }

        if self.options.xtrace {
            let cmd_str = pipeline
                .commands
                .iter()
                .map(|c| c.args.join(" "))
                .collect::<Vec<_>>()
                .join(" | ");
            eprintln!("+ {}", cmd_str);
        }

        let mut expanded_commands = Vec::new();

        if pipeline.commands.len() == 1 {
            let first = self.expand_command(&pipeline.commands[0])?;

            if first.args.is_empty() {
                for assignment in &first.assignments {
                    match assignment {
                        Assignment::Scalar(k, v) => set_var(k, v),
                        Assignment::Array(name, elements) => {
                            self.arrays.insert(name.clone(), elements.clone());
                        }
                    }
                }
                return Ok(ExecOutcome::Status(0));
            }

            if first.args[0] == "source" || first.args[0] == "." {
                if first.args.len() < 2 {
                    eprintln!("mitos: {}: expected a file", first.args[0]);
                    return Ok(ExecOutcome::Status(2));
                }
                return self.source_file(&first.args[1], &first.args[2..]);
            }

            // Intercept `read` so it can mutate shell state (like arrays)
            if first.args[0] == "read" {
                return self.execute_read(&first.args);
            }

            if let Some(outcome) = builtins::try_execute(self, &first.args) {
                return Ok(outcome);
            }

            if let Some(function) = self.functions.get(&first.args[0]).cloned() {
                return self.exec_function(&function, &first.args[1..]);
            }

            expanded_commands.push(first);
        } else {
            for command in &pipeline.commands {
                expanded_commands.push(self.expand_command(command)?);
            }
        }

        // Validate every stage up front: forking only to watch execvp fail
        // wastes a process, and for a multi-stage pipeline it would leave
        // the other stages running against a broken pipe.
        for cmd in &expanded_commands {
            if let Some(name) = cmd.args.first() {
                if !command_exists(name) {
                    let error = ShellError::NotFound(name.clone());
                    eprintln!("mitos: {}", error);
                    self.last_status = 127;
                    return Ok(ExecOutcome::Status(127));
                }
            }
        }

        let status = self.fork_pipeline(&expanded_commands)?;

        if status != 0 && self.options.errexit {
            return Ok(ExecOutcome::Exit(status));
        }

        self.last_status = status;
        Ok(ExecOutcome::Status(status))
    }

    fn exec_function(&mut self, fdef: &FunctionDef, args: &[String]) -> Result<ExecOutcome> {
        self.push_context(args.to_vec());
        let result = match self.exec_node(&fdef.body)? {
            ExecOutcome::Return(s) => Ok(ExecOutcome::Status(s)),
            other => Ok(other),
        };
        self.pop_context();
        result
    }

    fn exec_if(&mut self, c: &IfClause) -> Result<ExecOutcome> {
        match self.exec_node(&c.condition)? {
            ExecOutcome::Status(0) => self.exec_node(&c.then_branch),
            ExecOutcome::Status(_) => match &c.else_branch {
                Some(eb) => self.exec_node(eb),
                None => Ok(ExecOutcome::Status(0)),
            },
            other => Ok(other),
        }
    }

    fn exec_while(&mut self, c: &WhileClause) -> Result<ExecOutcome> {
        loop {
            match self.exec_node(&c.condition)? {
                ExecOutcome::Status(0) => match self.exec_node(&c.body)? {
                    ExecOutcome::Break => return Ok(ExecOutcome::Status(0)),
                    ExecOutcome::Continue => continue,
                    ExecOutcome::Status(_) => continue,
                    other => return Ok(other),
                },
                ExecOutcome::Status(_) => return Ok(ExecOutcome::Status(0)),
                other => return Ok(other),
            }
        }
    }

    fn exec_for(&mut self, c: &ForClause) -> Result<ExecOutcome> {
        let expander = Expander::new(
            self.last_status,
            self.current_args().to_vec(),
            self.options.clone(),
            self.arrays.clone(),
        );

        let mut words = Vec::new();
        for w in &c.words {
            let tokens: Vec<Token> = Lexer::new(w).collect();
            words.extend(expander.expand_tokens(tokens)?);
        }

        let mut status = 0;
        for word in words {
            set_var(&c.var, &word);
            match self.exec_node(&c.body)? {
                ExecOutcome::Break => return Ok(ExecOutcome::Status(status)),
                ExecOutcome::Continue => continue,
                ExecOutcome::Status(s) => status = s,
                other => return Ok(other),
            }
        }
        Ok(ExecOutcome::Status(status))
    }

    fn exec_case(&mut self, c: &CaseClause) -> Result<ExecOutcome> {
        let expander = Expander::new(
            self.last_status,
            self.current_args().to_vec(),
            self.options.clone(),
            self.arrays.clone(),
        );

        let tokens: Vec<Token> = Lexer::new(&c.word).collect();
        let target = expander
            .expand_tokens(tokens)?
            .into_iter()
            .next()
            .unwrap_or_default();

        for branch in &c.branches {
            for pattern in &branch.patterns {
                if let Ok(pat) = glob::Pattern::new(pattern) {
                    if pat.matches(&target) {
                        return self.exec_node(&branch.body);
                    }
                } else if pattern == &target {
                    return self.exec_node(&branch.body);
                }
            }
        }
        Ok(ExecOutcome::Status(0))
    }

    fn exec_subshell(&mut self, inner: &Node) -> Result<ExecOutcome> {
        let inner = inner.clone();
        match unsafe { fork()? } {
            ForkResult::Parent { child } => {
                let status = self.wait_for_child(child)?;
                Ok(ExecOutcome::Status(status))
            }
            ForkResult::Child => {
                let code = match self.exec_node(&inner) {
                    Ok(o) => o.status_or_zero(),
                    Err(_) => 1,
                };
                std::process::exit(code);
            }
        }
    }

    fn exec_background(&mut self, inner: &Node) -> Result<ExecOutcome> {
        // Fast path: a single external command with no redirects or
        // assignments can be spawned directly via JobControl, which puts it
        // in its own process group. `jobs`/`fg`/`bg` signal a job by calling
        // killpg on its pgid, so without this every backgrounded command
        // would stay in the shell's own group and those builtins would
        // target the wrong process (or the whole shell). Anything else —
        // compound commands, builtins, functions, or redirects — still
        // goes through the general recursive fork below, since JobControl
        // only knows how to execvp a flat argument list.
        //
        // Eligibility is checked against the *raw*, unexpanded command
        // first: expansion can run command substitutions, and calling it
        // here just to check eligibility — then again in the fallback
        // path below — would run those substitutions twice.
        if let Node::Pipeline(p) = inner {
            if !p.negated && p.commands.len() == 1 {
                let raw = &p.commands[0];
                let looks_plain_external = raw.redirects.is_empty()
                    && raw.assignments.is_empty()
                    && raw
                        .args
                        .first()
                        .map(|name| {
                            !builtins::is_builtin(name) && !self.functions.contains_key(name)
                        })
                        .unwrap_or(false);

                if looks_plain_external {
                    let expanded = self.expand_command(raw)?;
                    if !expanded.args.is_empty() {
                        let child = JobControl::spawn_process(
                            &expanded.args,
                            Pid::from_raw(0),
                            0,
                            1,
                            false,
                        )?;
                        let job_id = self.jobs.add(child, expanded.args.join(" "));
                        println!("[{}] {}", job_id, child);
                        return Ok(ExecOutcome::Status(0));
                    }
                    // Expansion produced no args (e.g. an unmatched glob) —
                    // fall through to the general path below.
                }
            }
        }

        let inner = inner.clone();
        match unsafe { fork()? } {
            ForkResult::Parent { child } => {
                let job_id = self.jobs.add(child, "<background>".into());
                println!("[{}] {}", job_id, child);
                Ok(ExecOutcome::Status(0))
            }
            ForkResult::Child => {
                let code = match self.exec_node(&inner) {
                    Ok(o) => o.status_or_zero(),
                    Err(_) => 1,
                };
                std::process::exit(code);
            }
        }
    }

    fn fork_pipeline(&mut self, commands: &[SimpleCommand]) -> Result<i32> {
        use nix::fcntl::{open, OFlag};
        use nix::sys::stat::Mode;
        use nix::unistd::{close, dup2, pipe};
        use std::ffi::CString;

        let n = commands.len();
        let mut pipes: Vec<(i32, i32)> = Vec::new();
        for _ in 0..n.saturating_sub(1) {
            // nix 0.29's pipe() returns owned fds; convert to raw fds since the
            // rest of this function manages fd lifetimes manually via close().
            let (r, w) = pipe()?;
            pipes.push((r.into_raw_fd(), w.into_raw_fd()));
        }

        let mut children = Vec::new();

        for (i, cmd) in commands.iter().enumerate() {
            if cmd.args.is_empty() {
                continue;
            }

            match unsafe { fork()? } {
                ForkResult::Parent { child } => children.push(child),
                ForkResult::Child => {
                    // Wrapping the setup logic in a closure lets `?` stay
                    // ergonomic below while guaranteeing every path exits
                    // directly: a forked child must never return through
                    // the caller's control flow, since (being a copy of
                    // the same process) that would resume the parent
                    // shell's own REPL as a duplicate process instead of
                    // terminating.
                    let run = || -> Result<()> {
                        if i > 0 {
                            dup2(pipes[i - 1].0, 0)?;
                        }
                        if i < n - 1 {
                            dup2(pipes[i].1, 1)?;
                        }
                        for (r, w) in &pipes {
                            let _ = close(*r);
                            let _ = close(*w);
                        }

                        for redir in &cmd.redirects {
                            match redir {
                                Redirect::Input(p) => {
                                    let fd = open(p.as_str(), OFlag::O_RDONLY, Mode::empty())?;
                                    dup2(fd, 0)?;
                                    let _ = close(fd);
                                }
                                Redirect::Output(p) => {
                                    let fd = open(
                                        p.as_str(),
                                        OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_TRUNC,
                                        Mode::from_bits(0o644).unwrap(),
                                    )?;
                                    dup2(fd, 1)?;
                                    let _ = close(fd);
                                }
                                Redirect::Append(p) => {
                                    let fd = open(
                                        p.as_str(),
                                        OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_APPEND,
                                        Mode::from_bits(0o644).unwrap(),
                                    )?;
                                    dup2(fd, 1)?;
                                    let _ = close(fd);
                                }
                                Redirect::HereString(s) | Redirect::HereDoc(s, _, _) => {
                                    let path = format!(
                                        "/tmp/mitos_heredoc_{}_{}",
                                        std::process::id(),
                                        i
                                    );
                                    let _ = fs::write(&path, s);
                                    let fd = open(path.as_str(), OFlag::O_RDONLY, Mode::empty())?;
                                    dup2(fd, 0)?;
                                    let _ = close(fd);
                                    let _ = fs::remove_file(path);
                                }
                            }
                        }

                        for assignment in &cmd.assignments {
                            if let Assignment::Scalar(k, v) = assignment {
                                set_var(k, v);
                            }
                        }

                        let c_args: Vec<CString> = cmd
                            .args
                            .iter()
                            .map(|s| CString::new(s.as_str()).unwrap())
                            .collect();
                        let _ = nix::unistd::execvp(&c_args[0], &c_args);
                        Ok(())
                    };

                    if let Err(error) = run() {
                        eprintln!("mitos: {}: {}", cmd.args[0], error);
                        std::process::exit(126);
                    }
                    eprintln!("mitos: command not found: {}", cmd.args[0]);
                    std::process::exit(127);
                }
            }
        }

        for (r, w) in pipes {
            let _ = close(r);
            let _ = close(w);
        }

        let mut last = 0;
        for child in children {
            match waitpid(child, None)? {
                WaitStatus::Exited(_, s) => last = s,
                WaitStatus::Signaled(_, sig, _) => last = 128 + sig as i32,
                _ => {}
            }
        }
        Ok(last)
    }

    fn wait_for_child(&self, child: Pid) -> Result<i32> {
        match waitpid(child, None)? {
            WaitStatus::Exited(_, s) => Ok(s),
            WaitStatus::Signaled(_, sig, _) => Ok(128 + sig as i32),
            _ => Ok(0),
        }
    }

    fn reap_children(&mut self) {
        loop {
            match waitpid(Pid::from_raw(-1), Some(WaitPidFlag::WNOHANG)) {
                Ok(WaitStatus::StillAlive) | Err(_) => break,
                Ok(WaitStatus::Exited(pid, code)) => {
                    self.report_job_done(pid, JobStatus::Exited(code));
                }
                Ok(WaitStatus::Signaled(pid, sig, _)) => {
                    self.report_job_done(pid, JobStatus::Signaled(sig));
                }
                Ok(_) => continue,
            }
        }
    }

    /// Marks a reaped background job as finished, prints a completion
    /// notice (matching common shell behavior for `jobs`), and drops it
    /// from the table.
    fn report_job_done(&mut self, pid: Pid, status: JobStatus) {
        if let Some(job) = self.jobs.jobs.iter().find(|j| j.pgid == pid) {
            println!("[{}]+  Done                    {}", job.id, job.command);
        }
        self.jobs.update_status(pid, status);
        self.jobs.cleanup_finished();
    }

    fn expand_command(&self, command: &SimpleCommand) -> Result<SimpleCommand> {
        let expander = Expander::new(
            self.last_status,
            self.current_args().to_vec(),
            self.options.clone(),
            self.arrays.clone(),
        );

        let mut expanded = command.clone();
        expanded.args.clear();
        expanded.assignments.clear();
        expanded.redirects.clear();

        for arg in &command.args {
            let tokens: Vec<Token> = Lexer::new(arg).collect();
            expanded.args.extend(expander.expand_tokens(tokens)?);
        }

        for assignment in &command.assignments {
            match assignment {
                Assignment::Scalar(key, value) => {
                    let tokens: Vec<Token> = Lexer::new(value).collect();
                    let expanded_value = expander
                        .expand_tokens(tokens)?
                        .into_iter()
                        .next()
                        .unwrap_or_default();
                    expanded
                        .assignments
                        .push(Assignment::Scalar(key.clone(), expanded_value));
                }
                Assignment::Array(name, elements) => {
                    let mut expanded_elements = Vec::new();
                    for e in elements {
                        let tokens: Vec<Token> = Lexer::new(e).collect();
                        expanded_elements.extend(expander.expand_tokens(tokens)?);
                    }
                    expanded
                        .assignments
                        .push(Assignment::Array(name.clone(), expanded_elements));
                }
            }
        }

        for redirect in &command.redirects {
            match redirect {
                Redirect::Input(path) => {
                    let tokens: Vec<Token> = Lexer::new(path).collect();
                    let p = expander
                        .expand_tokens(tokens)?
                        .into_iter()
                        .next()
                        .unwrap_or_default();
                    expanded.redirects.push(Redirect::Input(p));
                }
                Redirect::Output(path) => {
                    let tokens: Vec<Token> = Lexer::new(path).collect();
                    let p = expander
                        .expand_tokens(tokens)?
                        .into_iter()
                        .next()
                        .unwrap_or_default();
                    expanded.redirects.push(Redirect::Output(p));
                }
                Redirect::Append(path) => {
                    let tokens: Vec<Token> = Lexer::new(path).collect();
                    let p = expander
                        .expand_tokens(tokens)?
                        .into_iter()
                        .next()
                        .unwrap_or_default();
                    expanded.redirects.push(Redirect::Append(p));
                }
                Redirect::HereString(s) => {
                    let tokens: Vec<Token> = Lexer::new(s).collect();
                    let expanded_s = expander.expand_tokens(tokens)?.join(" ");
                    expanded.redirects.push(Redirect::HereString(expanded_s));
                }
                Redirect::HereDoc(body, strip, expand) => {
                    // Heredoc bodies aren't shell words (no quote syntax to
                    // respect), so they get the same variable/command/
                    // arithmetic expansion as a double-quoted string, minus
                    // glob/word-splitting — unless the delimiter requested
                    // no expansion at all (`<<'EOF'`).
                    let expanded_body = if *expand {
                        expander.expand_string(body)?
                    } else {
                        body.clone()
                    };
                    expanded
                        .redirects
                        .push(Redirect::HereDoc(expanded_body, *strip, *expand));
                }
            }
        }

        Ok(expanded)
    }

    fn execute_read(&mut self, args: &[String]) -> Result<ExecOutcome> {
        let mut prompt = None;
        let mut silent = false;
        let mut timeout = None;
        let mut delim = b'\n';
        let mut array_name = None;
        let mut vars = Vec::new();

        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "-p" => {
                    prompt = args.get(i + 1).cloned();
                    i += 2;
                }
                "-s" => {
                    silent = true;
                    i += 1;
                }
                "-t" => {
                    if let Some(t) = args.get(i + 1).and_then(|s| s.parse::<f64>().ok()) {
                        timeout = Some(t);
                    }
                    i += 2;
                }
                "-d" => {
                    if let Some(d) = args.get(i + 1) {
                        delim = d.bytes().next().unwrap_or(b'\n');
                    }
                    i += 2;
                }
                "-a" => {
                    array_name = args.get(i + 1).cloned();
                    i += 2;
                }
                "--" => {
                    i += 1;
                    break;
                }
                _ => break,
            }
        }

        while i < args.len() {
            vars.push(args[i].clone());
            i += 1;
        }

        if let Some(p) = prompt {
            eprint!("{}", p);
            let _ = io::stderr().flush();
        }

        let mut old_termios: Option<libc::termios> = None;
        if silent {
            unsafe {
                let mut t: libc::termios = std::mem::zeroed();
                if libc::tcgetattr(0, &mut t) == 0 {
                    old_termios = Some(t);
                    let mut new_t = t;
                    new_t.c_lflag &= !libc::ECHO;
                    libc::tcsetattr(0, libc::TCSAFLUSH, &new_t);
                }
            }
        }

        let mut buffer = Vec::new();
        let stdin_fd = unsafe { BorrowedFd::borrow_raw(0) };
        let start_time = std::time::Instant::now();

        loop {
            let mut remaining_ms = None;
            if let Some(t) = timeout {
                let elapsed = start_time.elapsed().as_secs_f64();
                let rem = t - elapsed;
                if rem <= 0.0 {
                    if silent {
                        println!();
                    }
                    return Ok(ExecOutcome::Status(142));
                }
                remaining_ms = Some((rem * 1000.0) as u16);
            }

            let mut fds = [PollFd::new(stdin_fd, PollFlags::POLLIN)];
            let poll_timeout = remaining_ms
                .map(PollTimeout::from)
                .unwrap_or(PollTimeout::NONE);

            match poll(&mut fds, poll_timeout) {
                Ok(0) => {
                    if silent {
                        println!();
                    }
                    return Ok(ExecOutcome::Status(142));
                }
                Ok(_) => {
                    let mut byte = [0; 1];
                    match nix_read(0, &mut byte) {
                        Ok(0) => break,
                        Ok(_) => {
                            if byte[0] == delim {
                                break;
                            }
                            buffer.push(byte[0]);
                        }
                        Err(_) => break,
                    }
                }
                Err(_) => break,
            }
        }

        if let Some(t) = old_termios {
            unsafe {
                libc::tcsetattr(0, libc::TCSAFLUSH, &t);
            }
            println!();
        }

        let input = String::from_utf8_lossy(&buffer)
            .trim_end_matches('\r')
            .to_string();

        if let Some(arr) = array_name {
            let words: Vec<String> = input.split_whitespace().map(String::from).collect();
            self.arrays.insert(arr, words);
        } else {
            if vars.is_empty() {
                vars.push("REPLY".to_string());
            }

            if vars.len() == 1 {
                crate::util::set_var(&vars[0], &input);
            } else {
                let words: Vec<&str> = input.split_whitespace().collect();
                for (idx, var) in vars.iter().enumerate() {
                    if idx == vars.len() - 1 {
                        crate::util::set_var(var, words[idx..].join(" "));
                    } else {
                        crate::util::set_var(var, words.get(idx).unwrap_or(&""));
                    }
                }
            }
        }

        Ok(ExecOutcome::Status(0))
    }
}
