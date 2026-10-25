use crate::{BinaryOp, CompileError, DoStep, Expr, Program, Statement, Type};

pub(super) fn validate_program(program: &Program) -> Result<(), CompileError> {
    for (statement, line) in program.statements().iter().zip(program.statement_lines()) {
        match statement {
            Statement::Function {
                params,
                return_type,
                body,
                ..
            } => {
                for param in params {
                    validate_type(&param.ty, *line)?;
                }
                validate_type(return_type, *line)?;
                validate_program(body)?;
            }
            Statement::Block { body }
            | Statement::While { body, .. }
            | Statement::For { body, .. } => validate_program(body)?,
            Statement::Trait { methods, .. } => {
                for method in methods {
                    for param in &method.params {
                        validate_type(&param.ty, *line)?;
                    }
                    validate_type(&method.return_type, *line)?;
                }
            }
            Statement::Impl {
                for_type, methods, ..
            } => {
                validate_type(for_type, *line)?;
                for method in methods {
                    for param in &method.params {
                        validate_type(&param.ty, *line)?;
                    }
                    validate_type(&method.return_type, *line)?;
                    validate_program(&method.body)?;
                }
            }
            Statement::InherentImpl {
                for_type,
                consts,
                methods,
            } => {
                validate_type(for_type, *line)?;
                for value in consts {
                    if let Some(annotation) = &value.annotation {
                        validate_type(annotation, *line)?;
                    }
                    validate_expr(&value.expr, *line)?;
                }
                for method in methods {
                    for param in &method.params {
                        validate_type(&param.ty, *line)?;
                    }
                    validate_type(&method.return_type, *line)?;
                    validate_program(&method.body)?;
                }
            }
            Statement::TypeAlias { ty, .. } => validate_type(ty, *line)?,
            Statement::SumType { variants, .. } => {
                for variant in variants {
                    for field in &variant.fields {
                        validate_type(field, *line)?;
                    }
                }
            }
            Statement::Newtype { base, .. } => validate_type(base, *line)?,
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                validate_expr(condition, *line)?;
                validate_program(then_branch)?;
                if let Some(else_branch) = else_branch {
                    validate_program(else_branch)?;
                }
            }
            Statement::Const {
                annotation, expr, ..
            }
            | Statement::Let {
                annotation, expr, ..
            } => {
                if let Some(annotation) = annotation {
                    validate_type(annotation, *line)?;
                }
                validate_expr(expr, *line)?;
            }
            Statement::Destructure { expr, .. }
            | Statement::Assign { expr, .. }
            | Statement::Expr(expr)
            | Statement::TryResult(expr)
            | Statement::Return(expr) => validate_expr(expr, *line)?,
            Statement::TryCommand(_)
            | Statement::TryCommandResult(_)
            | Statement::TryPipeline { .. }
            | Statement::TryPipelineResult { .. }
            | Statement::Command(_)
            | Statement::Redirect { .. } => {
                return Err(CompileError::new(
                    *line,
                    "$sh commands and shell pipelines are disabled; use a policy-approved `run.<group>.<command>(...)` call"
                        .to_string(),
                ));
            }
            Statement::Raw(_) => {
                return Err(CompileError::new(
                    *line,
                    "raw Bash blocks are disabled in the safe language profile".to_string(),
                ));
            }
            Statement::Require { .. } | Statement::RequireOneOf { .. } => {
                return Err(CompileError::new(
                    *line,
                    "`require` is disabled; declare every executable in the external policy and call it through `run.<group>.<command>(...)`".to_string(),
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn validate_type(ty: &Type, line: usize) -> Result<(), CompileError> {
    match ty {
        Type::Applied(name, args) if name == "Map" && args.len() != 2 => Err(CompileError::new(
            line,
            "Map type requires key and value types".to_string(),
        )),
        Type::Future(value) | Type::Array(value) | Type::Brand { base: value, .. } => {
            validate_type(value, line)
        }
        Type::Map(key, value) => {
            validate_type(key, line)?;
            validate_type(value, line)
        }
        Type::Record(fields) => {
            for (_, field) in fields {
                validate_type(field, line)?;
            }
            Ok(())
        }
        Type::Tuple(types)
        | Type::Union(types)
        | Type::Intersection(types)
        | Type::Applied(_, types) => {
            for ty in types {
                validate_type(ty, line)?;
            }
            Ok(())
        }
        Type::Function(params, result) => {
            for param in params {
                validate_type(param, line)?;
            }
            validate_type(result, line)
        }
        _ => Ok(()),
    }
}

pub(super) fn validate_expr(expr: &Expr, line: usize) -> Result<(), CompileError> {
    match expr {
        Expr::Default { value, fallback } => {
            if matches!(value.as_ref(), Expr::Env(_)) {
                return Err(CompileError::new(
                    line,
                    "expected quoted string".to_string(),
                ));
            }
            validate_expr(value, line)?;
            validate_expr(fallback, line)
        }
        Expr::Binary {
            left,
            op: BinaryOp::Shr,
            ..
        } if matches!(left.as_ref(), Expr::Command { .. } | Expr::Pipeline { .. }) => {
            Err(CompileError::new(
                line,
                "redirect target must be `write(...)` or `append(...)`".to_string(),
            ))
        }
        Expr::Binary { left, right, .. } => {
            validate_expr(left, line)?;
            validate_expr(right, line)
        }
        Expr::Array(values) | Expr::Tuple(values) => {
            for value in values {
                validate_expr(value, line)?;
            }
            Ok(())
        }
        Expr::Map(values) => {
            for (key, value) in values {
                validate_expr(key, line)?;
                validate_expr(value, line)?;
            }
            Ok(())
        }
        Expr::Record(values) => {
            for (_, value) in values {
                validate_expr(value, line)?;
            }
            Ok(())
        }
        Expr::Call { args, .. }
        | Expr::Variant { args, .. }
        | Expr::AllowedCommand { args, .. } => {
            for arg in args {
                validate_expr(arg, line)?;
            }
            Ok(())
        }
        Expr::Command { .. }
        | Expr::CommandResult { .. }
        | Expr::AsyncCommand(_)
        | Expr::Pipeline { .. }
        | Expr::TryPipeline { .. }
        | Expr::PipelineResult { .. } => Err(CompileError::new(
            line,
            "$sh commands and shell pipelines are disabled; use a policy-approved `run.<group>.<command>(...)` call"
                .to_string(),
        )),
        Expr::Lambda { params, body } => {
            for (index, param) in params.iter().enumerate() {
                if params[..index].contains(param) {
                    return Err(CompileError::new(
                        line,
                        format!("lambda parameter `{param}` is already defined"),
                    ));
                }
            }
            validate_expr(body, line)
        }
        Expr::IfElse {
            condition,
            then_expr,
            else_expr,
        } => {
            if matches!(condition.as_ref(), Expr::IfElse { .. }) {
                return Err(CompileError::new(
                    line,
                    "unexpected text after if expression".to_string(),
                ));
            }
            validate_expr(condition, line)?;
            validate_expr(then_expr, line)?;
            validate_expr(else_expr, line)
        }
        Expr::Match { value, arms } => {
            validate_expr(value, line)?;
            for arm in arms {
                if let Some(pattern) = &arm.pattern {
                    validate_expr(pattern, line)?;
                }
                if let Some(guard) = &arm.guard {
                    validate_expr(guard, line)?;
                }
                validate_expr(&arm.expr, line)?;
            }
            Ok(())
        }
        Expr::Do { steps, result } => {
            for step in steps {
                match step {
                    DoStep::Bind { expr, .. } | DoStep::Let { expr, .. } => {
                        validate_expr(expr, line)?;
                    }
                }
            }
            validate_expr(result, line)
        }
        _ => Ok(()),
    }
}
