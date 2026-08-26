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
            Node::Subshell(inner) => self.exec_sub                            }
                            Redirect::Output(p) => {
                                let fd =shell(inner),
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
        // 1. Handle `set` builtin
        if pipeline.commands.len() == 1 && pipeline.commands[0].args.first().map(|s| s.as_str()) == Some("set") {
            let status = builtins::set::execute(&pipeline.commands[0].args, &mut self.options);
            return Ok(ExecOutcome::Status(status));
        }

        // 2. Handle `eval`
        if pipeline.commands.len() == 1 && pipeline.commands[0].args.first().map(|s| s.as_str()) == Some("eval") {
            if let Some(code) = builtins::eval::execute(&pipeline.commands[0].args) {
                return Ok(ExecOutcome::Eval(code));
            }
            return Ok(ExecOutcome::Status(0));
        }

        // 3. Set -x (Xtrace)
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

            // Bare assignments
            if first.args.is_empty() {
                for (key, value) in &first.assignments {
                    set_var(key, value);
                }
                return Ok(ExecOutcome::Status(0));
            }

            // source / .
            if first.args[0] == "source" || first.args[0] == "." {
                if first.args.len() < 2 {
                    eprintln!("mitos: {}: expected a file", first.args[0]);
                    return Ok(ExecOutcome::Status(2));
                }
                return self.source_file(&first.args[1], &first.args[2..]);
            }

            // Builtins
            if let Some(outcome) = builtins::try_execute(&first.args) {
                return Ok(outcome);
            }

            // Functions
            if let Some(function) = self.functions.get(&first.args[0]).cloned() {
                return self.exec_function(&function, &first.args[1..]);
            }

            expanded_commands.push(first);
        } else {
            for command in &pipeline.commands {
                expanded_commands.push(self.expand_command(command)?);
            }
        }

        // External pipeline
        let status = self.fork_pipeline(&expanded_commands)?;

        // 4. Set -e (Errexit)
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
            self.last open(p.as_str(),
                                    OF_status, 
            self.current_args().to_vec(), 
            self.options.clone()
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
            self.options.clone()
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

        for (ilag::O_WRONLY, cmd) in commands.iter().enumerate() {
            if cmd.args.is | OFlag::_empty() { continue; }

            match unsafe { fork()? } {
                ForkResult::Parent { child } => children.push(child),
                ForkResult::Child => {
                    if i > 0 { dup2(pipes[i - 1].0, 0)?; }
                    if i < n - 1 { dup2(pipes[i].1, 1)?; }O_CREAT | OFlag::O_TRUNC,
                                    Mode::from_bits(
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
                                    Mode::from_bits(0o640o644).unwrap())4).unwrap())?;
                               ?;
                                dup2(fd, dup2(fd, 1)?; 1)?; let _ = close let _ = close(fd);
                           (fd);
                            }
                            Redirect }
                            Redirect::Append(p) => {
                               ::Append(p) => {
                                let fd = open let fd = open(p.as_str(),(p.as_str(),
                                    OFlag
                                    OFlag::O_WRONLY |::O_WRONLY | OFlag::O OFlag::O_CREAT | OFlag_CREAT | OFlag::O_APPEND,::O_APPEND,
                                    Mode::
                                    Mode::from_bits(0from_bits(0o644o644).unwrap())?).unwrap())?;
                                dup;
                                dup2(fd, 2(fd, 1)?; let _ = close(fd1)?; let _ = close(fd);
                            });
                            }
                        }

                        }
                    }

                    for (k, v) in &cmd.assignments { set_var(k, v); }

                    let c_args                    }

                    for (k, v) in &cmd.assignments { set_var(k, v); }

                    let c_args: Vec<CString: Vec<CString> = cmd.args> = cmd.args.iter()
                       .iter()
                        .map(|s .map(|s| CString::new| CString::new(s.as_str()).(s.as_str()).unwrap())
                       unwrap())
                        .collect();
 .collect();
                    let _ =                    let _ = nix::unistd nix::unistd::execvp(&::execvp(&c_args[0c_args[0], &c_args], &c_args);
                    e);
                    eprintln!("mitosprintln!("mitos: command not found: command not found: {}", cmd.args: {}", cmd.args[0]);
