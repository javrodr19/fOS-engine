//! Token Types
//!
//! JavaScript token definitions for ES2023.

/// Source span (byte offsets)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }
    
    pub fn len(&self) -> u32 {
        self.end - self.start
    }
    
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
    
    pub fn merge(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// Token with kind and span
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

/// Token kinds for JavaScript
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literals
    Number(f64),
    String(Box<str>),
    BigInt(Box<str>),
    Boolean(bool),
    Null,
    Undefined,
    Regex { pattern: Box<str>, flags: Box<str> },
    
    // Identifiers and Keywords
    Identifier(Box<str>),
    PrivateIdentifier(Box<str>),  // #privateField
    
    // Keywords
    Await,
    Break,
    Case,
    Catch,
    Class,
    Const,
    Continue,
    Debugger,
    Default,
    Delete,
    Do,
    Else,
    Enum,
    Export,
    Extends,
    Finally,
    For,
    Function,
    If,
    Import,
    In,
    Instanceof,
    Let,
    New,
    Of,
    Return,
    Super,
    Switch,
    This,
    Throw,
    Try,
    Typeof,
    Var,
    Void,
    While,
    With,
    Yield,
    
    // Contextual keywords
    As,
    Async,
    From,
    Get,
    Meta,
    Set,
    Static,
    Target,
    
    // Punctuators
    LBrace,      // {
    RBrace,      // }
    LParen,      // (
    RParen,      // )
    LBracket,    // [
    RBracket,    // ]
    Dot,         // .
    DotDotDot,   // ...
    Semicolon,   // ;
    Comma,       // ,
    Colon,       // :
    Question,    // ?
    QuestionDot, // ?.
    QuestionQuestion, // ??
    Arrow,       // =>
    
    // Operators
    Plus,        // +
    Minus,       // -
    Star,        // *
    StarStar,    // **
    Slash,       // /
    Percent,     // %
    PlusPlus,    // ++
    MinusMinus,  // --
    LessThan,    // <
    LessThanEq,  // <=
    GreaterThan, // >
    GreaterThanEq, // >=
    EqEq,        // ==
    NotEq,       // !=
    EqEqEq,      // ===
    NotEqEq,     // !==
    Ampersand,   // &
    Pipe,        // |
    Caret,       // ^
    Tilde,       // ~
    AmpersandAmpersand, // &&
    PipePipe,    // ||
    Bang,        // !
    LShift,      // <<
    RShift,      // >>
    URShift,     // >>>
    
    // Assignment
    Eq,          // =
    PlusEq,      // +=
    MinusEq,     // -=
    StarEq,      // *=
    StarStarEq,  // **=
    SlashEq,     // /=
    PercentEq,   // %=
    AmpersandEq, // &=
    PipeEq,      // |=
    CaretEq,     // ^=
    LShiftEq,    // <<=
    RShiftEq,    // >>=
    URShiftEq,   // >>>=
    AmpersandAmpersandEq, // &&=
    PipePipeEq,  // ||=
    QuestionQuestionEq, // ??=
    
    // Template literals
    TemplateHead(Box<str>),
    TemplateMiddle(Box<str>),
    TemplateTail(Box<str>),
    NoSubstitutionTemplate(Box<str>),
    
    // Special
    Eof,
    LineTerminator,
    Error(Box<str>),
}

impl TokenKind {
    /// Check if this is a keyword
    pub fn is_keyword(&self) -> bool {
        matches!(self,
            TokenKind::Await | TokenKind::Break | TokenKind::Case |
            TokenKind::Catch | TokenKind::Class | TokenKind::Const |
            TokenKind::Continue | TokenKind::Debugger | TokenKind::Default |
            TokenKind::Delete | TokenKind::Do | TokenKind::Else |
            TokenKind::Enum | TokenKind::Export | TokenKind::Extends |
            TokenKind::Finally | TokenKind::For | TokenKind::Function |
            TokenKind::If | TokenKind::Import | TokenKind::In |
            TokenKind::Instanceof | TokenKind::Let | TokenKind::New |
            TokenKind::Of | TokenKind::Return | TokenKind::Super |
            TokenKind::Switch | TokenKind::This | TokenKind::Throw |
            TokenKind::Try | TokenKind::Typeof | TokenKind::Var |
            TokenKind::Void | TokenKind::While | TokenKind::With |
            TokenKind::Yield
        )
    }
    
    /// Check if this is an assignment operator
    pub fn is_assignment(&self) -> bool {
        matches!(self,
            TokenKind::Eq | TokenKind::PlusEq | TokenKind::MinusEq |
            TokenKind::StarEq | TokenKind::StarStarEq | TokenKind::SlashEq |
            TokenKind::PercentEq | TokenKind::AmpersandEq | TokenKind::PipeEq |
            TokenKind::CaretEq | TokenKind::LShiftEq | TokenKind::RShiftEq |
            TokenKind::URShiftEq | TokenKind::AmpersandAmpersandEq |
            TokenKind::PipePipeEq | TokenKind::QuestionQuestionEq
        )
    }
    
