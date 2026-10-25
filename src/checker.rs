mod bindings;
mod calls;
mod closures;
mod collections;
mod declarations;
mod effects;
mod expression_lowering;
mod expressions;
mod functions;
mod matching;
mod names;
mod option_result;
mod reachability;
mod strings;
mod try_lowering;
mod type_utils;
mod types;
mod validation;

use self::names::collect_declared_names;
use self::reachability::{is_imported_function_name, reachable_imported_functions};
use self::type_utils::{cmd_error_type, command_output_type};
use self::validation::{interpolation_names, statement_flow, unsafe_execution_error};
use crate::policy::ExecutionPolicy;
use crate::{
    CompileError, Expr, ForBinding, Param, Program, Statement, TraitMethod, Type, TypeParam,
};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

pub fn type_check(program: &Program) -> Result<(), CompileError> {
    type_check_and_lower(program).map(|_| ())
}

pub fn type_check_with_policy(
    program: &Program,
    policy: &ExecutionPolicy,
) -> Result<(), CompileError> {
    type_check_and_lower_with_policy(program, policy).map(|_| ())
}

pub(crate) fn type_check_and_lower(program: &Program) -> Result<Program, CompileError> {
    type_check_and_lower_with_policy(program, &ExecutionPolicy::deny_all())
}

