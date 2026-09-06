pub mod token;

use crate::lexer::token::Token;

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

    fn skip_spaces(&mut self) {
        // Skip spaces and tabs only — newlines are meaningful tokens.
        while let Some(ch) = self.peek() {
            if ch == ' ' || ch == '\t' {
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
        self.skip_spaces();
        let ch = self.advance()?;

        match ch {
            '\n' => Some(Token::Newline),
            '|' => {
                if self.peek() == Some('|') {
                    self.advance();
                    Some(Token::Or)
                } else {
                    Some(Token::Pipe)
                }
            }
            '&' => {
                if self.peek() == Some('&') {
                    self.advance();
                    Some(Token::And)
                } else {
                    Some(Token::Background)
                }
            }
            ';' => Some(Token::Semicolon),
            '<' => {
                if self.peek() == Some('<') {
                    self.advance();
                    if self.peek() == Some('<') {
                        self.advance();
                        Some(Token::HereString) // <<<
                    } else {
                        let strip_tabs = if self.peek() == Some('-') {
                            self.advance();
                            true
                        } else {
                            false
                        };
                        // We emit a start token; the Parser/Executor will handle reading the body
                        // to keep the Lexer simple and stateless across lines.
                        Some(Token::HereDocStart(strip_tabs))
                    }
                } else {
                    Some(Token::RedirectIn)
                }
            }
            '>' => {
                if self.peek() == Some('>') {
                    self.advance();
                    Some(Token::AppendOut)
                } else {
                    Some(Token::RedirectOut)
                }
            }

            '\'' => {
                let mut word = String::new();
                while let Some(c) = self.advance() {
                    if c == '\'' {
                        break;
                    }
                    word.push(c);
                }
                Some(Token::SingleQuoted(word))
            }
            '"' => {
                let mut word = String::new();
                while let Some(c) = self.advance() {
                    if c == '"' {
                        break;
                    }
                    if c == '\\' {
                        if let Some(escaped) = self.advance() {
                            word.push(escaped);
                        }
                    } else {
                        word.push(c);
                    }
                }
                Some(Token::DoubleQuoted(word))
            }

            '(' => Some(Token::LeftParen),
            ')' => Some(Token::RightParen),
            '{' => Some(Token::LeftBrace),
            '}' => Some(Token::RightBrace),
            '!' => Some(Token::Bang),

            // Handle Array Assignments: If a word ends with '=', and the next char is '(', it's an array.
            // We handle this by checking if the word ends with '=' and peeking ahead.
            _ => {
                let mut word = String::new();
                word.push(ch);

                // Track quote state while accumulating so that whitespace
                // and operator characters *inside* a quoted region (e.g.
                // the space in `FOO="bar baz"`, or the `;` in `echo
                // "a;b"`) don't end the word early — only a matching
                // close-quote does. A quote that starts the word itself
                // never reaches this branch (see the '\'' and '"' arms
                // above); this only handles one appearing after some
                // other character has already begun the word. Escapes
                // and quote-removal aren't resolved here — the raw text
                // (backslashes and quote marks included) is kept as-is
                // and handled later during expansion, so re-lexing this
                // token's text on the second pass reproduces the same
                // quote regions.
                let mut in_single = false;
                let mut in_double = false;

                while let Some(c) = self.peek() {
                    if in_single {
                        word.push(c);
                        self.advance();
                        if c == '\'' {
                            in_single = false;
                        }
                    } else if in_double {
                        word.push(c);
                        self.advance();
                        if c == '\\' {
                            if let Some(escaped) = self.peek() {
                                word.push(escaped);
                                self.advance();
                            }
                        } else if c == '"' {
                            in_double = false;
                        }
                    } else {
                        if c.is_whitespace() || "|&;<>(){}!\n".contains(c) {
                            break;
                        }
                        word.push(c);
                        self.advance();
                        if c == '\'' {
                            in_single = true;
                        } else if c == '"' {
                            in_double = true;
                        }
                    }
                }

                // Array detection: if word ends with '=' and next non-whitespace is '('
                if word.ends_with('=') {
                    let mut temp_pos = self.pos;
                    while temp_pos < self.input.len() && self.input[temp_pos].is_whitespace() {
                        temp_pos += 1;
                    }
                    if temp_pos < self.input.len() && self.input[temp_pos] == '(' {
                        return Some(Token::ArrayAssign(word.trim_end_matches('=').to_string()));
                    }
                }

                Some(Token::Word(word))
            }
        }
    }
}

/// Tokenizes `input` and appends a trailing [`Token::Eof`] sentinel. The
/// live parser/executor pipeline uses plain iterator exhaustion (`None`) to
/// detect end-of-input, but callers that want an explicit marker in the
/// token sequence itself — such as the interactive completeness heuristic,
/// which just scans for balanced keywords/braces and ignores tokens it
/// doesn't recognize — can use this instead of `Lexer::new(..).collect()`.
pub fn tokenize_with_eof(input: &str) -> Vec<Token> {
    let mut tokens: Vec<Token> = Lexer::new(input).collect();
    tokens.push(Token::Eof);
    tokens
}