    /// Check if this is a binary operator
    pub fn is_binary_op(&self) -> bool {
        matches!(self,
            TokenKind::Plus | TokenKind::Minus | TokenKind::Star |
            TokenKind::StarStar | TokenKind::Slash | TokenKind::Percent |
            TokenKind::LessThan | TokenKind::LessThanEq |
            TokenKind::GreaterThan | TokenKind::GreaterThanEq |
            TokenKind::EqEq | TokenKind::NotEq | TokenKind::EqEqEq |
            TokenKind::NotEqEq | TokenKind::Ampersand | TokenKind::Pipe |
            TokenKind::Caret | TokenKind::AmpersandAmpersand |
            TokenKind::PipePipe | TokenKind::LShift | TokenKind::RShift |
            TokenKind::URShift | TokenKind::QuestionQuestion |
            TokenKind::In | TokenKind::Instanceof
        )
    }
    
    /// Get operator precedence (higher = binds tighter)
    pub fn precedence(&self) -> u8 {
        match self {
            TokenKind::PipePipe | TokenKind::QuestionQuestion => 4,
            TokenKind::AmpersandAmpersand => 5,
            TokenKind::Pipe => 6,
            TokenKind::Caret => 7,
            TokenKind::Ampersand => 8,
            TokenKind::EqEq | TokenKind::NotEq | TokenKind::EqEqEq | TokenKind::NotEqEq => 9,
            TokenKind::LessThan | TokenKind::LessThanEq | TokenKind::GreaterThan | 
            TokenKind::GreaterThanEq | TokenKind::In | TokenKind::Instanceof => 10,
            TokenKind::LShift | TokenKind::RShift | TokenKind::URShift => 11,
            TokenKind::Plus | TokenKind::Minus => 12,
            TokenKind::Star | TokenKind::Slash | TokenKind::Percent => 13,
            TokenKind::StarStar => 14,
            _ => 0,
        }
    }
}

/// Keywords lookup table
pub fn keyword_from_str(s: &str) -> Option<TokenKind> {
    match s {
        "await" => Some(TokenKind::Await),
        "break" => Some(TokenKind::Break),
        "case" => Some(TokenKind::Case),
        "catch" => Some(TokenKind::Catch),
        "class" => Some(TokenKind::Class),
        "const" => Some(TokenKind::Const),
        "continue" => Some(TokenKind::Continue),
        "debugger" => Some(TokenKind::Debugger),
        "default" => Some(TokenKind::Default),
        "delete" => Some(TokenKind::Delete),
        "do" => Some(TokenKind::Do),
        "else" => Some(TokenKind::Else),
        "enum" => Some(TokenKind::Enum),
        "export" => Some(TokenKind::Export),
        "extends" => Some(TokenKind::Extends),
        "false" => Some(TokenKind::Boolean(false)),
        "finally" => Some(TokenKind::Finally),
        "for" => Some(TokenKind::For),
        "function" => Some(TokenKind::Function),
        "if" => Some(TokenKind::If),
        "import" => Some(TokenKind::Import),
        "in" => Some(TokenKind::In),
        "instanceof" => Some(TokenKind::Instanceof),
        "let" => Some(TokenKind::Let),
        "new" => Some(TokenKind::New),
        "null" => Some(TokenKind::Null),
        "of" => Some(TokenKind::Of),
        "return" => Some(TokenKind::Return),
        "super" => Some(TokenKind::Super),
        "switch" => Some(TokenKind::Switch),
        "this" => Some(TokenKind::This),
        "throw" => Some(TokenKind::Throw),
        "true" => Some(TokenKind::Boolean(true)),
        "try" => Some(TokenKind::Try),
        "typeof" => Some(TokenKind::Typeof),
        "undefined" => Some(TokenKind::Undefined),
        "var" => Some(TokenKind::Var),
        "void" => Some(TokenKind::Void),
        "while" => Some(TokenKind::While),
        "with" => Some(TokenKind::With),
        "yield" => Some(TokenKind::Yield),
        // Contextual keywords
        "as" => Some(TokenKind::As),
        "async" => Some(TokenKind::Async),
        "from" => Some(TokenKind::From),
        "get" => Some(TokenKind::Get),
        "meta" => Some(TokenKind::Meta),
        "set" => Some(TokenKind::Set),
        "static" => Some(TokenKind::Static),
        "target" => Some(TokenKind::Target),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_span() {
        let span = Span::new(0, 10);
        assert_eq!(span.len(), 10);
        assert!(!span.is_empty());
    }
    
    #[test]
    fn test_span_merge() {
        let a = Span::new(0, 5);
        let b = Span::new(3, 10);
        let merged = a.merge(b);
        assert_eq!(merged.start, 0);
        assert_eq!(merged.end, 10);
    }
    
    #[test]
    fn test_keyword_lookup() {
        assert_eq!(keyword_from_str("if"), Some(TokenKind::If));
        assert_eq!(keyword_from_str("true"), Some(TokenKind::Boolean(true)));
        assert_eq!(keyword_from_str("null"), Some(TokenKind::Null));
        assert_eq!(keyword_from_str("notakeyword"), None);
    }
    
    #[test]
    fn test_precedence() {
        assert!(TokenKind::Star.precedence() > TokenKind::Plus.precedence());
        assert!(TokenKind::Plus.precedence() > TokenKind::PipePipe.precedence());
    }
}
