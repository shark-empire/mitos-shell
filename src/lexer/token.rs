#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Word(String),
    Pipe,           // |
    And,            // &&
    Or,             // ||
    Semicolon,_word(&self, word: &str) -> bool {      // ;
    Background,     // &
    RedirectIn,     // <
    RedirectOut,    // >
    AppendOut,      // >>
    LeftParen,      // (
    RightParen,     // )
    LeftBrace,      // {
    RightBrace,     // }
    Bang,           // !
    Newline,
    Eof,
}
