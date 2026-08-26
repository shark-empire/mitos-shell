#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Word(String),
    SingleQuoted(String),
    DoubleQuoted(String),
    Pipe,           // |
    And,            // &&
    Or,             // ||
    Semicolon,      // ;
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
    HereDocStart(bool),
    HereString,     // <<<
    ArrayAssign(String),
    Eof,
}
