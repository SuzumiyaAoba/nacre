use crate::{CompileError, Expr, Program, Statement, Type};

use super::names::is_valid_name;

pub(super) fn program_has_return(program: &Program) -> bool {
    program.statements().iter().any(statement_has_return)
}

pub(super) fn statement_has_return(statement: &Statement) -> bool {
    match statement {
        Statement::Return(_) => true,
        Statement::Block { body } => program_has_return(body),
        Statement::If {
            then_branch,
            else_branch,
            ..
        } => {
            program_has_return(then_branch) || else_branch.as_ref().is_some_and(program_has_return)
        }
        Statement::While { body, .. } | Statement::For { body, .. } => program_has_return(body),
        Statement::Function { .. }
        | Statement::ExternalFunction { .. }
        | Statement::Use { .. }
        | Statement::Trait { .. }
        | Statement::Impl { .. }
        | Statement::TypeAlias { .. }
        | Statement::SumType { .. }
        | Statement::Newtype { .. }
        | Statement::Const { .. }
        | Statement::Let { .. }
        | Statement::Destructure { .. }
        | Statement::Assign { .. }
        | Statement::Expr(_)
        | Statement::TryCommand(_)
        | Statement::TryCommandResult(_)
        | Statement::TryResult(_)
        | Statement::TryPipeline { .. }
        | Statement::TryPipelineResult { .. }
        | Statement::Command(_)
        | Statement::Redirect { .. }
        | Statement::Require { .. }
        | Statement::RequireOneOf { .. }
        | Statement::Break
        | Statement::Continue
        | Statement::Raw(_) => false,
    }
}

pub(super) fn interpolation_names(value: &str, line: usize) -> Result<Vec<String>, CompileError> {
    let mut names = Vec::new();
    let mut rest = value;
    while let Some(start) = rest.find("${") {
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find('}') else {
            return Err(CompileError::new(
                line,
                "unterminated string interpolation".to_string(),
            ));
        };
        let name = &after_start[..end];
        if !is_valid_name(name) {
            return Err(CompileError::new(
                line,
                format!("invalid interpolation name `{name}`"),
            ));
        }
        names.push(name.to_string());
        rest = &after_start[end + 1..];
    }
    Ok(names)
}

pub(super) fn match_pattern_mismatch(line: usize, value_ty: &Type, pattern: &Expr) -> CompileError {
    CompileError::new(
        line,
        format!(
            "match pattern type mismatch: expected {}, found {}",
            value_ty.name(),
            constructor_pattern_name(pattern)
        ),
    )
}

pub(super) fn constructor_pattern_name(pattern: &Expr) -> &'static str {
    match pattern {
        Expr::Some(_) | Expr::None => "Option",
        Expr::Ok(_) | Expr::Err(_) => "Result",
        _ => "pattern",
    }
}

pub(super) fn unsafe_execution_error(line: usize) -> CompileError {
    CompileError::new(
        line,
        "unsafe shell execution is disabled; use a policy-approved `run.<group>.<command>(...)` call"
            .to_string(),
    )
}
