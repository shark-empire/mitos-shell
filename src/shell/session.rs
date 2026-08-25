use rustyline::DefaultEditor;
use crate::execution::executor::Executor;
use crate::parser::parser::Parser;
use crate::lexer::lexer::Lexer;

pub struct Session {
    rl: DefaultEditor,
    executor: Executor,
}

impl Session {
    pub fn init() -> rustyline::Result<Self> {
        let mut rl = DefaultEditor::new()?;
        // Load history from ~/.mitos_history here if desired
        Ok(Self {
            rl,
            executor: Executor::new(),
        })
    }

    pub fn run(&mut self) {
        loop {
            let prompt = format!("\x1b[1;34mMITOS\x1b[0m \x1b[1;32m{}\x1b[0m > ", 
                std::env::current_dir().unwrap_or_default().display());
                
            match self.rl.readline(&prompt) {
                Ok(line) => {
                    let _ = self.rl.add_history_entry(line.as_str());
                    
                    let lexer = Lexer::new(&line);
                    let tokens: Vec<_> = lexer.collect();
                    
                    match Parser::new(tokens).parse() {
                        Ok(ast) => {
                            if let Err(e) = self.executor.execute(ast) {
                                eprintln!("\x1b[31mmitos: {}\x1b[0m", e);
                            }
                        }
                        Err(e) => eprintln!("\x1b[31mmitos: syntax error: {}\x1b[0m", e),
                    }
                }
                Err(_) => {
                    println!("exit");
                    break;
                }
            }
        }
    }
}
