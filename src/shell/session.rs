use crate::completion::helper::MitosHelper;
use crate::execution::executor::Executor;
use crate::lexer::lexer::Lexer;
use crate::parser::parser::Parser;
use rustyline::Editor;

pub struct Session {
    rl: Editor<MitosHelper>,
    executor: Executor,
}

impl Session {
    pub fn init() -> rustyline::Result<Self> {
        let mut rl = Editor::with_config(rustyline::Config::builder().auto_add_history(true).build())?;
        rl.set_helper(Some(MitosHelper));
        let _ = rl.load_history(&history_path());
        Ok(Self { rl, executor: Executor::new() })
    }

    pub fn run(&mut self) -> i32 {
        let mut exit_code = 0;
        loop {
            let prompt = build_prompt();
            match self.rl.readline(&prompt) {
                Ok(line) => {
                    if line.trim().is_empty() { continue; }
                    if let Some(code) = self.execute_line(&line) {
                        exit_code = code;
                        break;
                    }
                }
                Err(rustyline::error::ReadlineError::Interrupted) => { println!("^C"); continue; }
                Err(rustyline::error::ReadlineError::Eof) => { println!("exit"); break; }
                Err(e) => { eprintln!("mitos: {:?}", e); break; }
            }
        }
        let _ = self.rl.save_history(&history_path());
        exit_code
    }

    /// Returns Some(code) when the shell should exit.
    fn execute_line(&mut self, line: &str) -> Option<i32> {
        let tokens: Vec<_> = Lexer::new(line).collect();
        match Parser::new(tokens).parse() {
            Ok(ast) => match self.executor.execute(ast) {
                Ok(maybe_exit) => return maybe_exit,
                Err(e) => eprintln!("mitos: {}", e),
            },
            Err(e) => eprintln!("mitos: syntax error: {}", e),
        }
        None
    }
}

fn history_path() -> std::path::PathBuf {
    let mut p = dirs::home_dir().unwrap_or_default();
    p.push(".mitos_history");
    p
}

fn build_prompt() -> String {
    let cwd = std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_else(|_| "?".into());
    format!("\x1b[1;34mMITOS\x1b[0m \x1b[32m{}\x1b[0m \x1b[1m❯\x1b[0m ", cwd)
}
