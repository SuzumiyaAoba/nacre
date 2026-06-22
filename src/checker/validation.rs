use crate::{CompileError, Expr, Program, Statement, Type};

use super::names::is_valid_name;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FlowSummary {
    pub(super) falls_through: bool,
    returns: bool,
    breaks: bool,
    continues: bool,
}

impl FlowSummary {
    const fn falls_through() -> Self {
        Self {
            falls_through: true,
            returns: false,
            breaks: false,
            continues: false,
        }
    }

    const fn returned() -> Self {
        Self {
            falls_through: false,
            returns: true,
            breaks: false,
            continues: false,
        }
    }

    const fn broke() -> Self {
        Self {
            falls_through: false,
            returns: false,
            breaks: true,
            continues: false,
        }
    }

    const fn continued() -> Self {
        Self {
            falls_through: false,
            returns: false,
            breaks: false,
            continues: true,
        }
    }

    const fn try_result() -> Self {
        Self {
            falls_through: true,
            returns: true,
            breaks: false,
            continues: false,
        }
    }

    fn alternatives(left: Self, right: Self) -> Self {
        Self {
            falls_through: left.falls_through || right.falls_through,
            returns: left.returns || right.returns,
            breaks: left.breaks || right.breaks,
            continues: left.continues || right.continues,
        }
    }

    pub(super) fn always_returns(self) -> bool {
        !self.falls_through && self.returns && !self.breaks && !self.continues
    }
}

pub(super) fn program_flow(program: &Program) -> FlowSummary {
    let mut flow = FlowSummary::falls_through();
    for statement in program.statements() {
        if !flow.falls_through {
            break;
        }
        let statement = statement_flow(statement);
        flow.falls_through = statement.falls_through;
        flow.returns |= statement.returns;
        flow.breaks |= statement.breaks;
        flow.continues |= statement.continues;
    }
    flow
}

pub(super) fn statement_flow(statement: &Statement) -> FlowSummary {
    match statement {
        Statement::Return(_) => FlowSummary::returned(),
        Statement::Break => FlowSummary::broke(),
        Statement::Continue => FlowSummary::continued(),
        Statement::TryResult(_) => FlowSummary::try_result(),
        Statement::Defer(_) => FlowSummary::falls_through(),
        Statement::Block { body } => program_flow(body),
        Statement::If {
            then_branch,
            else_branch,
            ..
        } => FlowSummary::alternatives(
            program_flow(then_branch),
            else_branch
                .as_ref()
                .map_or_else(FlowSummary::falls_through, program_flow),
        ),
        Statement::While { body, .. } | Statement::For { body, .. } => {
            let body = program_flow(body);
            FlowSummary {
                falls_through: true,
                returns: body.returns,
                breaks: false,
                continues: false,
            }
        }
        Statement::Function { .. }
        | Statement::ExternalFunction { .. }
        | Statement::Use { .. }
        | Statement::Trait { .. }
        | Statement::Impl { .. }
        | Statement::InherentImpl { .. }
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
        | Statement::TryPipeline { .. }
        | Statement::TryPipelineResult { .. }
        | Statement::Command(_)
        | Statement::Redirect { .. }
        | Statement::Require { .. }
        | Statement::RequireOneOf { .. }
        | Statement::Raw(_) => FlowSummary::falls_through(),
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
        let interpolation = &after_start[..end];
        let Some(name) = interpolation_base_name(interpolation) else {
            return Err(CompileError::new(
                line,
                format!("invalid interpolation name `{interpolation}`"),
            ));
        };
        names.push(name.to_string());
        rest = &after_start[end + 1..];
    }
    Ok(names)
}

fn interpolation_base_name(value: &str) -> Option<&str> {
    if is_valid_name(value) {
        return Some(value);
    }
    if let Some((base, index)) = value
        .strip_suffix(']')
        .and_then(|value| value.split_once('['))
    {
        if is_valid_name(base) && !index.is_empty() {
            return Some(base);
        }
    }
    if let Some((base, field)) = value.split_once('.') {
        if is_valid_name(base) && (is_valid_name(field) || tuple_field_name(field)) {
            return Some(base);
        }
    }
    None
}

fn tuple_field_name(value: &str) -> bool {
    value
        .strip_prefix('_')
        .is_some_and(|field| !field.is_empty() && field.chars().all(|ch| ch.is_ascii_digit()))
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
