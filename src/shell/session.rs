use rustyline::DefaultEditor;
use crate::execution::executor::Executor;
use crate::parser::parser::Parser;
use crate::lexer::lexer::Lexer;
use crate::expansion::pipeline::ExpansionPipeline;
// src/shell/session.rs
use rustyline::{Editor, Config};
use crate::completion::engine::MitosCompleter;

pub struct Session {
    rl: Editor<MitosCompleter>,
    executor: Executor,
}

impl Session {
    pub fn init() -> rustyline::Result<Self> {
        let config = Config::builder()
            .history_ignore_space(true)
            .auto_add_history(true)
            .build();

        let mut rl = Editor::with_config(config)?;
        rl.set_helper(Some(MitosCompleter));

        // Load persistent history
        let _ = rl.load_history(&history_path());

        Ok(Self { rl, executor: Executor::new() })
    }

    pub fn run(&mut self) {
        loop {
            let prompt = build_prompt();
            match self.rl.readline(&prompt) {
                Ok(line) => {
                    if line.trim().is_empty() { continue; }
                    self.execute_line(&line);
                }
                Err(rustyline::error::ReadlineError::Interrupted) => {
                    println!("^C");
                    continue; // Ctrl+C on empty line shouldn't exit
                }
                Err(rustyline::error::ReadlineError::Eof) => {
                    println!("exit");
                    break;
                }
                Err(err) => {
                    eprintln!("Error: {:?}", err);
                    break;
                }
            }
        }

        // Save history on exit
        let _ = self.rl.save_history(&history_path());
    }

    fn execute_line(&mut self, line: &str) {
        use crate::expansion::pipeline::ExpansionPipeline;
        use crate::lexer::lexer::Lexer;
        use crate::parser::parser::Parser;

        let pipeline = ExpansionPipeline::new(self.executor.last_status());
        let expanded = pipeline.expand_line(line);

        let lexer = Lexer::new(&expanded);
        let tokens: Vec<_> = lexer.collect();

        match Parser::new(tokens).parse() {
            Ok(ast) => {
                if let Err(e) = self.executor.execute(ast) {
                    eprintln!("mitos: {}", e);
                }
            }
            Err(e) => eprintln!("mitos: syntax error: {}", e),
        }
    }
}

fn history_path() -> std::path::PathBuf {
    let mut path = dirs::home_dir().unwrap_or_default();
    path.push(".mitos_history");
    path
}

fn build_prompt() -> String {
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "?".to_string());
    // ANSI: bold blue "MITOS", green cwd, reset
    format!("\x1b[1;34mMITOS\x1b[0m \x1b[32m{}\x1b[0m \x1b[1m❯\x1b[0m ", cwd)
}
