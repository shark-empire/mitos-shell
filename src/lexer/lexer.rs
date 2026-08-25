use super::token::Token;

pub struct Lexer {
    input: Vec<char>,
    pos: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.peek();
        self.pos += 1;
        ch
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }
}

impl Iterator for Lexer {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        self.skip_whitespace();
        let ch = self.advance()?;

        match ch {
            '|' => {
                if self.peek() == Some('|') { self.advance(); Some(Token::Or) } 
                else { Some(Token::Pipe) }
            }
            '&' => {
                if self.peek() == Some('&') { self.advance(); Some(Token::And) } 
                else { Some(Token::Background) }
            }
            ';' => Some(Token::Semicolon),
            '<' => Some(Token::RedirectIn),
            '>' => {
                if self.peek() == Some('>') { self.advance(); Some(Token::AppendOut) } 
                else { Some(Token::RedirectOut) }
            }
            '\'' | '"' => {
                let quote = ch;
                let mut word = String::new();
                while let Some(c) = self.advance() {
                    if c == quote { break; }
                    if c == '\\' && quote == '"' {
                        if let Some(escaped) = self.advance() { word.push(escaped); }
                    } else {
                        word.push(c);
                    }
                }
                Some(Token::Word(word))
            }
            _ => {
                let mut word = String::new();
                word.push(ch);
                while let Some(c) = self.peek() {
                    if c.is_whitespace() || "|&;<>()".contains(c) { break; }
                    word.push(c);
                    self.advance();
                }
                Some(Token::Word(word))
            }
        }
    }
}
