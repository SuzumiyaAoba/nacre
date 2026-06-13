use crate::{CompileError, Program};

pub fn parse(source: &str) -> Result<Program, CompileError> {
    crate::parser_peg::parse(source)
}
