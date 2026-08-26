use crate::builtins::alias;
use crate::completion::helper::MitosHelper;
use crate::config::startup;
use crate::execution::executor::Executor;
use crate::execution::outcome::ExecOutcome;
use crate::lexer::lexer::Lexer;
use crate::parser::parser::Parser;
use rustyline::Editor;

pub struct Session {
    rl: Editor<MitosHelper>,
    executor: Executor,
}

impl Session {
    pub fn init() -> rustyline::Result<Self> {
        let mut executor = Executor::new();

        // Load startup files unless disabled.
        if std::env::var("MITOS_NO_RC").is_err() {
            load_startup_files(&mut executor);
        }

        let mut rl =
            Editor::with_config(rustyline::Config::builder().auto_add_history(true).build())?;

        rl.set_helper(Some(MitosHelper));

        let _ = rl.load_history(&history_path());

        Ok(Self { rl, executor })
    }

    pub fn run(&mut self) -> i32 {
        let mut exit_code = 0;

        loop {
            let prompt = build_prompt();

            match self.rl.readline(&prompt) {
                Ok(line) => {
                    if line.trim().is_empty() {
                        continue;
                    }

                    if let Some(code) = self.execute_line(&line) {
                        exit_code = code;
                        break;
                    }
                }

                Err(rustyline::error::ReadlineError::Interrupted) => {
                    println!("^C");
                    continue;
                }

                Err(rustyline::error::ReadlineError::Eof) => {
                    println!("exit");
                    break;
                }

                Err(error) => {
                    eprintln!("mitos: {:?}", error);
                    break;
                }
            }
        }

        let _ = self.rl.save_history(&history_path());

        exit_code
    }

    fn execute_line(&mut self, line: &str) -> Option<i32> {
        // Expand aliases before parsing.
        let expanded = alias::expand(line);

        let tokens: Vec<_> = Lexer::new(&expanded).collect();

        match Parser::new(tokens).parse() {
            Ok(ast) => match self.executor.execute(ast) {
                Ok(Some(exit_code)) => return Some(exit_code),
                Ok(None) => {}
                Err(error) => eprintln!("mitos: {}", error),
            },

            Err(error) => {
                eprintln!("mitos: syntax error: {}", error);
            }
        }

        None
    }
}

fn load_startup_files(executor: &mut Executor) {
    for path in startup::files() {
        if !path.exists() {
            continue;
        }

        let path_string = path.to_string_lossy();

        match executor.source_file(&path_string, &[]) {
            Ok(ExecOutcome::Exit(code)) => {
                std::process::exit(code);
            }

            Ok(_) => {}

            Err(error) => {
                eprintln!("mitos: {}: {}", path_string, error);
            }
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
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "?".to_string());

    format!(
        "\x1b[1;34mMITOS\x1b[0m \x1b[32m{}\x1b[0m \x1b[1m❯\x1b[0m ",
        cwd
    )
}