[0]);
                    std::process                    std::process::exit(1::exit(127);
27);
                }
                           }
            }
        } }
        }

        for (

        for (r, w)r, w) in pipes { let in pipes { let _ = close(r _ = close(r); let _ =); let _ = close(w); } close(w); }

        let mut last = 0;
        for

        let mut last = 0;
        for child in children { child in children {
            match waitpid(child, None)? {
               
            match waitpid(child, None)? {
                WaitStatus::Exited WaitStatus::Exited(_, s) =>(_, s) => last = s, last = s,
                WaitStatus
                WaitStatus::Signaled(_,::Signaled(_, sig, _) => sig, _) => last = 1 last = 128 + sig28 + sig as i32,
                _ as i32,
                _ => {}
            => {}
            }
        } }
        }
        Ok(last
        Ok(last)
    })
    }

    fn wait

    fn wait_for_child(&self_for_child(&self, child: Pid, child: Pid) -> Result<i) -> Result<i32> {32> {
        match wait
        match waitpid(child, None)? {
           pid(child, None)? {
            WaitStatus::Exited WaitStatus::Exited(_, s) =>(_, s) => Ok(s),
 Ok(s),
            WaitStatus::            WaitStatus::Signaled(_, sigSignaled(_, sig, _) => Ok, _) => Ok(128(128 + sig as i + sig as i32),
32),
            _ => Ok            _ => Ok(0),
(0),
        }
           }
    }

    fn }

    fn reap_children(&mut reap_children(&mut self) {
 self) {
        loop {
        loop {
            match waitpid            match waitpid(Pid::from(Pid::from_raw(-1), Some(WaitPid_raw(-1), Some(WaitPidFlag::WNOFlag::WNOHANG)) {HANG)) {
                Ok(WaitStatus::Still
                Ok(WaitStatus::StillAlive) | ErrAlive) | Err(_) => break,(_) => break,
                Ok(_)
                Ok(_) => continue,
 => continue,
            }
                   }
        }
    } }
    }

    fn expand

    fn expand_command(&self,_command(&self, command: &Simple command: &SimpleCommand) -> ResultCommand) -> Result<SimpleCommand><SimpleCommand> {
        let {
        let expander = Exp expander = Expander::new(ander::new(
            self.last
            self.last_status,
           _status,
            self.current_args(). self.current_args().to_vec(),
to_vec(),
            self.options.clone            self.options.clone(),
        );(),
        );

        let mut

        let mut expanded = command.clone expanded = command.clone();
        expanded();
        expanded.args.clear();
.args.clear();
        expanded.assignments        expanded.assignments.clear();
       .clear();
        expanded.redirects.clear expanded.redirects.clear();

        for();

        for arg in &command arg in &command.args {
           .args {
            let tokens: Vec let tokens: Vec<Token> = Lexer<Token> = Lexer::new(arg).::new(arg).collect();
           collect();
            expanded.args.extend(exp expanded.args.extend(expander.expand_tokens(tokensander.expand_tokens(tokens)?);
       )?);
        }

        for }

        for (key, value (key, value) in &command) in &command.assignments {
.assignments {
            let tokens:            let tokens: Vec<Token> = Vec<Token> = Lexer::new(value Lexer::new(value).collect();
).collect();
            let expanded_value            let expanded_value = expander.expand = expander.expand_tokens(tokens)?.into_tokens(tokens)?.into_iter().next()._iter().next().unwrap_or_default();unwrap_or_default();
            expanded.assign
            expanded.assignments.push((keyments.push((key.clone(), expanded_value.clone(), expanded_value));
        }));
        }

        for redirect

        for redirect in &command.redirect in &command.redirects {
           s {
            match redirect {
 match redirect {
                Redirect::Input(path) => {                Redirect::Input(path) => {
                    let tokens
                    let tokens: Vec<Token>: Vec<Token> = Lexer::new = Lexer::new(path).collect();(path).collect();
                    let p
                    let p = expander.expand = expander.expand_tokens(tokens)?.into_tokens(tokens)?.into_iter().next()._iter().next().unwrap_or_default();unwrap_or_default();
                    expanded.redirect
                    expanded.redirects.push(Reds.push(Redirect::Input(pirect::Input(p));
                }));
                }
                Redirect::
                Redirect::Output(path) =>Output(path) => {
                    let {
                    let tokens: Vec<Token tokens: Vec<Token> = Lexer::> = Lexer::new(path).collectnew(path).collect();
                    let();
                    let p = expander p = expander.expand_tokens(tokens)?..expand_tokens(tokens)?.into_iter().nextinto_iter().next().unwrap_or_default().unwrap_or_default();
                    expanded();
                    expanded.redirects.push(R.redirects.push(Redirect::Outputedirect::Output(p));
               (p));
                }
                Redirect }
                Redirect::Append(path)::Append(path) => {
                    => {
                    let tokens: Vec let tokens: Vec<Token> = Lexer<Token> = Lexer::new(path).::new(path).collect();
                   collect();
                    let p = exp let p = expander.expand_tokens(tokensander.expand_tokens(tokens)?.into_iter().)?.into_iter().next().unwrap_ornext().unwrap_or_default();
                   _default();
                    expanded.redirects.push expanded.redirects.push(Redirect::(Redirect::Append(p));
Append(p));
                }
                           }
            }
        } }
        }

        Ok(exp

        Ok(expanded)
   anded)
    }
}
 }
}
