use super::ast::{Node, Command, Redirect};
use crate::lexer::token::Token;
use crate::error::{ShellError, Result};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn advance(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.pos).cloned();
        self.pos += 1;
        token
    }

    pub fn parse(&mut self) -> Result<Node> {
        let mut commands = Vec::new();
        let mut current_cmd = Command {
            args: Vec::new(),
            redirects: Vec::new(),
            background: false,
        };

        while let Some(token) = self.advance() {
            match token {
                Token::Word(w) => current_cmd.args.push(w),
                Token::RedirectIn => {
                    if let Some(Token::Word(file)) = self.advance() {
                        current_cmd.redirects.push(Redirect::Input(file));
                    } else { return Err(ShellError::Syntax("Expected file after '<'".into())); }
                }
                Token::RedirectOut => {
                    if let Some(Token::Word(file)) = self.advance() {
                        current_cmd.redirects.push(Redirect::Output(file));
                    } else { return Err(ShellError::Syntax("Expected file after '>'".into())); }
                }
                Token::AppendOut => {
                    if let Some(Token::Word(file)) = self.advance() {
                        current_cmd.redirects.push(Redirect::Append(file));
                    } else { return Err(ShellError::Syntax("Expected file after '>>'".into())); }
                }
                Token::Background => current_cmd.background = true,
                Token::Pipe => {
                    if current_cmd.args.is_empty() {
                        return Err(ShellError::Syntax("Empty command in pipeline".into()));
                    }
                    commands.push(current_cmd);
                    current_cmd = Command { args: Vec::new(), redirects: Vec::new(), background: false };
                }
                Token::Semicolon | Token::And | Token::Or => break, // Handled in higher-level List parsing
                _ => {}
            }
        }

        if !current_cmd.args.is_empty() { commands.push(current_cmd); }
        if commands.is_empty() { return Err(ShellError::Syntax("Empty command".into())); }

        Ok(Node::Pipeline(commands))
    }
}
