use crate::builtins;
use crate::error::Result;
use crate::execution::outcome::ExecOutcome;
use crate::expansion::expander::Expander;
use crate::lexer::lexer::Lexer;
use crate::lexer::token::Token;
use crate::parser::ast::*;
use crate::parser::parser::Parser;
use crate::process::job::JobTable;
use crate::terminal::tty::TtyManager;
use crate::util::set_var;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{fork, ForkResult, Pid};
use std::collections::HashMap;
use std::fs;

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
            arrays: HashMap::new(), // Fixed: Initialize arrays
        }
    }

    fn push_context(&mut self, args: Vec<String>) {
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
        if crate::main::INTERRUPTED.load(std::sync::atomic::Ordering::SeqCst) {
            crate::main::INTERRUPTED.store(false, std::sync::atomic::Ordering::SeqCst);
            
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
            Node::Case(c) => self.exec_case(c),
            Node::Function(f) => {
                self.functions.insert(f.name.clone(), f.clone());
                Ok(ExecOutcome::Status(0))
            }
        }
    }

    fn exec_pipeline(&mut self, pipeline: &Pipeline) -> Result<ExecOutcome> {
        if pipeline.commands.len() == 1 && pipeline.commands[0].args.first().map(|s| s.as_str()) == Some("set") {
            let status = builtins::set::execute(&pipeline.commands[0].args, &mut self.options);
            return Ok(ExecOutcome::Status(status));
        }

        if pipeline.commands.len() == 1 && pipeline.commands[0].args.first().map(|s| s.as_str()) == Some("eval") {
            if let Some(code) = builtins::eval::execute(&pipeline.commands[0].args) {
                return Ok(ExecOutcome::Eval(code));
            }
            return Ok(ExecOutcome::Status(0));
        }

        if self.options.xtrace {
            let cmd_str = pipeline.commands.iter()
                .map(|c| c.args.join(" "))
                .collect::<Vec<_>>()
                .join(" | ");
            eprintln!("+ {}", cmd_str);
        }

        let mut expanded_commands = Vec::new();

        if pipeline.commands.len() == 1 {
            let first = self.expand_command(&pipeline.commands[0])?;

            // Bare assignments (Fixed: Handles both Scalar and Array)
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

            if let Some(outcome) = builtins::try_execute(&first.args) {
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
            self.arrays.clone(), // Fixed: Pass arrays
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
            self.arrays.clone(), // Fixed: Pass arrays
        );
        
        let tokens: Vec<Token> = Lexer::new(&c.word).collect();
        let target = expander.expand_tokens(tokens)?.into_iter().next().unwrap_or_default();

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
                            // Fixed: Handle HereString and HereDoc
                            Redirect::HereString(s) | Redirect::HereDoc(s, _, _) => {
                                let path = format!("/tmp/mitos_heredoc_{}_{}", std::process::id(), i);
                                let _ = fs::write(&path, s);
                                let fd = open(path.as_str(), OFlag::O_RDONLY, Mode::empty())?;
                                dup2(fd, 0)?; let _ = close(fd);
                                let _ = fs::remove_file(path);
                            }
                        }
                    }

                    // Fixed: Handle Assignment enum instead of tuples
                    for assignment in &cmd.assignments {
                        if let Assignment::Scalar(k, v) = assignment {
                            set_var(k, v);
                        }
                    }

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
                Ok(_) => continue,
            }
        }
    }

    fn expand_command(&self, command: &SimpleCommand) -> Result<SimpleCommand> {
        let expander = Expander::new(
            self.last_status,
            self.current_args().to_vec(),
            self.options.clone(),
            self.arrays.clone(), // Fixed: Pass arrays
        );

        let mut expanded = command.clone();
        expanded.args.clear();
        expanded.assignments.clear();
        expanded.redirects.clear();

        for arg in &command.args {
            let tokens: Vec<Token> = Lexer::new(arg).collect();
            expanded.args.extend(expander.expand_tokens(tokens)?);
        }

        // Fixed: Handle Assignment enum
        for assignment in &command.assignments {
            match assignment {
                Assignment::Scalar(key, value) => {
                    let tokens: Vec<Token> = Lexer::new(value).collect();
                    let expanded_value = expander.expand_tokens(tokens)?.into_iter().next().unwrap_or_default();
                    expanded.assignments.push(Assignment::Scalar(key.clone(), expanded_value));
                }
                Assignment::Array(name, elements) => {
                    let mut expanded_elements = Vec::new();
                    for e in elements {
                        let tokens: Vec<Token> = Lexer::new(e).collect();
                        expanded_elements.extend(expander.expand_tokens(tokens)?);
                    }
                    expanded.assignments.push(Assignment::Array(name.clone(), expanded_elements));
                }
            }
        }

        // Fixed: Handle all Redirect variants
        for redirect in &command.redirects {
            match redirect {
                Redirect::Input(path) => {
                    let tokens: Vec<Token> = Lexer::new(path).collect();
                    let p = expander.expand_tokens(tokens)?.into_iter().next().unwrap_or_default();
                    expanded.redirects.push(Redirect::Input(p));
                }
                Redirect::Output(path) => {
                    let tokens: Vec<Token> = Lexer::new(path).collect();
                    let p = expander.expand_tokens(tokens)?.into_iter().next().unwrap_or_default();
                    expanded.redirects.push(Redirect::Output(p));
                }
                Redirect::Append(path) => {
                    let tokens: Vec<Token> = Lexer::new(path).collect();
                    let p = expander.expand_tokens(tokens)?.into_iter().next().unwrap_or_default();
                    expanded.redirects.push(Redirect::Append(p));
                }
                Redirect::HereString(s) => {
                    let tokens: Vec<Token> = Lexer::new(s).collect();
                    let expanded_s = expander.expand_tokens(tokens)?.join(" ");
                    expanded.redirects.push(Redirect::HereString(expanded_s));
                }
                Redirect::HereDoc(body, strip, expand) => {
                    expanded.redirects.push(Redirect::HereDoc(body.clone(), *strip, *expand));
                }
            }
        }

        Ok(expanded)
    }
}
