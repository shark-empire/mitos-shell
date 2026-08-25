use crate::builtins;
use crate::parser::{self, Node};
use crate::process;

use std::env;
use std::path::PathBuf;
use rustyline::DefaultEditor;

pub struct Shell { last_status: i32 }

impl Shell {
    pub fn new() -> Self { Self { last_status: 0 } }

    pub fn run(&mut self) -> i32 {
        let mut rl = DefaultEditor::new().expect("Failed to init rustyline");
        let history_file = dirs::home_dir().map(|p| p.join(".mitos_history"));
        if let Some(path) = &history_file { let _ = rl.load_history(path); }

        loop {
            let prompt = self.get_prompt();
            match rl.readline(&prompt) {
                Ok(line) => {
                    let line = line.trim().to_string();
                    if line.is_empty() { continue; }
                    let _ = rl.add_history_entry(&line);
                    
                    match parser::parse(&line) {
                        Ok(ast) => { self.last_status = self.execute_node(&ast); }
                        Err(e) => eprintln!("mitos: {}", e),
                    }
                }
                Err(rustyline::error::ReadlineError::Interrupted) => println!("^C"),
                Err(rustyline::error::ReadlineError::Eof) => break,
                Err(err) => { eprintln!("mitos: input error: {:?}", err); break; }
            }
        }

        if let Some(path) = history_file { let _ = rl.save_history(path); }
        self.last_status
    }

    fn get_prompt(&self) -> String {
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let home = env::var("HOME").unwrap_or_default();
        let cwd_str = cwd.to_string_lossy();
        let display_path = if !home.is_empty() && cwd_str.starts_with(&home) {
            format!("~{}", &cwd_str[home.len()..])
        } else { cwd_str.into_owned() };

        format!("\x1b[1;34mMITOS\x1b[0m \x1b[1;32m{}\x1b[0m > ", display_path)
    }

    fn execute_node(&mut self, node: &Node) -> i32 {
        match node {
            Node::Pipeline(p) => self.execute_pipeline(p),
            Node::And(l, r) => {
                let s = self.execute_node(l);
                if s == 0 { self.execute_node(r) } else { s }
            }
            Node::Or(l, r) => {
                let s = self.execute_node(l);
                if s != 0 { self.execute_node(r) } else { s }
            }
            Node::Seq(l, r) => { let _ = self.execute_node(l); self.execute_node(r) }
        }
    }

    fn execute_pipeline(&mut self, p: &parser::Pipeline) -> i32 {
        let mut final_status = 0;
        
        if p.commands.len() == 1 {
            let cmd = &p.commands[0];
            let expanded_args = self.expand_args(&cmd.args);
            if expanded_args.is_empty() { return 0; }
            
            let program = &expanded_args[0];
            let args = &expanded_args[1..];

            match builtins::execute(program, args, self.last_status) {
                builtins::BuiltinResult::Continue(status) => final_status = status,
                builtins::BuiltinResult::Exit(status) => return status, 
                builtins::BuiltinResult::NotBuiltin => {
                    final_status = process::execute_with_redirs(program, args, &cmd.redirs, p.background);
                }
            }
        } else {
            final_status = process::execute_pipeline(&p.commands, p.background);
        }

        if !p.background {
            self.last_status = final_status;
        } else {
            println!("[{}] {} &", 1, p.commands[0].args.join(" "));
            final_status = 0;
        }
        final_status
    }

    fn expand_variables(&self, input: &str) -> String {
        let mut result = String::new();
        let mut chars = input.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch != '$' { result.push(ch); continue; }
            if chars.peek() == Some(&'?') { chars.next(); result.push_str(&self.last_status.to_string()); continue; }
            let mut name = String::new();
            while let Some(&next) = chars.peek() {
                if next.is_alphanumeric() || next == '_' { name.push(next); chars.next(); }
                else { break; }
            }
            if name.is_empty() { result.push('$'); continue; }
            if let Ok(value) = env::var(&name) { result.push_str(&value); }
        }
        result
    }

    fn expand_args(&self, args: &[String]) -> Vec<String> {
        let mut result = Vec::new();
        for arg in args {
            let expanded = self.expand_variables(arg);
            if expanded.contains('*') || expanded.contains('?') || expanded.contains('[') {
                if let Ok(paths) = glob::glob(&expanded) {
                    let mut matched = false;
                    for path in paths.flatten() {
                        result.push(path.to_string_lossy().into_owned());
                        matched = true;
                    }
                    if !matched { result.push(expanded); }
                } else { result.push(expanded); }
            } else { result.push(expanded); }
        }
        result
    }
}
