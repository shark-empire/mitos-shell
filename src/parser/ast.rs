#[derive(Debug, Clone)]
pub enum Node {
    Pipeline(Pipeline),
    /// left && right  |  left || right
    AndOr(Box<Node>, ListOp, Box<Node>),
    /// left ; right
    Sequence(Box<Node>, Box<Node>),
    /// node &
    Background(Box<Node>),
    /// ( node )
    Subshell(Box<Node>),
    /// { node; }
    BraceGroup(Box<Node>),
    If(IfClause),
    While(WhileClause),
    For(ForClause),
    Function(FunctionDef),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ListOp { And, Or }

#[derive(Debug, Clone)]
pub struct Pipeline {
    pub commands: Vec<SimpleCommand>,
    pub negated: bool, // leading `!`
}

#[derive(Debug, Clone)]
pub struct SimpleCommand {
    /// VAR=value prefixes
    pub assignments: Vec<(String, String)>,
    pub args: Vec<String>,
    pub redirects: Vec<Redirect>,
}

#[derive(Debug, Clone)]
pub enum Redirect {
    Input(String),
    Output(String),
    Append(String),
}

#[derive(Debug, Clone)]
pub struct IfClause {
    pub condition: Box<Node>,
    pub then_branch: Box<Node>,
    pub else_branch: Option<Box<Node>>,
}

#[derive(Debug, Clone)]
pub struct WhileClause {
    pub condition: Box<Node>,
    pub body: Box<Node>,
}

#[derive(Debug, Clone)]
pub struct ForClause {
    pub var: String,
    pub words: Vec<String>,
    pub body: Box<Node>,
}

#[derive(Debug, Clone)]
pub struct FunctionDef {
    pub name: String,
    pub body: Box<Node>,
}
