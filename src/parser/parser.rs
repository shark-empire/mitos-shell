use super::ast::*;
use crate::error::{Result, ShellError};
use crate::lexer::token::Token;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    // ---------- token helpers ----------
    fn peek(&self) -> Option<&Token> { self.tokens.get(self.pos) }

    fn advance(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn check_token(&self, expected: &Token) -> bool {
        matches!(self.peek(), Some(t) if std::mem::discriminant(t) == std::mem::discriminant(expected))
    }

    fn check_word(&self, word: &str) -> bool {
        matches!(self.peek(), Some(Token::Word(w)) if w == word)
    }

    fn expect_token(&mut self, expected: Token) -> Result<()> {
        if self.check_token(&expected) { self.advance(); Ok(()) }
        else { Err(ShellError::Syntax(format!("expected {:?}", expected))) }
    }

    fn expect_word(&mut self, word: &str) -> Result<()> {
        match self.advance() {
            Some(Token::Word(w)) if w == word => Ok(()),
            other => Err(ShellError::Syntax(format!("expected '{}', found {:?}", word, other))),
        }
    }

    fn expect_any_word(&mut self) -> Result<String> {
        match self.advance() {
            Some(Token::Word(w)) => Ok(w),
            other => Err(ShellError::Syntax(format!("expected a word, found {:?}", other))),
        }
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek(), Some(Token::Newline)) { self.advance(); }
    }

    fn is_list_terminator(&self) -> bool {
        match self.peek() {
            None => true,
            Some(Token::RightParen) | Some(Token::RightBrace) => true,
            Some(Token::Word(w)) => matches!(w.as_str(), "then" | "else" | "elif" | "fi" | "do" | "done"),
            _ => false,
        }
    }

    fn is_compound_start(&self) -> bool {
        match self.peek() {
            Some(Token::LeftParen) | Some(Token::LeftBrace) => true,
            Some(Token::Word(w)) => matches!(w.as_str(), "if" | "while" | "for"),
            _ => false,
        }
    }

    // ---------- entry ----------
    pub fn parse(&mut self) -> Result<Node> {
        self.skip_newlines();
        let node = self.parse_list()?;
        self.skip_newlines();
        if let Some(extra) = self.peek() {
            return Err(ShellError::Syntax(format!("unexpected token {:?}", extra)));
        }
        Ok(node)
    }

    // list := and_or ( (';' | '&' | NEWLINE) and_or )*
    fn parse_list(&mut self) -> Result<Node> {
        let mut left = self.parse_and_or()?;
        loop {
            match self.peek() {
                Some(Token::Semicolon) | Some(Token::Newline) => {
                    self.advance();
                    self.skip_newlines();
                    if self.is_list_terminator() { return Ok(left); }
                    let right = self.parse_and_or()?;
                    left = Node::Sequence(Box::new(left), Box::new(right));
                }
                Some(Token::Background) => {
                    self.advance();
                    left = Node::Background(Box::new(left));
                    self.skip_newlines();
                    if self.is_list_terminator() { return Ok(left); }
                    let right = self.parse_and_or()?;
                    left = Node::Sequence(Box::new(left), Box::new(right));
                }
                _ => return Ok(left),
            }
        }
    }

    // and_or := pipeline ( ('&&' | '||') pipeline )*
    fn parse_and_or(&mut self) -> Result<Node> {
        let mut left = self.parse_pipeline()?;
        loop {
            let op = match self.peek() {
                Some(Token::And) => ListOp::And,
                Some(Token::Or) => ListOp::Or,
                _ => return Ok(left),
            };
            self.advance();
            self.skip_newlines();
            let right = self.parse_pipeline()?;
            left = Node::AndOr(Box::new(left), op, Box::new(right));
        }
    }

    // pipeline := ['!'] command ( '|' command )*
    fn parse_pipeline(&mut self) -> Result<Node> {
        let negated = if matches!(self.peek(), Some(Token::Bang)) { self.advance(); true } else { false };

        let first = self.parse_command()?;

        if matches!(self.peek(), Some(Token::Pipe)) {
            // Multi-command pipeline: elements must be simple commands.
            let mut commands = match first {
                Node::Pipeline(p) if p.commands.len() == 1 && !p.negated => p.commands,
                _ => return Err(ShellError::Syntax("cannot pipe a compound command".into())),
            };
            while matches!(self.peek(), Some(Token::Pipe)) {
                self.advance();
                self.skip_newlines();
                commands.push(self.parse_simple_command()?);
            }
            return Ok(Node::Pipeline(Pipeline { commands, negated }));
        }

        if negated {
            if let Node::Pipeline(mut p) = first {
                p.negated = true;
                return Ok(Node::Pipeline(p));
            }
        }
        Ok(first)
    }

    // command := function_def | compound | simple_command
    fn parse_command(&mut self) -> Result<Node> {
        if self.is_compound_start() {
            return self.parse_compound();
        }

        // function definition: NAME '(' ')' compound
        if let Some(Token::Word(name)) = self.peek().cloned() {
            let is_func = matches!(self.tokens.get(self.pos + 1), Some(Token::LeftParen))
                && matches!(self.tokens.get(self.pos + 2), Some(Token::RightParen));
            if is_func {
                self.advance(); self.advance(); self.advance();
                let body = self.parse_compound()?;
                return Ok(Node::Function(FunctionDef { name, body: Box::new(body) }));
            }
        }

        let cmd = self.parse_simple_command()?;
        Ok(Node::Pipeline(Pipeline { commands: vec![cmd], negated: false }))
    }

    fn parse_compound(&mut self) -> Result<Node> {
        match self.peek() {
            Some(Token::LeftParen) => {
                self.advance();
                self.skip_newlines();
                let inner = self.parse_list()?;
                self.skip_newlines();
                self.expect_token(Token::RightParen)?;
                Ok(Node::Subshell(Box::new(inner)))
            }
            Some(Token::LeftBrace) => {
                self.advance();
                self.skip_newlines();
                let inner = self.parse_list()?;
                self.skip_newlines();
                self.expect_token(Token::RightBrace)?;
                Ok(Node::BraceGroup(Box::new(inner)))
            }
            Some(Token::Word(w)) if w == "case" => self.parse_case(),
            Some(Token::Word(w)) if w == "if" => self.parse_if(),
            Some(Token::Word(w)) if w == "while" => self.parse_while(),
            Some(Token::Word(w)) if w == "for" => self.parse_for(),
            _ => Err(ShellError::Syntax("expected a compound command".into())),
        }
    }

    fn parse_if(&mut self) -> Result<Node> {
        self.expect_word("if")?;
        let condition = self.parse_list()?;
        self.expect_word("then")?;
        let then_branch = self.parse_list()?;

        let else_branch = if self.check_word("else") {
            self.advance();
            Some(Box::new(self.parse_list()?))
        } else {
            None
        };

        self.expect_word("fi")?;
        Ok(Node::If(IfClause {
            condition: Box::new(condition),
            then_branch: Box::new(then_branch),
            else_branch,
        }))
    }

    fn parse_while(&mut self) -> Result<Node> {
        self.expect_word("while")?;
        let condition = self.parse_list()?;
        self.expect_word("do")?;
        let body = self.parse_list()?;
        self.expect_word("done")?;
        Ok(Node::While(WhileClause {
            condition: Box::new(condition),
            body: Box::new(body),
        }))
    }

    fn parse_for(&mut self) -> Result<Node> {
        self.expect_word("for")?;
        let var = self.expect_any_word()?;

        let mut words = Vec::new();
        if self.check_word("in") {
            self.advance();
            while let Some(Token::Word(w)) = self.peek() {
                if w == "do" { break; }
                words.push(w.clone());
                self.advance();
            }
        }

        if matches!(self.peek(), Some(Token::Semicolon) | Some(Token::Newline)) {
            self.advance();
            self.skip_newlines();
        }

        self.expect_word("do")?;
        let body = self.parse_list()?;
        self.expect_word("done")?;
        Ok(Node::For(ForClause { var, words, body: Box::new(body) }))
    }

    fn parse_simple_command(&mut self) -> Result<SimpleCommand> {
        let mut assignments = Vec::new();
        let mut args = Vec::new();
        let mut redirects = Vec::new();

        loop {
            match self.peek() {
                Some(Token::Word(w)) => {
                    // VAR=value allowed as a prefix before the first real argument.
                    if args.is_empty() && is_assignment(w) {
                        let (k, v) = w.split_once('=').unwrap();
                        assignments.push((k.to_string(), v.to_string()));
                        self.advance();
                        continue;
                    }
                    args.push(w.clone());
                    self.advance();
                }
                Some(Token::RedirectIn) => {
                    self.advance();
                    redirects.push(Redirect::Input(self.expect_any_word()?));
                }
                Some(Token::RedirectOut) => {
                    self.advance();
                    redirects.push(Redirect::Output(self.expect_any_word()?));
                }
                Some(Token::AppendOut) => {
                    self.advance();
                    redirects.push(Redirect::Append(self.expect_any_word()?));
                }
                _ => break,
            }
        }

        if args.is_empty() && assignments.is_empty() && redirects.is_empty() {
            return Err(ShellError::Syntax("expected a command".into()));
        }
        Ok(SimpleCommand { assignments, args, redirects })
    }
}

