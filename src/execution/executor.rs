use crate::builtins;
use crate::error::Result;
use crate::execution::outcome::ExecOutcome;
use crate::expansion::expander::Expander;
use crate::parser::ast::*;
use crate::process::job::JobTable;
use crate::terminal::tty::TtyManager;
use crate::util::set_var;
use crate::lexer::lexer::Lexer;
use crate::parser::parser::Parser;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{fork, ForkResult, Pid};
use std::collections::HashMap;
use std::fs;

pub struct Executor {
    pub tty: Option<TtyManager>,
    pub jobs: JobTable,
    pub last_status: i32,
    functions: HashMap<String, FunctionDef>,
    /// Stack of positional parameters. The top of the stack is the current $@.
    context_stack: Vec<Vec<String>>, 
}

impl Executor {
    pub fn new() -> Self {
        Self {
            tty: TtyManager::init(),
            jobs: JobTable::new(),
            last_status: 0,
            functions: HashMap::new(),
            context_stack: vec![Vec::new()], // Base context (script args)
        }
    }

        /// Push a new context when entering a function
    fn push_context(&mut self, args: Vec<String>) {
        self.context_stack.push(args);
    }

    /// Pop context when leaving a function
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


    pub fn last_status(&self) -> i32 { self.last_status }


     pub fn source_file(
        &mut self,
        path: &str,
        args: &[String],
    ) -> Result<ExecOutcome> {
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

    /// Top-level entry. Returns the shell exit code if `exit` was seen.
    pub fn execute(&mut self, node: Node) -> Result<Option<i32>> {
        self.reap_children();
        match self.exec_node(&node)? {
            ExecOutcome::Exit(code) => Ok(Some(code)),
            other => {
                self.last_status = other.status_or_zero();
                Ok(None)
            }
        }
    }

    fn exec_node(&mut self, node: &Node) -> Result<ExecOutcome> {
        match node {
            Node::Pipeline(p) => self.exec_pipeline(p),

            Node::Sequence(l, r) => match self.exec_node(l)? {
                ExecOutcome::Status(_) => self.exec_node(r),
                other => Ok(other), // propagate break/continue/return/exit
            },

            Node::AndOr(l, op, r) => match self.exec_node(l)? {
                ExecOutcome::Status(s) => {
                    let run_right = match op {
                        ListOp::And => s == 0,
                        ListOp::Or => s != 0,
                    };
                    if run_right { self.exec_node(r) } else { Ok(ExecOutcome::Status(s)) }
                }
                other => Ok(other),
            },

            Node::Background(inner) => self.exec_background(inner),
            Node::Subshell(inner) => self.exec_subshell(inner),
            Node::BraceGroup(inner) => self.exec_node(inner),
            Node::If(c) => self.exec_if(c),
            Node::While(c) => self.exec_while(c),
            Node::For(c) => self.exec_for(c),

            Node::Function(f) => {
                self.functions.insert(f.name.clone(), f.clone());
                Ok(ExecOutcome::Status(0))
            }
        }
    }

 fn exec_pipeline(&mut self, pipeline: &Pipeline) -> Result<ExecOutcome> {
    let first = self.expand_command(&pipeline.commands[0]);

    // Single-command pipeline.
    if pipeline.commands.len() == 1 {
        // Bare assignments:
        //   FOO=bar
        if first.args.is_empty() {
            for (key, value) in &first.assignments {
                set_var(key, value);
            }

            return Ok(ExecOutcome::Status(0));
        }

        // source / . builtin.
        if first.args[0] == "source" || first.args[0] == "." {
            if first.args.len() < 2 {
                eprintln!("mitos: {}: expected a file", first.args[0]);
                return Ok(ExecOutcome::Status(2));
            }

            return self.source_file(
                &first.args[1],
                &first.args[2..],
            );
        }

        // Regular builtin.
        if let Some(outcome) = builtins::try_execute(&first.args) {
            return Ok(outcome);
        }

        // Function call.
        if let Some(function) = self.functions.get(&first.args[0]).cloned() {
            return self.exec_function(&function, &first.args[1..]);
        }
    }

    // Expand all pipeline commands.
    let mut expanded_commands = vec![first];

    for command in pipeline.commands.iter().skip(1) {
        expanded_commands.push(self.expand_command(command));
    }

    // External pipeline.
    let status = self.fork_pipeline(&expanded_commands)?;

    let status = if pipeline.negated {
        if status == 0 {
            1
        } else {
            0
        }
    } else {
        status
    };

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


    // ---------- control flow ----------
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

    fn exec_case(&mut self, c: &CaseClause) -> Result<ExecOutcome> {
    // Expand the target word
    let expander = Expander::new(self.last_status, self.current_args().to_vec());
    let target = expander.expand_args(vec![c.word.clone()]).pop().unwrap_or_default();

    for branch in &c.branches {
        for pattern in &branch.patterns {
            // Use glob pattern matching
            if let Ok(pat) = glob::Pattern::new(pattern) {
                if pat.matches(&target) {
                    return self.exec_node(&branch.body);
                }
            } else if pattern == &target {
                return self.exec_node(&branch.body);
            }
        }
    }
    Ok(ExecOutcome::Status(0)) // No match
}


    fn exec_for(&mut self, c: &ForClause) -> Result<ExecOutcome> {
        let expander = Expander::new(self.last_status, self.current_args().to_vec())

        let words: Vec<String> = c.words
            .iter()
            .flat_map(|w| expander.expand_args(vec![w.clone()]))
            .collect();

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

    // ---------- subshells & background ----------
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

    // ---------- external pipelines (fork/exec + pipes) ----------
    fn run_external_pipeline(&mut self, commands: &[SimpleCommand]) -> Result<i32> {
        // Expand arguments, then reuse the Phase 2/3 fork-exec-pipe engine.
        let expander = Expander::new(self.last_status);
        let expanded: Vec<SimpleCommand> = commands.iter().map(|c| {
            let mut c = c.clone();
            c.args = expander.expand_args(c.args);
            c
        }).collect();

        self.fork_pipeline(&expanded)
    }

    /// The low-level fork/pipe/exec engine (adapted from Phase 2), now
    /// operating on `SimpleCommand` and applying per-command assignments.
    fn fork_pipeline(&mut self, commands: &[SimpleCommand]) -> Result<i32> {
        use nix::fcntl::{open, OFlag};
        use nix::sys::stat::Mode;
        use nix::unistd::{close, dup2, pipe};
        use std::ffi::CString;

        let n = commands.len();
        let mut pipes: Vec<(i32, i32)> = Vec::new();
        for _ in 0..n.saturating_sub(1) { pipes.push(pipe()?); }

        let mut children = Vec::new();

        for (i, cmd) in commands.iter().enumerate() {
            if cmd.args.is_empty() { continue; }

            match unsafe { fork()? } {
                ForkResult::Parent { child } => children.push(child),
                ForkResult::Child => {
                    if i > 0 { dup2(pipes[i - 1].0, 0)?; }
                    if i < n - 1 { dup2(pipes[i].1, 1)?; }
                    for (r, w) in &pipes { let _ = close(*r); let _ = close(*w); }

                    for redir in &cmd.redirects {
                        match redir {
                            Redirect::Input(p) => {
                                let fd = open(p.as_str(), OFlag::O_RDONLY, Mode::empty())?;
                                dup2(fd, 0)?; let _ = close(fd);
                            }
                            Redirect::Output(p) => {
                                let fd = open(p.as_str(),
                                    OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_TRUNC,
                                    Mode::from_bits(0o644).unwrap())?;
                                dup2(fd, 1)?; let _ = close(fd);
                            }
                            Redirect::Append(p) => {
                                let fd = open(p.as_str(),
                                    OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_APPEND,
                                    Mode::from_bits(0o644).unwrap())?;
                                dup2(fd, 1)?; let _ = close(fd);
                            }
                        }
                    }

                    // Per-command environment prefixes.
                    for (k, v) in &cmd.assignments { set_var(k, v); }

                    let c_args: Vec<CString> = cmd.args.iter()
                        .map(|s| CString::new(s.as_str()).unwrap())
                        .collect();
                    let _ = nix::unistd::execvp(&c_args[0], &c_args);
                    eprintln!("mitos: command not found: {}", cmd.args[0]);
                    std::process::exit(127);
                }
            }
        }

        for (r, w) in pipes { let _ = close(r); let _ = close(w); }

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

    // ---------- helpers ----------
    fn wait_for_child(&self, child: Pid) -> Result<i32> {
        match waitpid(child, None)? {
            WaitStatus::Exited(_, s) => Ok(s),
            WaitStatus::Signaled(_, sig, _) => Ok(128 + sig as i32),
            _ => Ok(0),
        }
    }

    /// Reap any finished background jobs so they don't become zombies.
    fn reap_children(&mut self) {
        loop {
            match waitpid(Pid::from_raw(-1), Some(WaitPidFlag::WNOHANG)) {
                Ok(WaitStatus::StillAlive) | Err(_) => break,
                Ok(_) => continue,
            }
        }
    }


    fn expand_command(&self, command: &SimpleCommand) -> SimpleCommand {
    let expander = Expander::new(
        self.last_status,
        self.current_args().to_vec(),
    );

    let mut expanded = command.clone();

    // Expand command arguments.
    expanded.args = expander.expand_args(command.args.clone());

    // Expand assignment values.
    expanded.assignments = command
        .assignments
        .iter()
        .map(|(key, value)| {
            let expanded_value = expander
                .expand_args(vec![value.clone()])
                .into_iter()
                .next()
                .unwrap_or_default();

            (key.clone(), expanded_value)
        })
        .collect();

    // Expand redirection targets.
    expanded.redirects = command
        .redirects
        .iter()
        .map(|redirect| match redirect {
            Redirect::Input(path) => {
                let path = expander
                    .expand_args(vec![path.clone()])
                    .into_iter()
                    .next()
                    .unwrap_or_default();

                Redirect::Input(path)
            }

            Redirect::Output(path) => {
                let path = expander
                    .expand_args(vec![path.clone()])
                    .into_iter()
                    .next()
                    .unwrap_or_default();

                Redirect::Output(path)
            }

            Redirect::Append(path) => {
                let path = expander
                    .expand_args(vec![path.clone()])
                    .into_iter()
                    .next()
                    .unwrap_or_default();

                Redirect::Append(path)
            }
        })
        .collect();

    expanded
     }

}
