use super::type_utils::{command_result_type, result_types};
use super::validation::program_flow;
use super::{FunctionDecl, FunctionSig, TypeChecker};
use crate::symbols::shell_function_name;
use crate::{CompileError, Expr, Param, Program, Statement, Type, TypeParam};
use std::collections::HashSet;

impl TypeChecker {
    pub(super) fn check_function(
        &mut self,
        declaration: FunctionDecl<'_>,
        line: usize,
    ) -> Result<(), CompileError> {
        let FunctionDecl {
            name,
            override_constructor,
            type_params,
            params,
            return_type,
            body,
        } = declaration;
        if self.functions.contains_key(name) {
            return Err(CompileError::new(
                line,
                format!("function `{name}` is already defined"),
            ));
        }
        let backend_name = shell_function_name(name);
        if let Some(existing) = self
            .functions
            .keys()
            .find(|existing| shell_function_name(existing) == backend_name)
        {
            return Err(CompileError::new(
                line,
                format!("function `{name}` conflicts with `{existing}` after Bash name mangling"),
            ));
        }
        if self.bindings.contains_key(name) {
            return Err(CompileError::new(
                line,
                format!("function `{name}` conflicts with existing variable"),
            ));
        }
        if override_constructor {
            self.check_constructor_override(name, type_params, line)?;
        } else if self.types.contains_key(name) {
            return Err(CompileError::new(
                line,
                format!("function `{name}` conflicts with existing type; use `fn!` to override its constructor"),
            ));
        }

        let generic_names = type_params
            .iter()
            .map(|param| param.name.clone())
            .collect::<HashSet<_>>();
        for type_param in type_params {
            for bound in &type_param.bounds {
                if !self.traits.contains_key(bound) {
                    return Err(CompileError::new(
                        line,
                        format!("unknown trait `{bound}` in generic bound"),
                    ));
                }
            }
        }
        let mut resolved_params = Vec::new();
        let mut saw_default = false;
        for (index, param) in params.iter().enumerate() {
            if param.variadic {
                if index + 1 != params.len() {
                    return Err(CompileError::new(
                        line,
                        "rest parameter must be last".to_string(),
                    ));
                }
                if param.default.is_some() {
                    return Err(CompileError::new(
                        line,
                        "rest parameter cannot have a default".to_string(),
                    ));
                }
                if saw_default {
                    return Err(CompileError::new(
                        line,
                        "rest parameters cannot follow default parameters".to_string(),
                    ));
                }
            }
            if param.default.is_some() {
                saw_default = true;
            } else if saw_default {
                return Err(CompileError::new(
                    line,
                    "required function parameters cannot follow default parameters".to_string(),
                ));
            }
            let ty = self.resolve_type_with_generics(&param.ty, &generic_names, line)?;
            if let Some(default) = &param.default {
                let default_ty = self.check_expr(default, line)?;
                if !self.is_assignable(&ty, &default_ty, default) {
                    return Err(CompileError::new(
                        line,
                        format!(
                            "default for parameter `{}`: expected {}, found {}",
                            param.name,
                            ty.name(),
                            default_ty.name()
                        ),
                    ));
                }
            }
            resolved_params.push(Param {
                name: param.name.clone(),
                ty,
                default: param.default.clone(),
                variadic: param.variadic,
                capture_name: param.capture_name.clone(),
            });
        }
        let resolved_return = self.resolve_type_with_generics(return_type, &generic_names, line)?;

        self.functions.insert(
            name.to_string(),
            FunctionSig {
                type_params: type_params.to_vec(),
                params: resolved_params.clone(),
                return_type: resolved_return.clone(),
            },
        );
        if override_constructor {
            self.constructor_overrides.insert(name.to_string());
        }

        let mut body_checker = self.clone();
        body_checker.enforce_capabilities = self.should_enforce_function_capabilities(name);
        body_checker.expected_return = Some(resolved_return.clone());
        body_checker.loop_depth = 0;
        for param in resolved_params {
            body_checker.define(&param.name, param.ty, false, line)?;
        }
        if resolved_return != Type::Unit
            && !body_checker.program_has_return_or_implicit(body, &resolved_return, line)?
        {
            return Err(CompileError::new(
                line,
                format!("function `{name}` must return {}", resolved_return.name()),
            ));
        }
        body_checker.check_program(body)
    }

