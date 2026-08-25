#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Word(String),
    Pipe,       // |
    And,        // &&
    Or,         // ||
    Semi,       // ;
    Amp,        // &
    Redir(RedirOp),
}

#[derive(Debug, Clone, PartialEq)]
pub enum RedirOp {
    In,       // <
    Out,      // >
    Append,   // >>
    ErrOut,   // 2>
    ErrApp,   // 2>>
    AllOut,   // &>
    AllApp,   // &>>
}

#[derive(Debug, Clone)]
pub enum Node {
    Pipeline(Pipeline),
    And(Box<Node>, Box<Node>),
    Or(Box<Node>, Box<Node>),
    Seq(Box<Node>, Box<Node>),
}

#[derive(Debug, Clone)]
pub struct Pipeline {
    pub commands: Vec<Command>,
    pub background: bool,
}

#[derive(Debug, Clone)]
pub struct Command {
    pub args: Vec<String>,
    pub redirs: Vec<(RedirOp, String)>,
}

pub fn parse(input: &str) -> Result<Node, String> {
    let tokens = tokenize(input)?;
    if tokens.is_empty() { return Err("empty input".to_string()); }
    let mut parser = Parser { tokens, pos: 0 };
    parser.parse_expr()
}

fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    let mut current_word = String::new();
    
    let mut push_word = |w: &mut String, t: &mut Vec<Token>| {
        if !w.is_empty() { t.push(Token::Word(std::mem::take(w))); }
    };

    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' => push_word(&mut current_word, &mut tokens),
            '\'' | '"' => {
                let quote = c;
                let mut escaped = false;
                while let Some(next) = chars.next() {
                    if escaped { current_word.push(next); escaped = false; }
                    else if next == '\\' { escaped = true; }
                    else if next == quote { break; }
                    else { current_word.push(next); }
                }
            }
            '\\' => if let Some(esc) = chars.next() { current_word.push(esc); },
            '|' => {
                push_word(&mut current_word, &mut tokens);
                if chars.peek() == Some(&'|') { chars.next(); tokens.push(Token::Or); }
                else { tokens.push(Token::Pipe); }
            }
            '&' => {
                push_word(&mut current_word, &mut tokens);
                if chars.peek() == Some(&'&') { chars.next(); tokens.push(Token::And); }
                else if chars.peek() == Some(&'>') {
                    chars.next();
                    if chars.peek() == Some(&'>') { chars.next(); tokens.push(Token::Redir(RedirOp::AllApp)); }
                    else { tokens.push(Token::Redir(RedirOp::AllOut)); }
                } else { tokens.push(Token::Amp); }
            }
            ';' => { push_word(&mut current_word, &mut tokens); tokens.push(Token::Semi); }
            '<' => { push_word(&mut current_word, &mut tokens); tokens.push(Token::Redir(RedirOp::In)); }
            '>' => {
                push_word(&mut current_word, &mut tokens);
                if chars.peek() == Some(&'>') { chars.next(); tokens.push(Token::Redir(RedirOp::Append)); }
                else { tokens.push(Token::Redir(RedirOp::Out)); }
            }
            '0'..='9' if current_word.is_empty() && chars.peek() == Some(&'>') => {
                chars.next(); // consume '>'
                push_word(&mut current_word, &mut tokens);
                if chars.peek() == Some(&'>') { chars.next(); tokens.push(Token::Redir(RedirOp::ErrApp)); }
                else { tokens.push(Token::Redir(RedirOp::ErrOut)); }
            }
            _ => current_word.push(c),
        }
    }
    push_word(&mut current_word, &mut tokens);
    Ok(tokens)
}

struct Parser { tokens: Vec<Token>, pos: usize }
impl Parser {
    fn peek(&self) -> Option<&Token> { self.tokens.get(self.pos) }
    fn next(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos).cloned();
        self.pos += 1;
        t
    }
    fn parse_expr(&mut self) -> Result<Node, String> {
        let mut left = self.parse_pipeline()?;
        loop {
            match self.peek() {
                Some(Token::And) => { self.next(); left = Node::And(Box::new(left), Box::new(self.parse_pipeline()?)); }
                Some(Token::Or) => { self.next(); left = Node::Or(Box::new(left), Box::new(self.parse_pipeline()?)); }
                Some(Token::Semi) => {
                    self.next();
                    if self.pos >= self.tokens.len() { break; }
                    left = Node::Seq(Box::new(left), Box::new(self.parse_pipeline()?));
                }
                _ => break,
            }
        }
        Ok(left)
    }
    fn parse_pipeline(&mut self) -> Result<Node, String> {
        let mut commands = vec![self.parse_command()?];
        while let Some(Token::Pipe) = self.peek() { self.next(); commands.push(self.parse_command()?); }
        let mut background = false;
        if let Some(Token::Amp) = self.peek() { self.next(); background = true; }
        Ok(Node::Pipeline(Pipeline { commands, background }))
    }
    fn parse_command(&mut self) -> Result<Command, String> {
        let mut args = Vec::new();
        let mut redirs = Vec::new();
        while let Some(token) = self.peek() {
            match token {
                Token::Word(w) => { args.push(w.clone()); self.next(); }
                Token::Redir(op) => {
                    let op = op.clone(); self.next();
                    if let Some(Token::Word(file)) = self.next() { redirs.push((op, file)); }
                    else { return Err("Expected filename after redirection".to_string()); }
                }
                _ => break,
            }
        }
        if args.is_empty() && redirs.is_empty() { return Err("Empty command".to_string()); }
        Ok(Command { args, redirs })
    }
}
