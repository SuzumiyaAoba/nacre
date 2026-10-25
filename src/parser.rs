mod builders;
mod diagnostics;
mod grammar;
mod validation;

use self::diagnostics::program_parse_error;
use self::grammar::nacre_grammar;
use self::validation::validate_program;
use crate::{
    BindingPattern, CompileError, Expr, ImplConst, ImplMethod, Param, Program, Statement, Type,
    TypeParam, VariantDecl,
};

#[derive(Debug)]
struct ParsedStatement {
    offset: usize,
    statement: Statement,
}

#[derive(Debug)]
enum BindingTarget {
    Name(String, Option<Type>),
    Pattern(BindingPattern),
}

#[derive(Debug)]
struct FunctionHead {
    name: String,
    override_constructor: bool,
    type_params: Vec<TypeParam>,
    params: Vec<Param>,
    return_type: Type,
}

#[derive(Debug)]
enum TypeDeclaration {
    Alias(Type),
    Sum(Vec<VariantDecl>),
}

#[derive(Debug)]
enum ImplItem {
    Const(ImplConst),
    Method(ImplMethod),
}

#[derive(Debug)]
enum Postfix {
    Call(Vec<Expr>),
    Method(String, Vec<Expr>),
    Index(Expr),
    Field(String),
    TupleField(usize),
    ResultOption,
    TryResult,
}

#[derive(Debug)]
enum CollectionEntry {
    Record(String, Expr),
    Map(Expr, Expr),
}

#[derive(Debug)]
enum DoItem {
    Bind(String, Expr),
    Let(String, Option<Type>, Expr),
    Expr(Expr),
}

pub fn parse(source: &str) -> Result<Program, CompileError> {
    let parsed =
        nacre_grammar::program_root(source).map_err(|error| program_parse_error(source, error))?;
    let mut statements = Vec::with_capacity(parsed.len());
    let mut lines = Vec::with_capacity(parsed.len());
    for parsed in parsed {
        statements.push(parsed.statement);
        lines.push(
            source[..parsed.offset]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
                + 1,
        );
    }
    let program = Program::new(statements, lines);
    validate_program(&program)?;
    Ok(program)
}

pub(crate) fn parse_expr(input: &str, line: usize) -> Result<Expr, CompileError> {
    let expr = nacre_grammar::expression_root(input).map_err(|error| {
        CompileError::new(
            line + error.location.line.saturating_sub(1),
            format!(
                "invalid expression syntax at column {}: expected {}",
                error.location.column, error.expected
            ),
        )
    })?;
    if matches!(
        &expr,
        Expr::Default {
            value,
            ..
        } if matches!(value.as_ref(), Expr::Env(_))
    ) {
        return Err(CompileError::new(
            line,
            "expected quoted string".to_string(),
        ));
    }
    Ok(expr)
}

#[cfg(test)]
pub(crate) fn parse_type(input: &str, line: usize) -> Result<Type, CompileError> {
    let ty = nacre_grammar::type_root(input).map_err(|error| {
        CompileError::new(
            line + error.location.line.saturating_sub(1),
            format!("invalid type syntax at column {}", error.location.column),
        )
    })?;
    if matches!(&ty, Type::Applied(name, args) if name == "Map" && args.len() != 2) {
        return Err(CompileError::new(
            line,
            "Map type requires key and value types".to_string(),
        ));
    }
    Ok(ty)
}

#[cfg(test)]
mod tests;
