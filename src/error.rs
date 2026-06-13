use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileError {
    line: usize,
    message: String,
}

impl CompileError {
    pub(crate) fn new(line: usize, message: String) -> Self {
        Self { line, message }
    }

    pub fn line(&self) -> usize {
        self.line
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for CompileError {}