pub(crate) fn type_check_and_lower_with_policy(
    program: &Program,
    policy: &ExecutionPolicy,
) -> Result<Program, CompileError> {
    let mut checker = TypeChecker::with_policy(policy.clone());
    checker.reachable_imported_functions = Rc::new(reachable_imported_functions(program));
    collect_declared_names(program, &mut checker.reserved_names.borrow_mut());
    let program = checker.check_and_lower_program(program)?;
    let generated = checker.generated_functions.borrow();
    if generated.is_empty() {
        return Ok(program);
    }
    let mut statements = generated
        .iter()
        .map(|(statement, _)| statement.clone())
        .collect::<Vec<_>>();
    statements.extend_from_slice(program.statements());
    let mut lines = generated.iter().map(|(_, line)| *line).collect::<Vec<_>>();
    lines.extend_from_slice(program.statement_lines());
    Ok(Program::new(statements, lines))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Binding {
    ty: Type,
    mutable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FunctionSig {
    type_params: Vec<TypeParam>,
    params: Vec<Param>,
    return_type: Type,
}

impl FunctionSig {
    fn function_type(&self) -> Type {
        Type::Function(
            self.params.iter().map(|param| param.ty.clone()).collect(),
            Box::new(self.return_type.clone()),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TraitSig {
    type_param: String,
    methods: Vec<TraitMethod>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VariantSig {
    sum_type: String,
    type_params: Vec<String>,
    fields: Vec<Type>,
}

struct FunctionDecl<'a> {
    name: &'a str,
    override_constructor: bool,
    type_params: &'a [TypeParam],
    params: &'a [Param],
    return_type: &'a Type,
    body: &'a Program,
}

type CapturedBindings = Rc<RefCell<HashSet<String>>>;

#[derive(Clone, Copy)]
enum DoFamily {
    Option,
    Result,
}

#[derive(Clone)]
struct TypeChecker {
    policy: ExecutionPolicy,
    bindings: HashMap<String, Binding>,
    types: HashMap<String, Type>,
    generic_types: HashMap<String, (Vec<String>, Type)>,
    generic_newtypes: HashMap<String, (Vec<String>, Type)>,
    generic_sum_types: HashMap<String, Vec<String>>,
    traits: HashMap<String, TraitSig>,
    trait_impls: HashSet<(String, String)>,
    method_impls: HashMap<(String, String), Vec<(String, String)>>,
    inherent_methods: HashMap<(String, String), String>,
    functions: HashMap<String, FunctionSig>,
    sum_types: HashMap<String, Vec<String>>,
    variants: HashMap<String, VariantSig>,
    constructor_overrides: HashSet<String>,
    expected_return: Option<Type>,
    generated_functions: Rc<RefCell<Vec<(Statement, usize)>>>,
    next_lambda: Rc<Cell<usize>>,
    next_try_temp: Rc<Cell<usize>>,
    reserved_names: Rc<RefCell<HashSet<String>>>,
    captured_bindings: Option<CapturedBindings>,
    capture_params: HashSet<String>,
    loop_depth: usize,
    enforce_capabilities: bool,
    reachable_imported_functions: Rc<HashSet<String>>,
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::with_policy(ExecutionPolicy::deny_all())
    }
}

impl TypeChecker {
    fn with_policy(policy: ExecutionPolicy) -> Self {
        let mut types = HashMap::new();
        types.insert("CmdError".to_string(), cmd_error_type());
        types.insert("CommandOutput".to_string(), command_output_type());
        Self {
            policy,
            bindings: HashMap::new(),
            types,
            generic_types: HashMap::new(),
            generic_newtypes: HashMap::new(),
            generic_sum_types: HashMap::new(),
            traits: HashMap::new(),
            trait_impls: HashSet::new(),
            method_impls: HashMap::new(),
            inherent_methods: HashMap::new(),
            functions: HashMap::new(),
            sum_types: HashMap::new(),
            variants: HashMap::new(),
            constructor_overrides: HashSet::new(),
            expected_return: None,
            generated_functions: Rc::new(RefCell::new(Vec::new())),
            next_lambda: Rc::new(Cell::new(0)),
            next_try_temp: Rc::new(Cell::new(0)),
            reserved_names: Rc::new(RefCell::new(HashSet::new())),
            captured_bindings: None,
            capture_params: HashSet::new(),
            loop_depth: 0,
            enforce_capabilities: true,
            reachable_imported_functions: Rc::new(HashSet::new()),
        }
    }
}

impl TypeChecker {
    fn should_enforce_function_capabilities(&self, name: &str) -> bool {
        !is_imported_function_name(name) || self.reachable_imported_functions.contains(name)
    }

    fn check_program(&mut self, program: &Program) -> Result<(), CompileError> {
        let mut reachable = true;
        for (statement, line) in program.statements().iter().zip(program.statement_lines()) {
            if !reachable {
                return Err(CompileError::new(
                    *line,
                    "unreachable statement after control flow exits".to_string(),
                ));
            }
            self.check_statement(statement, *line)?;
            reachable = statement_flow(statement).falls_through;
        }
        Ok(())
    }

    fn check_and_lower_program(&mut self, program: &Program) -> Result<Program, CompileError> {
        let mut statements = Vec::new();
        let mut lines = Vec::new();
        let mut reachable = true;
        for (statement, line) in program.statements().iter().zip(program.statement_lines()) {
            for expanded in self.expand_nested_try_statement(statement) {
                if !reachable {
                    return Err(CompileError::new(
                        *line,
                        "unreachable statement after control flow exits".to_string(),
                    ));
                }
                let lowered = self.check_and_lower_statement(&expanded, *line)?;
                reachable = statement_flow(&lowered).falls_through;
                statements.push(lowered);
                lines.push(*line);
            }
        }
        Ok(Program::new(statements, lines))
    }

    fn check_and_lower_statement(
        &mut self,
        statement: &Statement,
        line: usize,
    ) -> Result<Statement, CompileError> {
        self.require_process_args_access_in_statement(statement, line)?;
        match statement {
            Statement::Export(inner) => self.check_and_lower_statement(inner, line),
            Statement::Use { .. } => Ok(statement.clone()),
            Statement::ExternalFunction {
                name,
                type_params,
                params,
                return_type,
            } => {
                self.define_external_function(name, type_params, params, return_type, line)?;
                Ok(statement.clone())
            }
            Statement::Trait {
                name,
                type_param,
                methods,
            } => {
                self.define_trait(name, type_param, methods, line)?;
                Ok(statement.clone())
            }
            Statement::Impl {
                trait_name,
                for_type,
                methods,
            } => {
                let lowered_methods =
                    self.define_trait_impl(trait_name, for_type, methods, line)?;
                Ok(Statement::Impl {
                    trait_name: trait_name.clone(),
                    for_type: for_type.clone(),
                    methods: lowered_methods,
                })
            }
            Statement::InherentImpl {
                for_type,
                consts,
                methods,
            } => {
                let (lowered_consts, lowered_methods) =
                    self.define_inherent_impl(for_type, consts, methods, line)?;
                Ok(Statement::InherentImpl {
                    for_type: self.resolve_type(for_type, line)?,
                    consts: lowered_consts,
                    methods: lowered_methods,
                })
            }
            Statement::TypeAlias {
                name,
                type_params,
                ty,
            } => {
                self.define_type_alias(name, type_params, ty, line)?;
                Ok(statement.clone())
            }
            Statement::SumType {
                name,
                type_params,
                variants,
            } => {
                self.define_sum_type(name, type_params, variants, line)?;
                Ok(statement.clone())
            }
            Statement::Newtype {
                name,
                type_params,
                base,
            } => {
                self.define_newtype(name, type_params, base, line)?;
                Ok(statement.clone())
            }
            Statement::Function {
                name,
                override_constructor,
                type_params,
                params,
                return_type,
                body,
            } => {
                self.check_function(
                    FunctionDecl {
                        name,
                        override_constructor: *override_constructor,
                        type_params,
                        params,
                        return_type,
                        body,
                    },
                    line,
                )?;
                Ok(Statement::Function {
                    name: name.clone(),
                    override_constructor: *override_constructor,
                    type_params: type_params.to_vec(),
                    params: params.to_vec(),
                    return_type: return_type.clone(),
                    body: self.lower_function_body(
                        name,
                        type_params,
                        params,
                        return_type,
                        body,
                        line,
                    )?,
                })
            }
            Statement::Const {
                name,
                annotation,
                expr,
            } => {
                let expected = annotation
                    .as_ref()
                    .map(|ty| self.resolve_type(ty, line))
                    .transpose()?;
                let ty = if matches!(expr, Expr::Lambda { .. }) {
                    let Some(expected) = expected.as_ref() else {
                        return Err(CompileError::new(
                            line,
                            "lambda type cannot be inferred; provide a function type annotation or pass it to a typed function parameter".to_string(),
                        ));
                    };
                    self.check_expr_expected(expr, expected, line)?
                } else {
                    self.binding_expr_type(annotation.as_ref(), expr, line)?
                };
                let binding_ty = self.check_annotation(annotation.clone(), ty, expr, line)?;
                let expr = self.lower_expr_expected(expr, &binding_ty, line)?;
                self.define(name, binding_ty, false, line)?;
                Ok(Statement::Const {
                    name: name.clone(),
                    annotation: annotation.clone(),
                    expr,
                })
            }
            Statement::Let {
                name,
                annotation,
                expr,
            } => {
                let expected = annotation
                    .as_ref()
                    .map(|ty| self.resolve_type(ty, line))
                    .transpose()?;
                let ty = if matches!(expr, Expr::Lambda { .. }) {
                    let Some(expected) = expected.as_ref() else {
                        return Err(CompileError::new(
                            line,
                            "lambda type cannot be inferred; provide a function type annotation or pass it to a typed function parameter".to_string(),
                        ));
                    };
                    self.check_expr_expected(expr, expected, line)?
                } else {
                    self.binding_expr_type(annotation.as_ref(), expr, line)?
                };
                let binding_ty = self.check_annotation(annotation.clone(), ty, expr, line)?;
                let expr = self.lower_expr_expected(expr, &binding_ty, line)?;
                self.define(name, binding_ty, true, line)?;
                Ok(Statement::Let {
                    name: name.clone(),
                    annotation: annotation.clone(),
                    expr,
                })
            }
            Statement::Destructure {
                mutable,
                pattern,
                expr,
            } => {
                let ty = self.check_expr(expr, line)?;
                self.define_destructure(pattern, &ty, *mutable, line)?;
                self.ensure_destructurable_source(expr, line)?;
                Ok(Statement::Destructure {
                    mutable: *mutable,
                    pattern: pattern.clone(),
                    expr: self.lower_expr(expr, line)?,
                })
            }
            Statement::Assign { target, expr } => {
                let expected = self.check_assignment(target, expr, line)?;
                Ok(Statement::Assign {
                    target: self.lower_assign_target(target, line)?,
                    expr: self.lower_expr_expected(expr, &expected, line)?,
                })
            }
            Statement::Expr(expr) => {
                if let Expr::ArrayPush { name, value } = expr {
                    self.check_array_push(name, value, line)?;
                    return Ok(Statement::Expr(self.lower_expr(expr, line)?));
                }
                if let Expr::ArrayPop { name } = expr {
                    self.check_array_pop(name, line)?;
                    return Ok(Statement::Expr(self.lower_expr(expr, line)?));
                }
                if let Expr::MapSet { name, key, value } = expr {
                    if !self.is_qualified_function_call(name, "set") {
                        self.check_map_set(name, key, value, line)?;
                        return Ok(Statement::Expr(self.lower_expr(expr, line)?));
                    }
                }
                if let Expr::MapRemove { name, key } = expr {
                    if !self.is_qualified_function_call(name, "remove") {
                        self.check_map_remove(name, key, line)?;
                        return Ok(Statement::Expr(self.lower_expr(expr, line)?));
                    }
                }
                self.check_expr(expr, line)?;
                Ok(Statement::Expr(self.lower_expr(expr, line)?))
            }
            Statement::TryCommand(_)
            | Statement::TryCommandResult(_)
            | Statement::TryPipeline { .. }
            | Statement::TryPipelineResult { .. }
            | Statement::Command(_)
            | Statement::Redirect { .. }
            | Statement::Require { .. }
            | Statement::RequireOneOf { .. }
            | Statement::Raw(_) => Err(unsafe_execution_error(line)),
            Statement::TryResult(expr) => {
                self.check_try_result(expr, line)?;
                Ok(Statement::TryResult(
                    self.lower_try_result_value(expr, line)?,
                ))
            }
            Statement::Break => {
                self.check_loop_control("break", line)?;
                Ok(statement.clone())
            }
            Statement::Continue => {
                self.check_loop_control("continue", line)?;
                Ok(statement.clone())
            }
            Statement::Return(expr) => {
                self.check_return(expr, line)?;
                let expected = self.expected_return.clone().ok_or_else(|| {
                    CompileError::new(line, "return is only valid inside a function".to_string())
                })?;
                Ok(Statement::Return(
                    self.lower_expr_expected(expr, &expected, line)?,
                ))
            }
            Statement::Defer(statement) => {
                let lowered = self.check_and_lower_statement(statement, line)?;
                if !statement_flow(&lowered).falls_through {
                    return Err(CompileError::new(
                        line,
                        "defer body must not exit control flow".to_string(),
                    ));
                }
                Ok(Statement::Defer(Box::new(lowered)))
            }
            Statement::Block { body } => {
                let mut body_checker = self.clone();
                Ok(Statement::Block {
                    body: body_checker.check_and_lower_program(body)?,
                })
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.check_condition(condition, line)?;
                let condition = self.lower_expr(condition, line)?;
                let mut then_checker = self.clone();
                let then_branch = then_checker.check_and_lower_program(then_branch)?;
                let else_branch = else_branch
                    .as_ref()
                    .map(|branch| {
                        let mut else_checker = self.clone();
                        else_checker.check_and_lower_program(branch)
                    })
                    .transpose()?;
                Ok(Statement::If {
                    condition,
                    then_branch,
                    else_branch,
                })
            }
            Statement::While { condition, body } => {
                self.check_condition(condition, line)?;
                let condition = self.lower_expr(condition, line)?;
                let mut body_checker = self.clone();
                body_checker.loop_depth += 1;
                let body = body_checker.check_and_lower_program(body)?;
                Ok(Statement::While { condition, body })
            }
            Statement::For {
                binding,
                iterable,
                body,
            } => {
                let iterable_ty = self.check_expr(iterable, line)?;
                let Type::Array(element_ty) = iterable_ty else {
                    return Err(CompileError::new(
                        line,
                        format!(
                            "for loop iterable must be Array, found {}",
                            iterable_ty.name()
                        ),
                    ));
                };
                let iterable = self.lower_expr(iterable, line)?;
                let mut body_checker = self.clone();
                body_checker.loop_depth += 1;
                body_checker.define_for_binding(binding, &element_ty, line)?;
                let body = body_checker.check_and_lower_program(body)?;
                Ok(Statement::For {
                    binding: binding.clone(),
                    iterable,
                    body,
                })
            }
        }
    }

    fn check_statement(&mut self, statement: &Statement, line: usize) -> Result<(), CompileError> {
        self.require_process_args_access_in_statement(statement, line)?;
        match statement {
            Statement::Export(inner) => self.check_statement(inner, line),
            Statement::Use { .. } => Ok(()),
            Statement::Trait {
                name,
                type_param,
                methods,
            } => self.define_trait(name, type_param, methods, line),
            Statement::Impl {
                trait_name,
                for_type,
                methods,
            } => self
                .define_trait_impl(trait_name, for_type, methods, line)
                .map(|_| ()),
            Statement::InherentImpl {
                for_type,
                consts,
                methods,
            } => self
                .define_inherent_impl(for_type, consts, methods, line)
                .map(|_| ()),
            Statement::TypeAlias {
                name,
                type_params,
                ty,
            } => self.define_type_alias(name, type_params, ty, line),
            Statement::SumType {
                name,
                type_params,
                variants,
            } => self.define_sum_type(name, type_params, variants, line),
            Statement::Newtype {
                name,
                type_params,
                base,
            } => self.define_newtype(name, type_params, base, line),
            Statement::Function {
                name,
                override_constructor,
                type_params,
                params,
                return_type,
                body,
            } => self.check_function(
                FunctionDecl {
                    name,
                    override_constructor: *override_constructor,
                    type_params,
                    params,
                    return_type,
                    body,
                },
                line,
            ),
            Statement::ExternalFunction {
                name,
                type_params,
                params,
                return_type,
            } => self.define_external_function(name, type_params, params, return_type, line),
            Statement::Const {
                name,
                annotation,
                expr,
            } => {
                let expected = annotation
                    .as_ref()
                    .map(|ty| self.resolve_type(ty, line))
                    .transpose()?;
                let ty = if matches!(expr, Expr::Lambda { .. }) {
                    let Some(expected) = expected.as_ref() else {
                        return Err(CompileError::new(
                            line,
                            "lambda type cannot be inferred; provide a function type annotation or pass it to a typed function parameter".to_string(),
                        ));
                    };
                    self.check_expr_expected(expr, expected, line)?
                } else {
                    self.binding_expr_type(annotation.as_ref(), expr, line)?
                };
                let binding_ty = self.check_annotation(annotation.clone(), ty, expr, line)?;
                self.define(name, binding_ty, false, line)
            }
            Statement::Let {
                name,
                annotation,
                expr,
            } => {
                let expected = annotation
                    .as_ref()
                    .map(|ty| self.resolve_type(ty, line))
                    .transpose()?;
                let ty = if matches!(expr, Expr::Lambda { .. }) {
                    let Some(expected) = expected.as_ref() else {
                        return Err(CompileError::new(
                            line,
                            "lambda type cannot be inferred; provide a function type annotation or pass it to a typed function parameter".to_string(),
                        ));
                    };
                    self.check_expr_expected(expr, expected, line)?
                } else {
                    self.binding_expr_type(annotation.as_ref(), expr, line)?
                };
                let binding_ty = self.check_annotation(annotation.clone(), ty, expr, line)?;
                self.define(name, binding_ty, true, line)
            }
            Statement::Destructure {
                mutable,
                pattern,
                expr,
            } => {
                let ty = self.check_expr(expr, line)?;
                self.define_destructure(pattern, &ty, *mutable, line)?;
                self.ensure_destructurable_source(expr, line)
            }
            Statement::Assign { target, expr } => {
                self.check_assignment(target, expr, line).map(|_| ())
            }
            Statement::Expr(expr) => match expr {
                Expr::ArrayPush { name, value } => {
                    self.check_array_push(name, value, line).map(|_| ())
                }
                Expr::ArrayPop { name } => self.check_array_pop(name, line).map(|_| ()),
                Expr::MapSet { name, key, value }
                    if !self.is_qualified_function_call(name, "set") =>
                {
                    self.check_map_set(name, key, value, line).map(|_| ())
                }
                Expr::MapRemove { name, key }
                    if !self.is_qualified_function_call(name, "remove") =>
                {
                    self.check_map_remove(name, key, line).map(|_| ())
                }
                _ => self.check_expr(expr, line).map(|_| ()),
            },
            Statement::TryResult(_) => Ok(()),
            Statement::TryCommand(_)
            | Statement::TryCommandResult(_)
            | Statement::TryPipeline { .. }
            | Statement::TryPipelineResult { .. }
            | Statement::Command(_)
            | Statement::Redirect { .. }
            | Statement::Require { .. }
            | Statement::RequireOneOf { .. }
            | Statement::Raw(_) => Err(unsafe_execution_error(line)),
            Statement::Break => self.check_loop_control("break", line),
            Statement::Continue => self.check_loop_control("continue", line),
            Statement::Return(expr) => self.check_return(expr, line),
            Statement::Defer(statement) => {
                self.check_statement(statement, line)?;
                if !statement_flow(statement).falls_through {
                    return Err(CompileError::new(
                        line,
                        "defer body must not exit control flow".to_string(),
                    ));
                }
                Ok(())
            }
            Statement::Block { body } => {
                let mut body_checker = self.clone();
                body_checker.check_program(body)
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => self.check_if(condition, then_branch, else_branch.as_ref(), line),
            Statement::While { condition, body } => self.check_while(condition, body, line),
            Statement::For {
                binding,
                iterable,
                body,
            } => self.check_for(binding, iterable, body, line),
        }
    }

    fn check_if(
        &self,
        condition: &Expr,
        then_branch: &Program,
        else_branch: Option<&Program>,
        line: usize,
    ) -> Result<(), CompileError> {
        self.check_condition(condition, line)?;
        let mut then_checker = self.clone();
        then_checker.check_program(then_branch)?;
        if let Some(else_branch) = else_branch {
            let mut else_checker = self.clone();
            return else_checker.check_program(else_branch);
        }
        Ok(())
    }

    fn check_while(
        &self,
        condition: &Expr,
        body: &Program,
        line: usize,
    ) -> Result<(), CompileError> {
        self.check_condition(condition, line)?;
        let mut body_checker = self.clone();
        body_checker.loop_depth += 1;
        body_checker.check_program(body)
    }

    fn check_for(
        &self,
        binding: &ForBinding,
        iterable: &Expr,
        body: &Program,
        line: usize,
    ) -> Result<(), CompileError> {
        let iterable_ty = self.check_expr(iterable, line)?;
        let Type::Array(element_ty) = iterable_ty else {
            return Err(CompileError::new(
                line,
                format!(
                    "for loop iterable must be Array, found {}",
                    iterable_ty.name()
                ),
            ));
        };

        let mut body_checker = self.clone();
        body_checker.loop_depth += 1;
        body_checker.define_for_binding(binding, &element_ty, line)?;
        body_checker.check_program(body)
    }

    fn check_string_interpolations(&self, value: &str, line: usize) -> Result<(), CompileError> {
        for name in interpolation_names(value, line)? {
            if !self.bindings.contains_key(&name) {
                return Err(CompileError::new(
                    line,
                    format!("undefined variable `{name}` in string interpolation"),
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
