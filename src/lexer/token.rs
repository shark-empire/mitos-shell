#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Word(String),
    Pipe,           // |
    And,            // &&
    Or,             // ||
    Semicolon,      // ;
    Background,     // &
    RedirectIn,     // <
    RedirectOut,    // >
    AppendOut,      // >>
    Eof,
}