    pub(super) fn check_constructor_override(
        &self,
        name: &str,
        type_params: &[TypeParam],
        line: usize,
    ) -> Result<(), CompileError> {
        if !type_params.is_empty() {
            return Err(CompileError::new(
                line,
                "newtype constructor overrides cannot declare type parameters".to_string(),
            ));
        }
        let Some(ty) = self.types.get(name) else {
            return Err(CompileError::new(
                line,
                format!("fn! `{name}` can only override an existing newtype constructor"),
            ));
        };
        if !matches!(ty, Type::Brand { .. }) {
            return Err(CompileError::new(
                line,
                format!("fn! `{name}` can only override a newtype constructor"),
            ));
        }
        Ok(())
    }

    pub(super) fn define_external_function(
        &mut self,
        name: &str,
        type_params: &[TypeParam],
        params: &[Param],
        return_type: &Type,
        line: usize,
    ) -> Result<(), CompileError> {
        if self.functions.contains_key(name) {
            return Err(CompileError::new(
                line,
                format!("function `{name}` is already defined"),
            ));
        }
        if self.bindings.contains_key(name) {
            return Err(CompileError::new(
                line,
                format!("function `{name}` conflicts with existing variable"),
            ));
        }
        if !type_params.is_empty() {
            return Err(CompileError::new(
                line,
                "external functions cannot declare type parameters".to_string(),
            ));
        }

        let mut resolved_params = Vec::new();
        for (index, param) in params.iter().enumerate() {
            if param.variadic && index + 1 != params.len() {
                return Err(CompileError::new(
                    line,
                    "rest parameter must be last".to_string(),
                ));
            }
            if param.default.is_some() {
                return Err(CompileError::new(
                    line,
                    format!(
                        "external function parameter `{}` cannot have a default",
                        param.name
                    ),
                ));
            }
            resolved_params.push(Param {
                name: param.name.clone(),
                ty: self.resolve_type(&param.ty, line)?,
                default: None,
                variadic: param.variadic,
                capture_name: param.capture_name.clone(),
            });
        }
        let resolved_return = self.resolve_type(return_type, line)?;
        self.functions.insert(
            name.to_string(),
            FunctionSig {
                type_params: Vec::new(),
                params: resolved_params,
                return_type: resolved_return,
            },
        );
        Ok(())
    }

    pub(super) fn lower_function_body(
        &self,
        name: &str,
        type_params: &[TypeParam],
        params: &[Param],
        return_type: &Type,
        body: &Program,
        line: usize,
    ) -> Result<Program, CompileError> {
        let generic_names = type_params
            .iter()
            .map(|param| param.name.clone())
            .collect::<HashSet<_>>();
        let resolved_return = self.resolve_type_with_generics(return_type, &generic_names, line)?;
        let mut body_checker = self.clone();
        body_checker.enforce_capabilities = self.should_enforce_function_capabilities(name);
        body_checker.expected_return = Some(resolved_return.clone());
        body_checker.loop_depth = 0;
        for param in params {
            let ty = self.resolve_type_with_generics(&param.ty, &generic_names, line)?;
            body_checker.define(&param.name, ty, false, line)?;
        }
        let lowered = body_checker.check_and_lower_program(body)?;
        body_checker.lower_implicit_function_return(lowered, &resolved_return, line)
    }