fn is_assignment(word: &str) -> bool {
    if let Some((name, _)) = word.split_once('=') {
        !name.is_empty()
            && name.chars().next().map(|c| c.is_alphabetic() || c == '_').unwrap_or(false)
            && name.chars().all(|c| c.is_alphanumeric() || c == '_')
    } else {
        false
    }
}

fn parse_case(&mut self) -> Result<Node> {
    self.expect_word("case")?;
    let word = self.expect_any_word()?;
    self.expect_word("in")?;
    self.skip_newlines();

    let mut branches = Vec::new();

    while !self.check_word("esac") {
        let mut patterns = Vec::new();
        
        // Parse patterns (e.g., start|stop)
        loop {
            patterns.push(self.expect_any_word()?);
            if matches!(self.peek(), Some(Token::Word(w)) if w == "|") {
                self.advance();
            } else {
                break;
            }
        }

        // Expect ')' after patterns
        if !matches!(self.peek(), Some(Token::RightParen)) {
            return Err(ShellError::Syntax("expected ')' after case patterns".into()));
        }
        self.advance();

        let body = self.parse_list()?;
        branches.push(CaseBranch { patterns, body });

        // Expect ';;' or newline/esac
        if matches!(self.peek(), Some(Token::Semicolon)) {
            self.advance();
            if matches!(self.peek(), Some(Token::Semicolon)) { self.advance(); }
        }
        self.skip_newlines();
    }

    self.expect_word("esac")?;
    Ok(Node::Case(CaseClause { word, branches }))
}

