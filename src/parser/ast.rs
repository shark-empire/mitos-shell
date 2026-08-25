#[derive(Debug, Clone)]
pub enum Node {
    Pipeline(Vec<Command>),
    // Future: List(Box<Node>, ListOp, Box<Node>), Subshell(Box<Node>)
}

#[derive(Debug, Clone)]
pub struct Command {
    pub args: Vec<String>,
    pub redirects: Vec<Redirect>,
    pub background: bool,
}

#[derive(Debug, Clone)]
pub enum Redirect {
    Input(String),
    Output(String),
    Append(String),
}