    pub(super) fn check_return(&self, expr: &Expr, line: usize) -> Result<(), CompileError> {
        let Some(expected) = &self.expected_return else {
            return Err(CompileError::new(
                line,
                "return is only valid inside a function".to_string(),
            ));
        };
        let actual = if let Expr::TryResult(value) = expr {
            self.check_try_result_expr(value, line)?
        } else {
            self.check_expr_expected(expr, expected, line)?
        };
        if self.is_assignable(expected, &actual, expr)
            || result_types(expected)
                .is_some_and(|(ok_ty, _)| self.is_assignable(ok_ty, &actual, expr))
        {
            Ok(())
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "return type mismatch: expected {}, found {}",
                    expected.name(),
                    actual.name()
                ),
            ))
        }
    }

    pub(super) fn check_try_result(&self, expr: &Expr, line: usize) -> Result<(), CompileError> {
        self.try_result_types(expr, line).map(|_| ())
    }

    pub(super) fn check_try_result_expr(
        &self,
        expr: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        self.try_result_types(expr, line).map(|(ok_ty, _)| ok_ty)
    }

    pub(super) fn try_result_types(
        &self,
        expr: &Expr,
        line: usize,
    ) -> Result<(Type, Type), CompileError> {
        let Some(expected) = &self.expected_return else {
            return Err(CompileError::new(
                line,
                "try result is only valid inside a Result-returning function".to_string(),
            ));
        };
        let expected = self.resolve_type(expected, line)?;
        let Some((_, expected_err)) = result_types(&expected) else {
            return Err(CompileError::new(
                line,
                "try result is only valid inside a Result-returning function".to_string(),
            ));
        };
        let actual = self.check_expr(expr, line)?;
        let Some((_, actual_err)) = result_types(&actual) else {
            return Err(CompileError::new(
                line,
                format!("try expects Result value, found {}", actual.name()),
            ));
        };
        if self.is_assignable(expected_err, actual_err, expr) {
            let (ok_ty, err_ty) = result_types(&actual).expect("result_types matched Result");
            Ok((ok_ty.clone(), err_ty.clone()))
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "try error mismatch: expected {}, found {}",
                    expected_err.name(),
                    actual_err.name()
                ),
            ))
        }
    }

    pub(super) fn program_has_return_or_implicit(
        &self,
        program: &Program,
        expected: &Type,
        line: usize,
    ) -> Result<bool, CompileError> {
        if program_flow(program).always_returns() {
            return Ok(true);
        }
        let Some(statement) = program.statements().last() else {
            return Ok(false);
        };
        let statement_line = program.statement_lines().last().copied().unwrap_or(line);
        self.statement_is_implicit_return(statement, expected, statement_line)
    }

    pub(super) fn statement_is_implicit_return(
        &self,
        statement: &Statement,
        expected: &Type,
        line: usize,
    ) -> Result<bool, CompileError> {
        if expected == &Type::Unit {
            return Ok(false);
        }
        match statement {
            Statement::Command(_) if result_types(expected).is_some() => Ok(true),
            Statement::Expr(expr) => {
                let actual = self.return_expr_type(expr, expected, line)?;
                Ok(self.is_assignable(expected, &actual, expr))
            }
            _ => Ok(false),
        }
    }

    pub(super) fn return_expr_type(
        &self,
        expr: &Expr,
        expected: &Type,
        line: usize,
    ) -> Result<Type, CompileError> {
        if matches!(
            expr,
            Expr::Command { .. } | Expr::Pipeline { .. } | Expr::TryPipeline { .. }
        ) && result_types(expected).is_some()
        {
            Ok(command_result_type())
        } else {
            let actual = self.check_expr(expr, line)?;
            if result_types(expected)
                .is_some_and(|(ok_ty, _)| self.is_assignable(ok_ty, &actual, expr))
            {
                Ok(expected.clone())
            } else {
                Ok(actual)
            }
        }
    }

    pub(super) fn lower_implicit_function_return(
        &self,
        program: Program,
        expected: &Type,
        line: usize,
    ) -> Result<Program, CompileError> {
        if expected == &Type::Unit {
            return Ok(program);
        }
        if program_flow(&program).always_returns() {
            return Ok(program);
        }
        let mut statements = program.statements().to_vec();
        let lines = program.statement_lines().to_vec();
        let Some(last) = statements.pop() else {
            return Ok(Program::new(statements, lines));
        };
        let lowered = match last {
            Statement::Command(command) if result_types(expected).is_some() => {
                Statement::Return(Expr::CommandResult { command })
            }
            Statement::Expr(expr) => {
                let statement_line = lines.last().copied().unwrap_or(line);
                if self.statement_is_implicit_return(
                    &Statement::Expr(expr.clone()),
                    expected,
                    statement_line,
                )? {
                    Statement::Return(self.lower_binding_expr(&expr, expected, statement_line)?)
                } else {
                    Statement::Expr(expr)
                }
            }
            statement => statement,
        };
        statements.push(lowered);
        Ok(Program::new(statements, lines))
    }

    pub(super) fn check_condition(
        &self,
        condition: &Expr,
        line: usize,
    ) -> Result<(), CompileError> {
        let ty = self.check_expr(condition, line)?;
        if ty == Type::Bool {
            Ok(())
        } else {
            Err(CompileError::new(
                line,
                format!("condition must be Bool, found {}", ty.name()),
            ))
        }
    }

    pub(super) fn check_loop_control(
        &self,
        keyword: &str,
        line: usize,
    ) -> Result<(), CompileError> {
        if self.loop_depth > 0 {
            Ok(())
        } else {
            Err(CompileError::new(
                line,
                format!("`{keyword}` is only valid inside a loop"),
            ))
        }
    }
}
