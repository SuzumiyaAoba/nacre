mod names;
mod type_utils;
mod validation;

use names::*;
use type_utils::*;
use validation::*;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::policy::ExecutionPolicy;
use crate::{
    BinaryOp, BindingPattern, ClosureCapture, CompileError, DoStep, Expr, ImplMethod, MatchArm,
    Param, Program, Statement, TraitMethod, Type, TypeParam, VariantDecl,
};

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

fn collect_declared_names(program: &Program, names: &mut HashSet<String>) {
    for statement in program.statements() {
        match statement {
            Statement::Function {
                name, params, body, ..
            } => {
                names.insert(name.clone());
                names.extend(params.iter().map(|param| param.name.clone()));
                collect_declared_names(body, names);
            }
            Statement::ExternalFunction { name, params, .. } => {
                names.insert(name.clone());
                names.extend(params.iter().map(|param| param.name.clone()));
            }
            Statement::Impl { methods, .. } => {
                for method in methods {
                    names.insert(method.name.clone());
                    names.extend(method.params.iter().map(|param| param.name.clone()));
                    collect_declared_names(&method.body, names);
                }
            }
            Statement::Const { name, .. } | Statement::Let { name, .. } => {
                names.insert(name.clone());
            }
            Statement::Destructure { pattern, .. } => match pattern {
                BindingPattern::Tuple(values) => names.extend(values.iter().cloned()),
                BindingPattern::Record(fields) => {
                    names.extend(fields.iter().map(|(_, binding)| binding.clone()));
                }
                BindingPattern::Array {
                    names: values,
                    rest,
                } => {
                    names.extend(values.iter().cloned());
                    names.extend(rest.iter().cloned());
                }
            },
            Statement::Block { body } | Statement::While { body, .. } => {
                collect_declared_names(body, names);
            }
            Statement::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_declared_names(then_branch, names);
                if let Some(else_branch) = else_branch {
                    collect_declared_names(else_branch, names);
                }
            }
            Statement::For { name, body, .. } => {
                names.insert(name.clone());
                collect_declared_names(body, names);
            }
            _ => {}
        }
    }
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
    traits: HashMap<String, TraitSig>,
    trait_impls: HashSet<(String, String)>,
    method_impls: HashMap<(String, String), Vec<(String, String)>>,
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
        Self {
            policy,
            bindings: HashMap::new(),
            types,
            generic_types: HashMap::new(),
            traits: HashMap::new(),
            trait_impls: HashSet::new(),
            method_impls: HashMap::new(),
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
        }
    }
}

impl TypeChecker {
    fn check_program(&mut self, program: &Program) -> Result<(), CompileError> {
        for (statement, line) in program.statements().iter().zip(program.statement_lines()) {
            self.check_statement(statement, *line)?;
        }
        Ok(())
    }

    fn check_and_lower_program(&mut self, program: &Program) -> Result<Program, CompileError> {
        let mut statements = Vec::new();
        let mut lines = Vec::new();
        for (statement, line) in program.statements().iter().zip(program.statement_lines()) {
            for expanded in self.expand_nested_try_statement(statement) {
                statements.push(self.check_and_lower_statement(&expanded, *line)?);
                lines.push(*line);
            }
        }
        Ok(Program::new(statements, lines))
    }

    fn expand_nested_try_statement(&mut self, statement: &Statement) -> Vec<Statement> {
        let mut statement = statement.clone();
        let mut prefix = Vec::new();
        match &mut statement {
            Statement::Function { body, .. } => {
                *body = self.expand_nested_try_program(body);
            }
            Statement::Impl { methods, .. } => {
                for method in methods {
                    method.body = self.expand_nested_try_program(&method.body);
                }
            }
            Statement::Const { expr, .. }
            | Statement::Let { expr, .. }
            | Statement::Return(expr) => {
                self.extract_nested_try_results(expr, true, &mut prefix);
            }
            Statement::Destructure { expr, .. }
            | Statement::Assign { expr, .. }
            | Statement::Expr(expr) => {
                self.extract_nested_try_results(expr, false, &mut prefix);
            }
            Statement::Block { body } => {
                *body = self.expand_nested_try_program(body);
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.extract_nested_try_results(condition, false, &mut prefix);
                *then_branch = self.expand_nested_try_program(then_branch);
                if let Some(else_branch) = else_branch {
                    *else_branch = self.expand_nested_try_program(else_branch);
                }
            }
            Statement::For {
                iterable: condition,
                body,
                ..
            } => {
                self.extract_nested_try_results(condition, false, &mut prefix);
                *body = self.expand_nested_try_program(body);
            }
            Statement::While { body, .. } => {
                *body = self.expand_nested_try_program(body);
            }
            _ => {}
        }
        prefix.push(statement);
        prefix
    }

    fn expand_nested_try_program(&mut self, program: &Program) -> Program {
        let mut statements = Vec::new();
        let mut lines = Vec::new();
        for (statement, line) in program.statements().iter().zip(program.statement_lines()) {
            for expanded in self.expand_nested_try_statement(statement) {
                statements.push(expanded);
                lines.push(*line);
            }
        }
        Program::new(statements, lines)
    }

    fn extract_nested_try_results(
        &mut self,
        expr: &mut Expr,
        allow_root: bool,
        prefix: &mut Vec<Statement>,
    ) {
        if let Expr::TryResult(value) = expr {
            self.extract_nested_try_results(value, false, prefix);
            if allow_root {
                return;
            }
            let name = loop {
                let index = self.next_try_temp.get();
                self.next_try_temp.set(index + 1);
                let candidate = format!("__nacre_try_value_{index}");
                if self.reserved_names.borrow_mut().insert(candidate.clone()) {
                    break candidate;
                }
            };
            let propagated = std::mem::replace(expr, Expr::Ident(name.clone()));
            prefix.push(Statement::Const {
                name,
                annotation: None,
                expr: propagated,
            });
            return;
        }

        match expr {
            Expr::Some(value)
            | Expr::Ok(value)
            | Expr::Err(value)
            | Expr::ResultOption(value)
            | Expr::PathExists(value)
            | Expr::ArrayLenValue(value)
            | Expr::MapLenValue(value)
            | Expr::ArrayIsEmptyValue(value)
            | Expr::MapIsEmptyValue(value)
            | Expr::ArrayFirstValue(value)
            | Expr::ArrayLastValue(value)
            | Expr::ArrayReverseValue(value)
            | Expr::ArraySortValue(value)
            | Expr::ArrayUniqueValue(value)
            | Expr::MapKeysValue(value)
            | Expr::MapValuesValue(value)
            | Expr::StringLenValue(value)
            | Expr::StringIsEmptyValue(value)
            | Expr::StringTrimValue(value)
            | Expr::StringTrimStartValue(value)
            | Expr::StringTrimEndValue(value)
            | Expr::StringToUpperValue(value)
            | Expr::StringToLowerValue(value)
            | Expr::PathBasenameValue(value)
            | Expr::PathDirnameValue(value)
            | Expr::PathStemValue(value)
            | Expr::PathExtnameValue(value)
            | Expr::PathIsAbsoluteValue(value)
            | Expr::ProcessEnv { name: value }
            | Expr::FsIsFile { path: value }
            | Expr::FsIsDir { path: value }
            | Expr::FsSize { path: value }
            | Expr::FsReadLines { path: value }
            | Expr::FsList { path: value }
            | Expr::JsonParse { value }
            | Expr::JsonStringifyValue { value }
            | Expr::Not(value)
            | Expr::BitNot(value)
            | Expr::Cast { expr: value, .. }
            | Expr::NewtypeCtor { value, .. } => {
                self.extract_nested_try_results(value, false, prefix);
            }
            Expr::Array(values)
            | Expr::Tuple(values)
            | Expr::Variant { args: values, .. }
            | Expr::Call { args: values, .. }
            | Expr::AllowedCommand { args: values, .. } => {
                for value in values {
                    self.extract_nested_try_results(value, false, prefix);
                }
            }
            Expr::Map(entries) => {
                for (key, value) in entries {
                    self.extract_nested_try_results(key, false, prefix);
                    self.extract_nested_try_results(value, false, prefix);
                }
            }
            Expr::Record(fields) => {
                for (_, value) in fields {
                    self.extract_nested_try_results(value, false, prefix);
                }
            }
            Expr::Pipeline { input, .. }
            | Expr::TryPipeline { input, .. }
            | Expr::PipelineResult { input, .. } => {
                if let Some(input) = input {
                    self.extract_nested_try_results(input, false, prefix);
                }
            }
            Expr::Index { index, .. }
            | Expr::ArrayMap { mapper: index, .. }
            | Expr::OptionMap { mapper: index, .. }
            | Expr::OptionFlatMap { mapper: index, .. }
            | Expr::ResultMap { mapper: index, .. }
            | Expr::ResultFlatMap { mapper: index, .. }
            | Expr::OptionAp { value: index, .. }
            | Expr::ResultAp { value: index, .. }
            | Expr::ArrayTake { count: index, .. }
            | Expr::ArrayDrop { count: index, .. }
            | Expr::Join {
                separator: index, ..
            }
            | Expr::ArrayPush { value: index, .. }
            | Expr::ArrayContains { value: index, .. }
            | Expr::ArrayIndexOf { value: index, .. }
            | Expr::MapHas { key: index, .. }
            | Expr::MapRemove { key: index, .. }
            | Expr::StringContains { needle: index, .. }
            | Expr::StringIndexOf { needle: index, .. }
            | Expr::StringStartsWith { prefix: index, .. }
            | Expr::StringEndsWith { suffix: index, .. }
            | Expr::StringRepeat { count: index, .. }
            | Expr::StringSplit {
                separator: index, ..
            } => {
                self.extract_nested_try_results(index, false, prefix);
            }
            Expr::IndexValue { value, index }
            | Expr::ArrayContainsValue { value, item: index }
            | Expr::ArrayIndexOfValue { value, item: index }
            | Expr::MapHasValue { value, key: index }
            | Expr::StringContainsValue {
                value,
                needle: index,
            }
            | Expr::StringIndexOfValue {
                value,
                needle: index,
            }
            | Expr::StringStartsWithValue {
                value,
                prefix: index,
            }
            | Expr::StringEndsWithValue {
                value,
                suffix: index,
            }
            | Expr::StringRepeatValue {
                value,
                count: index,
            }
            | Expr::StringSplitValue {
                value,
                separator: index,
            }
            | Expr::OptionApValue {
                function: value,
                value: index,
            }
            | Expr::ResultApValue {
                function: value,
                value: index,
            } => {
                self.extract_nested_try_results(value, false, prefix);
                self.extract_nested_try_results(index, false, prefix);
            }
            Expr::Slice { start, end, .. } | Expr::StringSlice { start, end, .. } => {
                self.extract_nested_try_results(start, false, prefix);
                self.extract_nested_try_results(end, false, prefix);
            }
            Expr::ArraySliceValue { value, start, end }
            | Expr::StringSliceValue { value, start, end } => {
                self.extract_nested_try_results(value, false, prefix);
                self.extract_nested_try_results(start, false, prefix);
                self.extract_nested_try_results(end, false, prefix);
            }
            Expr::TupleFieldValue { value, .. } | Expr::FieldValue { value, .. } => {
                self.extract_nested_try_results(value, false, prefix);
            }
            Expr::ArrayMapValue { value, mapper }
            | Expr::OptionMapValue { value, mapper }
            | Expr::OptionFlatMapValue { value, mapper }
            | Expr::ResultMapValue { value, mapper }
            | Expr::ResultFlatMapValue { value, mapper }
            | Expr::ArrayTakeValue {
                value,
                count: mapper,
            }
            | Expr::ArrayDropValue {
                value,
                count: mapper,
            }
            | Expr::JoinValue {
                value,
                separator: mapper,
            } => {
                self.extract_nested_try_results(value, false, prefix);
                self.extract_nested_try_results(mapper, false, prefix);
            }
            Expr::MapSet { key, value, .. }
            | Expr::FsWriteLines {
                path: key,
                lines: value,
            }
            | Expr::FsAppendLines {
                path: key,
                lines: value,
            } => {
                self.extract_nested_try_results(key, false, prefix);
                self.extract_nested_try_results(value, false, prefix);
            }
            Expr::StringReplace { from, to, .. } => {
                self.extract_nested_try_results(from, false, prefix);
                self.extract_nested_try_results(to, false, prefix);
            }
            Expr::StringReplaceValue { value, from, to } => {
                self.extract_nested_try_results(value, false, prefix);
                self.extract_nested_try_results(from, false, prefix);
                self.extract_nested_try_results(to, false, prefix);
            }
            Expr::Binary { left, op, right } if op.is_logical() => {
                self.extract_nested_try_results(left, false, prefix);
                if let Some(lifted) = self.lift_lazy_try_branch(right) {
                    let condition = std::mem::replace(left, Box::new(Expr::Bool(false)));
                    let fallback = Expr::Ok(Box::new(Expr::Bool(*op == crate::BinaryOp::Or)));
                    let result = if *op == crate::BinaryOp::And {
                        Expr::IfElse {
                            condition,
                            then_expr: Box::new(lifted),
                            else_expr: Box::new(fallback),
                        }
                    } else {
                        Expr::IfElse {
                            condition,
                            then_expr: Box::new(fallback),
                            else_expr: Box::new(lifted),
                        }
                    };
                    *expr = Expr::TryResult(Box::new(result));
                    self.extract_nested_try_results(expr, allow_root, prefix);
                }
            }
            Expr::Binary { left, right, .. } => {
                self.extract_nested_try_results(left, false, prefix);
                self.extract_nested_try_results(right, false, prefix);
            }
            Expr::IfElse {
                condition,
                then_expr,
                else_expr,
            } => {
                self.extract_nested_try_results(condition, false, prefix);
                let then_lifted = self.lift_lazy_try_branch(then_expr);
                let else_lifted = self.lift_lazy_try_branch(else_expr);
                if then_lifted.is_some() || else_lifted.is_some() {
                    let condition = std::mem::replace(condition, Box::new(Expr::Bool(false)));
                    let then_expr = then_lifted.unwrap_or_else(|| {
                        Expr::Ok(Box::new(*std::mem::replace(
                            then_expr,
                            Box::new(Expr::Unit),
                        )))
                    });
                    let else_expr = else_lifted.unwrap_or_else(|| {
                        Expr::Ok(Box::new(*std::mem::replace(
                            else_expr,
                            Box::new(Expr::Unit),
                        )))
                    });
                    *expr = Expr::TryResult(Box::new(Expr::IfElse {
                        condition,
                        then_expr: Box::new(then_expr),
                        else_expr: Box::new(else_expr),
                    }));
                    self.extract_nested_try_results(expr, allow_root, prefix);
                }
            }
            Expr::Match { value, arms } => {
                self.extract_nested_try_results(value, false, prefix);
                let mut lifted = Vec::with_capacity(arms.len());
                let mut any_lifted = false;
                for arm in arms.iter_mut() {
                    if let Some(guard) = &mut arm.guard {
                        if let Some(lifted_guard) = self.lift_lazy_try_branch(guard) {
                            *guard = Expr::MatchGuardResult(Box::new(lifted_guard));
                            any_lifted = true;
                        }
                    }
                    let arm_lifted = self.lift_lazy_try_branch(&mut arm.expr);
                    any_lifted |= arm_lifted.is_some();
                    lifted.push(arm_lifted);
                }
                if any_lifted {
                    for (arm, lifted) in arms.iter_mut().zip(lifted) {
                        arm.expr = lifted.unwrap_or_else(|| {
                            Expr::Ok(Box::new(std::mem::replace(&mut arm.expr, Expr::Unit)))
                        });
                    }
                    let value = std::mem::replace(value, Box::new(Expr::Unit));
                    let arms = std::mem::take(arms);
                    *expr = Expr::TryResult(Box::new(Expr::Match { value, arms }));
                    self.extract_nested_try_results(expr, allow_root, prefix);
                }
            }
            Expr::Default { value, fallback } => {
                self.extract_nested_try_results(value, false, prefix);
                if let Some(fallback) = self.lift_lazy_try_branch(fallback) {
                    let value = std::mem::replace(value, Box::new(Expr::Unit));
                    *expr = Expr::TryResult(Box::new(Expr::DefaultTry {
                        value,
                        fallback: Box::new(fallback),
                    }));
                    self.extract_nested_try_results(expr, allow_root, prefix);
                }
            }
            Expr::OptionOrElse { name, fallback } => {
                if let Some(fallback) = self.lift_lazy_try_branch(fallback) {
                    *expr = Expr::TryResult(Box::new(Expr::OptionOrElseTry {
                        value: Box::new(Expr::Ident(name.clone())),
                        fallback: Box::new(fallback),
                    }));
                    self.extract_nested_try_results(expr, allow_root, prefix);
                }
            }
            Expr::OptionOrElseValue { value, fallback } => {
                self.extract_nested_try_results(value, false, prefix);
                if let Some(fallback) = self.lift_lazy_try_branch(fallback) {
                    let value = std::mem::replace(value, Box::new(Expr::Unit));
                    *expr = Expr::TryResult(Box::new(Expr::OptionOrElseTry {
                        value,
                        fallback: Box::new(fallback),
                    }));
                    self.extract_nested_try_results(expr, allow_root, prefix);
                }
            }
            Expr::RecordPattern(_)
            | Expr::Lambda { .. }
            | Expr::Closure { .. }
            | Expr::Do { .. }
            | Expr::LetIn { .. }
            | Expr::DefaultTry { .. }
            | Expr::OptionOrElseTry { .. }
            | Expr::MatchGuardResult(_)
            | Expr::Int(_)
            | Expr::Float(_)
            | Expr::Bool(_)
            | Expr::String(_)
            | Expr::RawString(_)
            | Expr::Unit
            | Expr::None
            | Expr::Command { .. }
            | Expr::CommandResult { .. }
            | Expr::AsyncCommand(_)
            | Expr::Await(_)
            | Expr::HasCommand(_)
            | Expr::TupleField { .. }
            | Expr::Field { .. }
            | Expr::Value(_)
            | Expr::Len(_)
            | Expr::IsEmpty(_)
            | Expr::ArrayFirst(_)
            | Expr::ArrayLast(_)
            | Expr::ArrayReverse(_)
            | Expr::ArraySort(_)
            | Expr::ArrayUnique(_)
            | Expr::ArrayPop { .. }
            | Expr::MapKeys(_)
            | Expr::MapValues(_)
            | Expr::StringLen(_)
            | Expr::StringIsEmpty(_)
            | Expr::StringTrim(_)
            | Expr::StringTrimStart(_)
            | Expr::StringTrimEnd(_)
            | Expr::StringToUpper(_)
            | Expr::StringToLower(_)
            | Expr::PathBasename(_)
            | Expr::PathDirname(_)
            | Expr::PathStem(_)
            | Expr::PathExtname(_)
            | Expr::PathIsAbsolute(_)
            | Expr::EnvDefault { .. }
            | Expr::Env(_)
            | Expr::ProcessArgs
            | Expr::CliParse
            | Expr::JsonStringify { .. }
            | Expr::Ident(_)
            | Expr::TryResult(_) => {}
        }
    }

    fn lift_lazy_try_branch(&mut self, expr: &mut Expr) -> Option<Expr> {
        let mut prefix = Vec::new();
        self.extract_nested_try_results(expr, false, &mut prefix);
        if prefix.is_empty() {
            return None;
        }
        let steps = prefix
            .into_iter()
            .map(|statement| {
                let Statement::Const {
                    name,
                    expr: Expr::TryResult(value),
                    ..
                } = statement
                else {
                    unreachable!("nested try extraction only emits propagated const bindings");
                };
                let expr = match *value {
                    Expr::Command { command, .. } => Expr::CommandResult { command },
                    Expr::Pipeline { input, commands } | Expr::TryPipeline { input, commands } => {
                        Expr::PipelineResult { input, commands }
                    }
                    value => value,
                };
                DoStep::Bind { name, expr }
            })
            .collect();
        let result = std::mem::replace(expr, Expr::Unit);
        Some(Expr::Do {
            steps,
            result: Box::new(Expr::Call {
                name: "pure".to_string(),
                args: vec![result],
            }),
        })
    }

    fn check_and_lower_statement(
        &mut self,
        statement: &Statement,
        line: usize,
    ) -> Result<Statement, CompileError> {
        match statement {
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
            Statement::TypeAlias {
                name,
                type_params,
                ty,
            } => {
                self.define_type_alias(name, type_params, ty, line)?;
                Ok(statement.clone())
            }
            Statement::SumType { name, variants } => {
                self.define_sum_type(name, variants, line)?;
                Ok(statement.clone())
            }
            Statement::Newtype { name, base } => {
                self.define_newtype(name, base, line)?;
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
                    body: self.lower_function_body(type_params, params, return_type, body, line)?,
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
            Statement::Assign { name, expr } => {
                self.check_assignment(name, expr, line)?;
                let expected = self.bindings.get(name).map(|binding| binding.ty.clone());
                Ok(Statement::Assign {
                    name: name.clone(),
                    expr: if let Some(expected) = expected {
                        self.lower_expr_expected(expr, &expected, line)?
                    } else {
                        self.lower_expr(expr, line)?
                    },
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
            Statement::Break | Statement::Continue => Ok(statement.clone()),
            Statement::Return(expr) => {
                self.check_return(expr, line)?;
                let expected = self.expected_return.clone().ok_or_else(|| {
                    CompileError::new(line, "return is only valid inside a function".to_string())
                })?;
                Ok(Statement::Return(
                    self.lower_expr_expected(expr, &expected, line)?,
                ))
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
                let body = body_checker.check_and_lower_program(body)?;
                Ok(Statement::While { condition, body })
            }
            Statement::For {
                name,
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
                body_checker.define(name, *element_ty, false, line)?;
                let body = body_checker.check_and_lower_program(body)?;
                Ok(Statement::For {
                    name: name.clone(),
                    iterable,
                    body,
                })
            }
        }
    }

    fn check_statement(&mut self, statement: &Statement, line: usize) -> Result<(), CompileError> {
        match statement {
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
            Statement::TypeAlias {
                name,
                type_params,
                ty,
            } => self.define_type_alias(name, type_params, ty, line),
            Statement::SumType { name, variants } => self.define_sum_type(name, variants, line),
            Statement::Newtype { name, base } => self.define_newtype(name, base, line),
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
            Statement::Assign { name, expr } => self.check_assignment(name, expr, line),
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
            Statement::Break | Statement::Continue => Ok(()),
            Statement::Return(expr) => self.check_return(expr, line),
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
                name,
                iterable,
                body,
            } => self.check_for(name, iterable, body, line),
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
        body_checker.check_program(body)
    }

    fn check_for(
        &self,
        name: &str,
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
        body_checker.define(name, *element_ty, false, line)?;
        body_checker.check_program(body)
    }

    fn check_function(
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
        let backend_name = backend_function_name(name);
        if let Some(existing) = self
            .functions
            .keys()
            .find(|existing| backend_function_name(existing) == backend_name)
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
        body_checker.expected_return = Some(resolved_return.clone());
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

    fn check_constructor_override(
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

    fn define_external_function(
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

    fn lower_function_body(
        &self,
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
        body_checker.expected_return = Some(resolved_return.clone());
        for param in params {
            let ty = self.resolve_type_with_generics(&param.ty, &generic_names, line)?;
            body_checker.define(&param.name, ty, false, line)?;
        }
        let lowered = body_checker.check_and_lower_program(body)?;
        body_checker.lower_implicit_function_return(lowered, &resolved_return, line)
    }

    fn check_return(&self, expr: &Expr, line: usize) -> Result<(), CompileError> {
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

    fn check_try_result(&self, expr: &Expr, line: usize) -> Result<(), CompileError> {
        self.try_result_types(expr, line).map(|_| ())
    }

    fn check_try_result_expr(&self, expr: &Expr, line: usize) -> Result<Type, CompileError> {
        self.try_result_types(expr, line).map(|(ok_ty, _)| ok_ty)
    }

    fn try_result_types(&self, expr: &Expr, line: usize) -> Result<(Type, Type), CompileError> {
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

    fn program_has_return_or_implicit(
        &self,
        program: &Program,
        expected: &Type,
        line: usize,
    ) -> Result<bool, CompileError> {
        if program_has_return(program) {
            return Ok(true);
        }
        let Some(statement) = program.statements().last() else {
            return Ok(false);
        };
        let statement_line = program.statement_lines().last().copied().unwrap_or(line);
        self.statement_is_implicit_return(statement, expected, statement_line)
    }

    fn statement_is_implicit_return(
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

    fn return_expr_type(
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

    fn lower_implicit_function_return(
        &self,
        program: Program,
        expected: &Type,
        line: usize,
    ) -> Result<Program, CompileError> {
        if expected == &Type::Unit {
            return Ok(program);
        }
        if program_has_return(&program) {
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

    fn check_condition(&self, condition: &Expr, line: usize) -> Result<(), CompileError> {
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

    fn define_newtype(&mut self, name: &str, base: &Type, line: usize) -> Result<(), CompileError> {
        let resolved_base = self.resolve_type(base, line)?;
        if self.types.contains_key(name) || self.generic_types.contains_key(name) {
            return Err(CompileError::new(
                line,
                format!("type `{name}` is already defined"),
            ));
        }
        self.types.insert(
            name.to_string(),
            Type::Brand {
                name: name.to_string(),
                base: Box::new(resolved_base),
            },
        );
        Ok(())
    }

    fn define_sum_type(
        &mut self,
        name: &str,
        variants: &[VariantDecl],
        line: usize,
    ) -> Result<(), CompileError> {
        if self.types.contains_key(name) || self.generic_types.contains_key(name) {
            return Err(CompileError::new(
                line,
                format!("type `{name}` is already defined"),
            ));
        }
        if variants.is_empty() {
            return Err(CompileError::new(
                line,
                format!("sum type `{name}` requires at least one variant"),
            ));
        }
        let mut names = Vec::new();
        let mut seen = HashSet::new();
        let mut resolved_variants = Vec::new();
        for variant in variants {
            if !seen.insert(variant.name.clone()) {
                return Err(CompileError::new(
                    line,
                    format!("variant `{}` is already defined", variant.name),
                ));
            }
            if self.variants.contains_key(&variant.name)
                || self.functions.contains_key(&variant.name)
                || self.bindings.contains_key(&variant.name)
            {
                return Err(CompileError::new(
                    line,
                    format!(
                        "variant `{}` conflicts with an existing declaration",
                        variant.name
                    ),
                ));
            }
            let fields = variant
                .fields
                .iter()
                .map(|field| self.resolve_type(field, line))
                .collect::<Result<Vec<_>, _>>()?;
            names.push(variant.name.clone());
            resolved_variants.push((variant.name.clone(), fields));
        }
        self.types
            .insert(name.to_string(), Type::Named(name.to_string()));
        self.sum_types.insert(name.to_string(), names);
        for (variant, fields) in resolved_variants {
            self.variants.insert(
                variant,
                VariantSig {
                    sum_type: name.to_string(),
                    fields,
                },
            );
        }
        Ok(())
    }

    fn define_trait(
        &mut self,
        name: &str,
        type_param: &str,
        methods: &[TraitMethod],
        line: usize,
    ) -> Result<(), CompileError> {
        if self.traits.contains_key(name) {
            return Err(CompileError::new(
                line,
                format!("trait `{name}` is already defined"),
            ));
        }
        let mut seen_methods = HashSet::new();
        let generic_names = HashSet::from([type_param.to_string()]);
        let mut resolved_methods = Vec::new();
        for method in methods {
            if !seen_methods.insert(method.name.clone()) {
                return Err(CompileError::new(
                    line,
                    format!("trait method `{}` is already defined", method.name),
                ));
            }
            if method.params.is_empty() {
                return Err(CompileError::new(
                    line,
                    format!(
                        "trait method `{}` requires a receiver parameter",
                        method.name
                    ),
                ));
            }
            let receiver_ty =
                self.resolve_type_with_generics(&method.params[0].ty, &generic_names, line)?;
            if receiver_ty != Type::Generic(type_param.to_string()) {
                return Err(CompileError::new(
                    line,
                    format!(
                        "trait method `{}` receiver must be `{type_param}`",
                        method.name
                    ),
                ));
            }
            for param in &method.params {
                if param.variadic && param.name != method.params.last().unwrap().name {
                    return Err(CompileError::new(
                        line,
                        format!("trait method `{}` rest parameter must be last", method.name),
                    ));
                }
                if param.default.is_some() {
                    return Err(CompileError::new(
                        line,
                        format!(
                            "trait method `{}` cannot use default parameters",
                            method.name
                        ),
                    ));
                }
                self.resolve_type_with_generics(&param.ty, &generic_names, line)?;
            }
            self.resolve_type_with_generics(&method.return_type, &generic_names, line)?;
            let params = method
                .params
                .iter()
                .map(|param| {
                    Ok(Param {
                        name: param.name.clone(),
                        ty: self.resolve_type_with_generics(&param.ty, &generic_names, line)?,
                        default: None,
                        variadic: param.variadic,
                        capture_name: param.capture_name.clone(),
                    })
                })
                .collect::<Result<Vec<_>, CompileError>>()?;
            let return_type =
                self.resolve_type_with_generics(&method.return_type, &generic_names, line)?;
            resolved_methods.push(TraitMethod {
                name: method.name.clone(),
                params,
                return_type,
            });
        }
        self.traits.insert(
            name.to_string(),
            TraitSig {
                type_param: type_param.to_string(),
                methods: resolved_methods,
            },
        );
        Ok(())
    }

    fn define_trait_impl(
        &mut self,
        trait_name: &str,
        for_type: &Type,
        methods: &[ImplMethod],
        line: usize,
    ) -> Result<Vec<ImplMethod>, CompileError> {
        let Some(trait_sig) = self.traits.get(trait_name).cloned() else {
            return Err(CompileError::new(
                line,
                format!("unknown trait `{trait_name}`"),
            ));
        };
        let resolved = self.resolve_type(for_type, line)?;
        let key = (trait_name.to_string(), resolved.name());
        if !self.trait_impls.insert(key) {
            return Err(CompileError::new(
                line,
                format!(
                    "trait `{trait_name}` is already implemented for {}",
                    resolved.name()
                ),
            ));
        }
        self.check_impl_methods(trait_name, &trait_sig, &resolved, methods, line)
    }

    fn check_impl_methods(
        &mut self,
        trait_name: &str,
        trait_sig: &TraitSig,
        for_type: &Type,
        methods: &[ImplMethod],
        line: usize,
    ) -> Result<Vec<ImplMethod>, CompileError> {
        let mut seen_methods = HashSet::new();
        for method in methods {
            if !seen_methods.insert(method.name.clone()) {
                return Err(CompileError::new(
                    line,
                    format!("impl method `{}` is already defined", method.name),
                ));
            }
            let Some(expected) = trait_sig
                .methods
                .iter()
                .find(|candidate| candidate.name == method.name)
            else {
                return Err(CompileError::new(
                    line,
                    format!(
                        "impl method `{}` is not declared by trait `{trait_name}`",
                        method.name
                    ),
                ));
            };
            self.check_impl_method_signature(
                expected,
                method,
                &trait_sig.type_param,
                for_type,
                line,
            )?;
        }
        for expected in &trait_sig.methods {
            if !seen_methods.contains(&expected.name) {
                return Err(CompileError::new(
                    line,
                    format!(
                        "impl for trait `{trait_name}` is missing method `{}`",
                        expected.name
                    ),
                ));
            }
        }
        for method in methods {
            let lowered_name = impl_method_name(trait_name, for_type, &method.name);
            let key = (method.name.clone(), for_type.name());
            self.method_impls
                .entry(key)
                .or_default()
                .push((trait_name.to_string(), lowered_name.clone()));
            let params = &method.params;
            let return_type = &method.return_type;
            let body = &method.body;
            self.check_function(
                FunctionDecl {
                    name: &lowered_name,
                    override_constructor: false,
                    type_params: &[],
                    params,
                    return_type,
                    body,
                },
                line,
            )?;
        }
        methods
            .iter()
            .map(|method| {
                let params = &method.params;
                let return_type = &method.return_type;
                let method_body = &method.body;
                let body = self.lower_function_body(&[], params, return_type, method_body, line)?;
                Ok(ImplMethod {
                    name: impl_method_name(trait_name, for_type, &method.name),
                    params: method.params.clone(),
                    return_type: method.return_type.clone(),
                    body,
                })
            })
            .collect()
    }

    fn check_impl_method_signature(
        &self,
        expected: &TraitMethod,
        actual: &ImplMethod,
        type_param: &str,
        for_type: &Type,
        line: usize,
    ) -> Result<(), CompileError> {
        if expected.params.len() != actual.params.len() {
            return Err(CompileError::new(
                line,
                format!(
                    "impl method `{}` parameter count mismatch: expected {}, found {}",
                    actual.name,
                    expected.params.len(),
                    actual.params.len()
                ),
            ));
        }
        let inferred = HashMap::from([(type_param.to_string(), for_type.clone())]);
        for (expected, actual_param) in expected.params.iter().zip(&actual.params) {
            if expected.variadic != actual_param.variadic {
                return Err(CompileError::new(
                    line,
                    format!(
                        "impl method `{}` parameter `{}` rest modifier mismatch",
                        actual.name, actual_param.name
                    ),
                ));
            }
            let expected_ty = substitute_generics(&expected.ty, &inferred);
            let actual_ty = self.resolve_type(&actual_param.ty, line)?;
            if expected_ty != actual_ty {
                return Err(CompileError::new(
                    line,
                    format!(
                        "impl method `{}` parameter `{}` type mismatch: expected {}, found {}",
                        actual.name,
                        actual_param.name,
                        expected_ty.name(),
                        actual_ty.name()
                    ),
                ));
            }
        }
        let expected_return = substitute_generics(&expected.return_type, &inferred);
        let actual_return = self.resolve_type(&actual.return_type, line)?;
        if expected_return != actual_return {
            return Err(CompileError::new(
                line,
                format!(
                    "impl method `{}` return type mismatch: expected {}, found {}",
                    actual.name,
                    expected_return.name(),
                    actual_return.name()
                ),
            ));
        }
        Ok(())
    }

    fn define_type_alias(
        &mut self,
        name: &str,
        type_params: &[String],
        ty: &Type,
        line: usize,
    ) -> Result<(), CompileError> {
        let generic_names = type_params.iter().cloned().collect::<HashSet<_>>();
        let resolved_ty = self.resolve_type_with_generics(ty, &generic_names, line)?;
        if self.types.contains_key(name) {
            return Err(CompileError::new(
                line,
                format!("type `{name}` is already defined"),
            ));
        }
        if self.generic_types.contains_key(name) {
            return Err(CompileError::new(
                line,
                format!("type `{name}` is already defined"),
            ));
        }
        if type_params.is_empty() {
            self.types.insert(name.to_string(), resolved_ty);
        } else {
            self.generic_types
                .insert(name.to_string(), (type_params.to_vec(), resolved_ty));
        }
        Ok(())
    }

    fn define(
        &mut self,
        name: &str,
        ty: Type,
        mutable: bool,
        line: usize,
    ) -> Result<(), CompileError> {
        if name == "_" {
            return Ok(());
        }
        if self.bindings.contains_key(name) {
            return Err(CompileError::new(
                line,
                format!("variable `{name}` is already defined"),
            ));
        }
        if self.captured_bindings.is_some() {
            self.capture_params.insert(name.to_string());
        }
        self.bindings
            .insert(name.to_string(), Binding { ty, mutable });
        Ok(())
    }

    fn lookup_binding(&self, name: &str) -> Option<Binding> {
        let binding = self.bindings.get(name).cloned().or_else(|| {
            (name == "args").then(|| Binding {
                ty: Type::Array(Box::new(Type::String)),
                mutable: false,
            })
        });
        if binding.is_some() {
            self.record_capture(name);
        }
        binding
    }

    fn record_capture(&self, name: &str) {
        if !self.capture_params.contains(name) {
            if let Some(captures) = &self.captured_bindings {
                captures.borrow_mut().insert(name.to_string());
            }
        }
    }

    fn is_qualified_function_call(&self, receiver: &str, method: &str) -> bool {
        self.lookup_binding(receiver).is_none()
            && self.functions.contains_key(&format!("{receiver}.{method}"))
    }

    fn define_destructure(
        &mut self,
        pattern: &BindingPattern,
        ty: &Type,
        mutable: bool,
        line: usize,
    ) -> Result<(), CompileError> {
        match (pattern, ty) {
            (BindingPattern::Array { names, rest }, Type::Array(element)) => {
                for name in names {
                    self.define(name, (**element).clone(), mutable, line)?;
                }
                if let Some(rest) = rest {
                    self.define(rest, Type::Array(element.clone()), mutable, line)?;
                }
                Ok(())
            }
            (BindingPattern::Array { .. }, other) => Err(CompileError::new(
                line,
                format!(
                    "array destructuring requires array value, found {}",
                    other.name()
                ),
            )),
            (BindingPattern::Tuple(names), Type::Tuple(elements)) => {
                if names.len() != elements.len() {
                    return Err(CompileError::new(
                        line,
                        format!(
                            "tuple destructuring expected {} values, found {}",
                            names.len(),
                            elements.len()
                        ),
                    ));
                }
                for (name, ty) in names.iter().zip(elements) {
                    self.define(name, ty.clone(), mutable, line)?;
                }
                Ok(())
            }
            (BindingPattern::Tuple(_), other) => Err(CompileError::new(
                line,
                format!(
                    "tuple destructuring requires tuple value, found {}",
                    other.name()
                ),
            )),
            (BindingPattern::Record(bindings), Type::Record(fields)) => {
                for (field, name) in bindings {
                    let Some((_, ty)) = fields.iter().find(|(candidate, _)| candidate == field)
                    else {
                        return Err(CompileError::new(
                            line,
                            format!("record destructuring field `{field}` is missing"),
                        ));
                    };
                    self.define(name, ty.clone(), mutable, line)?;
                }
                Ok(())
            }
            (BindingPattern::Record(_), other) => Err(CompileError::new(
                line,
                format!(
                    "record destructuring requires record value, found {}",
                    other.name()
                ),
            )),
        }
    }

    fn ensure_destructurable_source(&self, expr: &Expr, line: usize) -> Result<(), CompileError> {
        match expr {
            Expr::Array(_)
            | Expr::Tuple(_)
            | Expr::Record(_)
            | Expr::Ident(_)
            | Expr::ProcessArgs => Ok(()),
            _ => Err(CompileError::new(
                line,
                "destructuring requires an array, tuple, or record literal or variable".to_string(),
            )),
        }
    }

    fn check_assignment(
        &mut self,
        name: &str,
        expr: &Expr,
        line: usize,
    ) -> Result<(), CompileError> {
        if name == "_" {
            self.check_expr(expr, line)?;
            return Ok(());
        }
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("cannot assign to undefined variable `{name}`"),
            ));
        };
        let expr_ty = self.check_expr_expected(expr, &binding.ty, line)?;
        if !binding.mutable {
            return Err(CompileError::new(
                line,
                format!("cannot assign to const `{name}`"),
            ));
        }
        if !self.is_assignable(&binding.ty, &expr_ty, expr) {
            return Err(CompileError::new(
                line,
                format!(
                    "type mismatch for `{name}`: expected {}, found {}",
                    binding.ty.name(),
                    expr_ty.name()
                ),
            ));
        }
        Ok(())
    }

    fn check_annotation(
        &self,
        annotation: Option<Type>,
        actual: Type,
        expr: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(expected) = annotation else {
            return Ok(actual);
        };
        let expected = self.resolve_type(&expected, line)?;
        if self.is_assignable(&expected, &actual, expr) {
            Ok(expected)
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "type annotation mismatch: expected {}, found {}",
                    expected.name(),
                    actual.name()
                ),
            ))
        }
    }

    fn binding_expr_type(
        &self,
        annotation: Option<&Type>,
        expr: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        if let Expr::TryResult(value) = expr {
            return self.check_try_result_expr(value, line);
        }
        if let Some(annotation) = annotation {
            if matches!(
                expr,
                Expr::Command { .. } | Expr::Pipeline { .. } | Expr::TryPipeline { .. }
            ) {
                let expected = self.resolve_type(annotation, line)?;
                if result_types(&expected).is_some() {
                    return Ok(command_result_type());
                }
            }
        }
        self.check_expr(expr, line)
    }

    fn check_expr_expected(
        &self,
        expr: &Expr,
        expected: &Type,
        line: usize,
    ) -> Result<Type, CompileError> {
        if let Expr::Lambda { params, body } = expr {
            return self.check_lambda(params, body, expected, line);
        }
        self.check_expr(expr, line)
    }

    fn lower_expr_expected(
        &self,
        expr: &Expr,
        expected: &Type,
        line: usize,
    ) -> Result<Expr, CompileError> {
        if let Expr::Lambda { params, body } = expr {
            return self.lower_lambda(params, body, expected, line);
        }
        self.lower_binding_expr(expr, expected, line)
    }

    fn check_lambda(
        &self,
        params: &[String],
        body: &Expr,
        expected: &Type,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Type::Function(param_types, return_type) = expected else {
            return Err(CompileError::new(
                line,
                format!("lambda requires a function type, found {}", expected.name()),
            ));
        };
        if params.len() != param_types.len() {
            return Err(CompileError::new(
                line,
                format!(
                    "lambda expects {} parameters from its function type, found {}",
                    param_types.len(),
                    params.len()
                ),
            ));
        }
        let (mut checker, _) = self.lambda_checker(params, param_types, line)?;
        checker.expected_return = Some((**return_type).clone());
        let body = Program::new(vec![Statement::Return(body.clone())], vec![line]);
        let body = checker.expand_nested_try_program(&body);
        if let Err(error) = checker.check_program(&body) {
            if let Some(message) = error.message().strip_prefix("return type mismatch: ") {
                return Err(CompileError::new(
                    error.line(),
                    format!("lambda return type mismatch: {message}"),
                ));
            }
            return Err(error);
        }
        Ok(expected.clone())
    }

    fn lower_lambda(
        &self,
        params: &[String],
        body: &Expr,
        expected: &Type,
        line: usize,
    ) -> Result<Expr, CompileError> {
        let Type::Function(param_types, return_type) = expected else {
            return Err(CompileError::new(
                line,
                format!("lambda requires a function type, found {}", expected.name()),
            ));
        };
        let (mut checker, captured_bindings) = self.lambda_checker(params, param_types, line)?;
        checker.expected_return = Some((**return_type).clone());
        let body = Program::new(vec![Statement::Return(body.clone())], vec![line]);
        let body = checker.check_and_lower_program(&body)?;
        let mut captures = captured_bindings
            .borrow()
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        captures.sort();
        for capture in &captures {
            self.record_capture(capture);
        }
        let mut index = self.next_lambda.get();
        let name = loop {
            let candidate = format!("__nacre_lambda_{index}");
            index += 1;
            if !self.functions.contains_key(&candidate) {
                break candidate;
            }
        };
        self.next_lambda.set(index);
        let mut capture_params = Vec::with_capacity(captures.len());
        let mut closure_captures = Vec::with_capacity(captures.len());
        for (capture_index, source) in captures.iter().enumerate() {
            let binding = self.bindings.get(source).cloned().or_else(|| {
                (source == "args").then(|| Binding {
                    ty: Type::Array(Box::new(Type::String)),
                    mutable: false,
                })
            });
            let Some(binding) = binding else {
                return Err(CompileError::new(
                    line,
                    format!("undefined captured variable `{source}`"),
                ));
            };
            let target = format!("__nacre_capture_{name}_{capture_index}");
            capture_params.push(Param {
                name: source.clone(),
                ty: binding.ty.clone(),
                default: None,
                variadic: false,
                capture_name: Some(target.clone()),
            });
            closure_captures.push(ClosureCapture {
                source: source.clone(),
                target,
                suffixes: capture_suffixes(&binding.ty),
            });
        }
        let params = capture_params
            .into_iter()
            .chain(params.iter().zip(param_types).map(|(name, ty)| Param {
                name: name.clone(),
                ty: ty.clone(),
                default: None,
                variadic: false,
                capture_name: None,
            }))
            .collect::<Vec<_>>();
        self.generated_functions.borrow_mut().push((
            Statement::Function {
                name: name.clone(),
                override_constructor: false,
                type_params: Vec::new(),
                params,
                return_type: (**return_type).clone(),
                body,
            },
            line,
        ));
        if captures.is_empty() {
            Ok(Expr::Ident(name))
        } else {
            Ok(Expr::Closure {
                name,
                captures: closure_captures,
            })
        }
    }

    fn lambda_checker(
        &self,
        params: &[String],
        param_types: &[Type],
        line: usize,
    ) -> Result<(Self, CapturedBindings), CompileError> {
        let mut checker = self.clone();
        checker.expected_return = None;
        let captured_bindings = Rc::new(RefCell::new(HashSet::new()));
        checker.captured_bindings = Some(captured_bindings.clone());
        checker.capture_params = params.iter().cloned().collect();
        for (name, ty) in params.iter().zip(param_types) {
            checker.bindings.remove(name);
            checker.define(name, ty.clone(), false, line)?;
        }
        Ok((checker, captured_bindings))
    }

    fn desugar_do(
        &self,
        steps: &[DoStep],
        result: &Expr,
        line: usize,
    ) -> Result<Expr, CompileError> {
        let family = self.do_family(steps, result, line)?;
        let result = match result {
            Expr::Call { name, args } if name == "pure" => {
                let [value] = args.as_slice() else {
                    return Err(CompileError::new(
                        line,
                        format!("pure expects 1 argument, found {}", args.len()),
                    ));
                };
                match family {
                    DoFamily::Option => Expr::Some(Box::new(value.clone())),
                    DoFamily::Result => Expr::Ok(Box::new(value.clone())),
                }
            }
            result => result.clone(),
        };
        Ok(steps.iter().rev().fold(result, |body, step| match step {
            DoStep::Bind { name, expr } => Expr::OptionFlatMapValue {
                value: Box::new(expr.clone()),
                mapper: Box::new(Expr::Lambda {
                    params: vec![name.clone()],
                    body: Box::new(body),
                }),
            },
            DoStep::Let {
                name,
                annotation,
                expr,
            } => Expr::LetIn {
                name: name.clone(),
                annotation: annotation.clone(),
                value: Box::new(expr.clone()),
                body: Box::new(body),
            },
        }))
    }

    fn do_family(
        &self,
        steps: &[DoStep],
        result: &Expr,
        line: usize,
    ) -> Result<DoFamily, CompileError> {
        let mut checker = self.clone();
        for step in steps {
            match step {
                DoStep::Let {
                    name,
                    annotation,
                    expr,
                } => {
                    let value_ty = checker.binding_expr_type(annotation.as_ref(), expr, line)?;
                    let binding_ty =
                        checker.check_annotation(annotation.clone(), value_ty, expr, line)?;
                    checker.define(name, binding_ty, false, line)?;
                }
                DoStep::Bind { expr, .. } => {
                    let ty = checker.check_expr(expr, line)?;
                    if option_element_type(&ty).is_some() {
                        return Ok(DoFamily::Option);
                    }
                    if result_types(&ty).is_some() {
                        return Ok(DoFamily::Result);
                    }
                    return Err(CompileError::new(
                        line,
                        format!("do binding expects Option or Result, found {}", ty.name()),
                    ));
                }
            }
        }
        if matches!(result, Expr::Call { name, .. } if name == "pure") {
            return Err(CompileError::new(
                line,
                "pure in do expression requires an Option or Result binding".to_string(),
            ));
        }
        let ty = checker.check_expr(result, line)?;
        if option_element_type(&ty).is_some() {
            Ok(DoFamily::Option)
        } else if result_types(&ty).is_some() {
            Ok(DoFamily::Result)
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "do expression result must be Option or Result, found {}",
                    ty.name()
                ),
            ))
        }
    }

    fn lower_binding_expr(
        &self,
        expr: &Expr,
        binding_ty: &Type,
        line: usize,
    ) -> Result<Expr, CompileError> {
        if let Some((ok_ty, _)) = result_types(binding_ty) {
            match expr {
                Expr::Command { command, .. } => {
                    return Ok(Expr::CommandResult {
                        command: command.clone(),
                    });
                }
                Expr::TryResult(value) => {
                    return Ok(Expr::TryResult(Box::new(
                        self.lower_try_result_value(value, line)?,
                    )));
                }
                Expr::Pipeline { input, commands } => {
                    return Ok(Expr::PipelineResult {
                        input: input
                            .as_ref()
                            .map(|input| self.lower_expr(input, line).map(Box::new))
                            .transpose()?,
                        commands: commands.clone(),
                    });
                }
                Expr::TryPipeline { input, commands } => {
                    return Ok(Expr::PipelineResult {
                        input: input
                            .as_ref()
                            .map(|input| self.lower_expr(input, line).map(Box::new))
                            .transpose()?,
                        commands: commands.clone(),
                    });
                }
                _ => {}
            }
            let actual = self.check_expr(expr, line)?;
            if !self.is_assignable(binding_ty, &actual, expr)
                && self.is_assignable(ok_ty, &actual, expr)
            {
                return Ok(Expr::Ok(Box::new(self.lower_expr(expr, line)?)));
            }
        }
        self.lower_expr(expr, line)
    }

    fn lower_try_result_value(&self, expr: &Expr, line: usize) -> Result<Expr, CompileError> {
        match expr {
            Expr::Command { command, .. } => Ok(Expr::CommandResult {
                command: command.clone(),
            }),
            Expr::Pipeline { input, commands } | Expr::TryPipeline { input, commands } => {
                Ok(Expr::PipelineResult {
                    input: input
                        .as_ref()
                        .map(|input| self.lower_expr(input, line).map(Box::new))
                        .transpose()?,
                    commands: commands.clone(),
                })
            }
            _ => self.lower_expr(expr, line),
        }
    }

    fn lower_expr(&self, expr: &Expr, line: usize) -> Result<Expr, CompileError> {
        match expr {
            Expr::AllowedCommand {
                group,
                command,
                args,
                ..
            } => {
                let policy = self.policy.command(group, command).ok_or_else(|| {
                    CompileError::new(
                        line,
                        format!("command `{group}.{command}` is not allowed by the execution policy"),
                    )
                })?;
                Ok(Expr::AllowedCommand {
                    group: group.clone(),
                    command: command.clone(),
                    args: args
                        .iter()
                        .map(|arg| self.lower_expr(arg, line))
                        .collect::<Result<Vec<_>, _>>()?,
                    program: Some(policy.program().to_string_lossy().into_owned()),
                    read_args: policy.read_args().to_vec(),
                    write_args: policy.write_args().to_vec(),
                })
            }
            Expr::Call { name, args } => {
                if let Some(variant) = self.variants.get(name) {
                    return Ok(Expr::Variant {
                        name: name.clone(),
                        args: args
                            .iter()
                            .map(|arg| self.lower_expr(arg, line))
                            .collect::<Result<Vec<_>, _>>()?,
                        field_types: variant.fields.clone(),
                    });
                }
                if let Some((trait_name, method)) = self.scoped_trait_method_parts(name) {
                    let lowered_name =
                        self.resolve_scoped_method_name(trait_name, method, args, line)?;
                    return Ok(Expr::Call {
                        args: self.lower_call_args(&lowered_name, args, line)?,
                        name: lowered_name,
                    });
                }
                if let Some((receiver, method)) = method_call_parts(name) {
                    if !self.functions.contains_key(name) {
                        let lowered_name = self.resolve_method_name(receiver, method, line)?;
                        let mut source_args = Vec::with_capacity(args.len() + 1);
                        source_args.push(Expr::Ident(receiver.to_string()));
                        source_args.extend_from_slice(args);
                        let lowered_args =
                            self.lower_call_args(&lowered_name, &source_args, line)?;
                        return Ok(Expr::Call {
                            name: lowered_name,
                            args: lowered_args,
                        });
                    }
                }
                Ok(Expr::Call {
                    name: name.clone(),
                    args: self.lower_call_args(name, args, line)?,
                })
            }
            Expr::Array(values) => Ok(Expr::Array(
                values
                    .iter()
                    .map(|value| self.lower_expr(value, line))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            Expr::Map(entries) => Ok(Expr::Map(
                entries
                    .iter()
                    .map(|(key, value)| {
                        Ok((self.lower_expr(key, line)?, self.lower_expr(value, line)?))
                    })
                    .collect::<Result<Vec<_>, CompileError>>()?,
            )),
            Expr::Record(fields) => Ok(Expr::Record(
                fields
                    .iter()
                    .map(|(name, value)| Ok((name.clone(), self.lower_expr(value, line)?)))
                    .collect::<Result<Vec<_>, CompileError>>()?,
            )),
            Expr::RecordPattern(fields) => Ok(Expr::RecordPattern(
                fields
                    .iter()
                    .map(|(name, value)| {
                        Ok((
                            name.clone(),
                            value
                                .as_ref()
                                .map(|value| self.lower_expr(value, line))
                                .transpose()?,
                        ))
                    })
                    .collect::<Result<Vec<_>, CompileError>>()?,
            )),
            Expr::Tuple(values) => Ok(Expr::Tuple(
                values
                    .iter()
                    .map(|value| self.lower_expr(value, line))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            Expr::Some(value) => Ok(Expr::Some(Box::new(self.lower_expr(value, line)?))),
            Expr::Ok(value) => Ok(Expr::Ok(Box::new(self.lower_expr(value, line)?))),
            Expr::Err(value) => Ok(Expr::Err(Box::new(self.lower_expr(value, line)?))),
            Expr::ResultOption(value) => {
                Ok(Expr::ResultOption(Box::new(self.lower_expr(value, line)?)))
            }
            Expr::TryResult(value) => Ok(Expr::TryResult(Box::new(
                self.lower_try_result_value(value, line)?,
            ))),
            Expr::Default { value, fallback } => Ok(Expr::Default {
                value: Box::new(self.lower_expr(value, line)?),
                fallback: Box::new(self.lower_expr(fallback, line)?),
            }),
            Expr::DefaultTry { value, fallback } => Ok(Expr::DefaultTry {
                value: Box::new(self.lower_expr(value, line)?),
                fallback: Box::new(self.lower_expr(fallback, line)?),
            }),
            Expr::Index { name, index } => Ok(Expr::Index {
                name: name.clone(),
                index: Box::new(self.lower_expr(index, line)?),
            }),
            Expr::IndexValue { value, index } => Ok(Expr::IndexValue {
                value: Box::new(self.lower_expr(value, line)?),
                index: Box::new(self.lower_expr(index, line)?),
            }),
            Expr::TupleFieldValue { value, field } => Ok(Expr::TupleFieldValue {
                value: Box::new(self.lower_expr(value, line)?),
                field: *field,
            }),
            Expr::FieldValue { value, field } => Ok(Expr::FieldValue {
                value: Box::new(self.lower_expr(value, line)?),
                field: field.clone(),
            }),
            Expr::Slice { name, start, end } => {
                let start = Box::new(self.lower_expr(start, line)?);
                let end = Box::new(self.lower_expr(end, line)?);
                if self
                    .lookup_binding(name)
                    .is_some_and(|binding| self.is_string_like(&binding.ty))
                {
                    Ok(Expr::StringSlice {
                        name: name.clone(),
                        start,
                        end,
                    })
                } else {
                    Ok(Expr::Slice {
                        name: name.clone(),
                        start,
                        end,
                    })
                }
            }
            Expr::ArraySliceValue { value, start, end } => Ok(Expr::ArraySliceValue {
                value: Box::new(self.lower_expr(value, line)?),
                start: Box::new(self.lower_expr(start, line)?),
                end: Box::new(self.lower_expr(end, line)?),
            }),
            Expr::Len(name) => Ok(self.lower_len(name)),
            Expr::ArrayLenValue(value) => {
                Ok(Expr::ArrayLenValue(Box::new(self.lower_expr(value, line)?)))
            }
            Expr::MapLenValue(value) => {
                Ok(Expr::MapLenValue(Box::new(self.lower_expr(value, line)?)))
            }
            Expr::IsEmpty(name) => Ok(self.lower_is_empty(name)),
            Expr::ArrayIsEmptyValue(value) => Ok(Expr::ArrayIsEmptyValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::MapIsEmptyValue(value) => Ok(Expr::MapIsEmptyValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::ArrayFirst(name) => Ok(Expr::ArrayFirst(name.clone())),
            Expr::ArrayFirstValue(value) => Ok(Expr::ArrayFirstValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::ArrayLast(name) => Ok(Expr::ArrayLast(name.clone())),
            Expr::ArrayLastValue(value) => Ok(Expr::ArrayLastValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::ArrayReverse(name) => Ok(Expr::ArrayReverse(name.clone())),
            Expr::ArrayReverseValue(value) => Ok(Expr::ArrayReverseValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::ArraySort(name) => Ok(Expr::ArraySort(name.clone())),
            Expr::ArraySortValue(value) => Ok(Expr::ArraySortValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::ArrayUnique(name) => Ok(Expr::ArrayUnique(name.clone())),
            Expr::ArrayUniqueValue(value) => Ok(Expr::ArrayUniqueValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::ArrayMap { name, mapper } => {
                if self.is_qualified_function_call(name, "map") {
                    return Ok(Expr::Call {
                        name: format!("{name}.map"),
                        args: vec![self.lower_expr(mapper, line)?],
                    });
                }
                if self
                    .lookup_binding(name)
                    .is_some_and(|binding| option_element_type(&binding.ty).is_some())
                {
                    let (element, mapped) = self.check_option_map(name, mapper, line)?;
                    return Ok(Expr::OptionMap {
                        name: name.clone(),
                        mapper: Box::new(self.lower_map_mapper(
                            mapper, &element, &mapped, line,
                        )?),
                    });
                }
                if self
                    .lookup_binding(name)
                    .is_some_and(|binding| result_types(&binding.ty).is_some())
                {
                    let (element, _, mapped) = self.check_result_map(name, mapper, line)?;
                    return Ok(Expr::ResultMap {
                        name: name.clone(),
                        mapper: Box::new(self.lower_map_mapper(
                            mapper, &element, &mapped, line,
                        )?),
                    });
                }
                let (element, mapped) = self.check_array_map(name, mapper, line)?;
                Ok(Expr::ArrayMap {
                    name: name.clone(),
                    mapper: Box::new(self.lower_map_mapper(
                        mapper, &element, &mapped, line,
                    )?),
                })
            }
            Expr::ArrayMapValue { value, mapper } => {
                let value_ty = self.check_expr(value, line)?;
                if option_element_type(&value_ty).is_some() {
                    let (element, mapped) = self.check_option_map_value(value, mapper, line)?;
                    return Ok(Expr::OptionMapValue {
                        value: Box::new(self.lower_expr(value, line)?),
                        mapper: Box::new(self.lower_map_mapper(
                            mapper, &element, &mapped, line,
                        )?),
                    });
                }
                if result_types(&value_ty).is_some() {
                    let (element, _, mapped) =
                        self.check_result_map_value(value, mapper, line)?;
                    return Ok(Expr::ResultMapValue {
                        value: Box::new(self.lower_expr(value, line)?),
                        mapper: Box::new(self.lower_map_mapper(
                            mapper, &element, &mapped, line,
                        )?),
                    });
                }
                let (element, mapped) = self.check_array_map_value(value, mapper, line)?;
                Ok(Expr::ArrayMapValue {
                    value: Box::new(self.lower_expr(value, line)?),
                    mapper: Box::new(self.lower_map_mapper(
                        mapper, &element, &mapped, line,
                    )?),
                })
            }
            Expr::OptionMap { name, mapper } => {
                let (element, mapped) = self.check_option_map(name, mapper, line)?;
                Ok(Expr::OptionMap {
                    name: name.clone(),
                    mapper: Box::new(self.lower_map_mapper(
                        mapper, &element, &mapped, line,
                    )?),
                })
            }
            Expr::OptionMapValue { value, mapper } => {
                let (element, mapped) = self.check_option_map_value(value, mapper, line)?;
                Ok(Expr::OptionMapValue {
                    value: Box::new(self.lower_expr(value, line)?),
                    mapper: Box::new(self.lower_map_mapper(
                        mapper, &element, &mapped, line,
                    )?),
                })
            }
            Expr::ResultMap { name, mapper } => {
                let (element, _, mapped) = self.check_result_map(name, mapper, line)?;
                Ok(Expr::ResultMap {
                    name: name.clone(),
                    mapper: Box::new(self.lower_map_mapper(
                        mapper, &element, &mapped, line,
                    )?),
                })
            }
            Expr::ResultMapValue { value, mapper } => {
                let (element, _, mapped) =
                    self.check_result_map_value(value, mapper, line)?;
                Ok(Expr::ResultMapValue {
                    value: Box::new(self.lower_expr(value, line)?),
                    mapper: Box::new(self.lower_map_mapper(
                        mapper, &element, &mapped, line,
                    )?),
                })
            }
            Expr::OptionFlatMap { name, mapper } => {
                if self.is_qualified_function_call(name, "flatMap") {
                    return Ok(Expr::Call {
                        name: format!("{name}.flatMap"),
                        args: vec![self.lower_expr(mapper, line)?],
                    });
                }
                if self
                    .lookup_binding(name)
                    .is_some_and(|binding| result_types(&binding.ty).is_some())
                {
                    let (element, mapper_result, _) =
                        self.check_result_flat_map(name, mapper, line)?;
                    return Ok(Expr::ResultFlatMap {
                        name: name.clone(),
                        mapper: Box::new(self.lower_map_mapper(
                            mapper,
                            &element,
                            &mapper_result,
                            line,
                        )?),
                    });
                }
                let (element, mapped) = self.check_option_flat_map(name, mapper, line)?;
                Ok(Expr::OptionFlatMap {
                    name: name.clone(),
                    mapper: Box::new(self.lower_map_mapper(
                        mapper, &element, &mapped, line,
                    )?),
                })
            }
            Expr::OptionFlatMapValue { value, mapper } => {
                let value_ty = self.check_expr(value, line)?;
                if result_types(&value_ty).is_some() {
                    let (element, mapper_result, _) =
                        self.check_result_flat_map_value(value, mapper, line)?;
                    return Ok(Expr::ResultFlatMapValue {
                        value: Box::new(self.lower_expr(value, line)?),
                        mapper: Box::new(self.lower_map_mapper(
                            mapper,
                            &element,
                            &mapper_result,
                            line,
                        )?),
                    });
                }
                let (element, mapped) = self.check_option_flat_map_value(value, mapper, line)?;
                Ok(Expr::OptionFlatMapValue {
                    value: Box::new(self.lower_expr(value, line)?),
                    mapper: Box::new(self.lower_map_mapper(
                        mapper, &element, &mapped, line,
                    )?),
                })
            }
            Expr::ResultFlatMap { name, mapper } => {
                let (element, mapper_result, _) =
                    self.check_result_flat_map(name, mapper, line)?;
                Ok(Expr::ResultFlatMap {
                    name: name.clone(),
                    mapper: Box::new(self.lower_map_mapper(
                        mapper,
                        &element,
                        &mapper_result,
                        line,
                    )?),
                })
            }
            Expr::ResultFlatMapValue { value, mapper } => {
                let (element, mapper_result, _) =
                    self.check_result_flat_map_value(value, mapper, line)?;
                Ok(Expr::ResultFlatMapValue {
                    value: Box::new(self.lower_expr(value, line)?),
                    mapper: Box::new(self.lower_map_mapper(
                        mapper,
                        &element,
                        &mapper_result,
                        line,
                    )?),
                })
            }
            Expr::OptionAp { name, value } => {
                if self.is_qualified_function_call(name, "ap") {
                    return Ok(Expr::Call {
                        name: format!("{name}.ap"),
                        args: vec![self.lower_expr(value, line)?],
                    });
                }
                if self
                    .lookup_binding(name)
                    .is_some_and(|binding| result_types(&binding.ty).is_some())
                {
                    self.check_result_ap(name, value, line)?;
                    return Ok(Expr::ResultAp {
                        name: name.clone(),
                        value: Box::new(self.lower_expr(value, line)?),
                    });
                }
                self.check_option_ap(name, value, line)?;
                Ok(Expr::OptionAp {
                    name: name.clone(),
                    value: Box::new(self.lower_expr(value, line)?),
                })
            }
            Expr::OptionApValue { function, value } => {
                let function_ty = self.check_expr(function, line)?;
                if result_types(&function_ty).is_some() {
                    self.check_result_ap_value(function, value, line)?;
                    return Ok(Expr::ResultApValue {
                        function: Box::new(self.lower_expr(function, line)?),
                        value: Box::new(self.lower_expr(value, line)?),
                    });
                }
                self.check_option_ap_value(function, value, line)?;
                Ok(Expr::OptionApValue {
                    function: Box::new(self.lower_expr(function, line)?),
                    value: Box::new(self.lower_expr(value, line)?),
                })
            }
            Expr::ResultAp { name, value } => {
                self.check_result_ap(name, value, line)?;
                Ok(Expr::ResultAp {
                    name: name.clone(),
                    value: Box::new(self.lower_expr(value, line)?),
                })
            }
            Expr::ResultApValue { function, value } => {
                self.check_result_ap_value(function, value, line)?;
                Ok(Expr::ResultApValue {
                    function: Box::new(self.lower_expr(function, line)?),
                    value: Box::new(self.lower_expr(value, line)?),
                })
            }
            Expr::OptionOrElse { name, fallback } => {
                if self.is_qualified_function_call(name, "orElse") {
                    return Ok(Expr::Call {
                        name: format!("{name}.orElse"),
                        args: vec![self.lower_expr(fallback, line)?],
                    });
                }
                self.check_option_or_else(name, fallback, line)?;
                Ok(Expr::OptionOrElse {
                    name: name.clone(),
                    fallback: Box::new(self.lower_expr(fallback, line)?),
                })
            }
            Expr::OptionOrElseValue { value, fallback } => {
                self.check_option_or_else_value(value, fallback, line)?;
                Ok(Expr::OptionOrElseValue {
                    value: Box::new(self.lower_expr(value, line)?),
                    fallback: Box::new(self.lower_expr(fallback, line)?),
                })
            }
            Expr::OptionOrElseTry { value, fallback } => {
                self.check_option_or_else_try(value, fallback, line)?;
                Ok(Expr::OptionOrElseTry {
                    value: Box::new(self.lower_expr(value, line)?),
                    fallback: Box::new(self.lower_expr(fallback, line)?),
                })
            }
            Expr::ArrayTake { name, count } => Ok(Expr::ArrayTake {
                name: name.clone(),
                count: Box::new(self.lower_expr(count, line)?),
            }),
            Expr::ArrayTakeValue { value, count } => Ok(Expr::ArrayTakeValue {
                value: Box::new(self.lower_expr(value, line)?),
                count: Box::new(self.lower_expr(count, line)?),
            }),
            Expr::ArrayDrop { name, count } => Ok(Expr::ArrayDrop {
                name: name.clone(),
                count: Box::new(self.lower_expr(count, line)?),
            }),
            Expr::ArrayDropValue { value, count } => Ok(Expr::ArrayDropValue {
                value: Box::new(self.lower_expr(value, line)?),
                count: Box::new(self.lower_expr(count, line)?),
            }),
            Expr::Join { name, separator } => Ok(Expr::Join {
                name: name.clone(),
                separator: Box::new(self.lower_expr(separator, line)?),
            }),
            Expr::JoinValue { value, separator } => Ok(Expr::JoinValue {
                value: Box::new(self.lower_expr(value, line)?),
                separator: Box::new(self.lower_expr(separator, line)?),
            }),
            Expr::ArrayPush { name, value } => Ok(Expr::ArrayPush {
                name: name.clone(),
                value: Box::new(self.lower_expr(value, line)?),
            }),
            Expr::ArrayPop { name } => Ok(Expr::ArrayPop { name: name.clone() }),
            Expr::MapSet { name, key, value } => {
                let key = self.lower_expr(key, line)?;
                let value = self.lower_expr(value, line)?;
                if self.is_qualified_function_call(name, "set") {
                    Ok(Expr::Call {
                        name: format!("{name}.set"),
                        args: vec![key, value],
                    })
                } else {
                    Ok(Expr::MapSet {
                        name: name.clone(),
                        key: Box::new(key),
                        value: Box::new(value),
                    })
                }
            }
            Expr::MapRemove { name, key } => {
                let key = self.lower_expr(key, line)?;
                if self.is_qualified_function_call(name, "remove") {
                    Ok(Expr::Call {
                        name: format!("{name}.remove"),
                        args: vec![key],
                    })
                } else {
                    Ok(Expr::MapRemove {
                        name: name.clone(),
                        key: Box::new(key),
                    })
                }
            }
            Expr::ArrayContains { name, value } => Ok(Expr::ArrayContains {
                name: name.clone(),
                value: Box::new(self.lower_expr(value, line)?),
            }),
            Expr::ArrayContainsValue { value, item } => Ok(Expr::ArrayContainsValue {
                value: Box::new(self.lower_expr(value, line)?),
                item: Box::new(self.lower_expr(item, line)?),
            }),
            Expr::ArrayIndexOf { name, value } => {
                let value = Box::new(self.lower_expr(value, line)?);
                if self
                    .lookup_binding(name)
                    .is_some_and(|binding| self.is_string_like(&binding.ty))
                {
                    Ok(Expr::StringIndexOf {
                        name: name.clone(),
                        needle: value,
                    })
                } else {
                    Ok(Expr::ArrayIndexOf {
                        name: name.clone(),
                        value,
                    })
                }
            }
            Expr::ArrayIndexOfValue { value, item } => Ok(Expr::ArrayIndexOfValue {
                value: Box::new(self.lower_expr(value, line)?),
                item: Box::new(self.lower_expr(item, line)?),
            }),
            Expr::MapKeys(name) => Ok(Expr::MapKeys(name.clone())),
            Expr::MapKeysValue(value) => {
                Ok(Expr::MapKeysValue(Box::new(self.lower_expr(value, line)?)))
            }
            Expr::MapValues(name) => Ok(Expr::MapValues(name.clone())),
            Expr::MapValuesValue(value) => Ok(Expr::MapValuesValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::MapHas { name, key } => Ok(Expr::MapHas {
                name: name.clone(),
                key: Box::new(self.lower_expr(key, line)?),
            }),
            Expr::MapHasValue { value, key } => Ok(Expr::MapHasValue {
                value: Box::new(self.lower_expr(value, line)?),
                key: Box::new(self.lower_expr(key, line)?),
            }),
            Expr::StringContains { name, needle } => {
                let needle = Box::new(self.lower_expr(needle, line)?);
                if matches!(
                    self.lookup_binding(name).map(|binding| binding.ty),
                    Some(Type::Array(_))
                ) {
                    Ok(Expr::ArrayContains {
                        name: name.clone(),
                        value: needle,
                    })
                } else {
                    Ok(Expr::StringContains {
                        name: name.clone(),
                        needle,
                    })
                }
            }
            Expr::StringContainsValue { value, needle } => Ok(Expr::StringContainsValue {
                value: Box::new(self.lower_expr(value, line)?),
                needle: Box::new(self.lower_expr(needle, line)?),
            }),
            Expr::StringIndexOf { name, needle } => Ok(Expr::StringIndexOf {
                name: name.clone(),
                needle: Box::new(self.lower_expr(needle, line)?),
            }),
            Expr::StringIndexOfValue { value, needle } => Ok(Expr::StringIndexOfValue {
                value: Box::new(self.lower_expr(value, line)?),
                needle: Box::new(self.lower_expr(needle, line)?),
            }),
            Expr::StringStartsWith { name, prefix } => Ok(Expr::StringStartsWith {
                name: name.clone(),
                prefix: Box::new(self.lower_expr(prefix, line)?),
            }),
            Expr::StringStartsWithValue { value, prefix } => Ok(Expr::StringStartsWithValue {
                value: Box::new(self.lower_expr(value, line)?),
                prefix: Box::new(self.lower_expr(prefix, line)?),
            }),
            Expr::StringEndsWith { name, suffix } => Ok(Expr::StringEndsWith {
                name: name.clone(),
                suffix: Box::new(self.lower_expr(suffix, line)?),
            }),
            Expr::StringEndsWithValue { value, suffix } => Ok(Expr::StringEndsWithValue {
                value: Box::new(self.lower_expr(value, line)?),
                suffix: Box::new(self.lower_expr(suffix, line)?),
            }),
            Expr::StringLen(name) => Ok(Expr::StringLen(name.clone())),
            Expr::StringLenValue(value) => Ok(Expr::StringLenValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::StringIsEmpty(name) => Ok(Expr::StringIsEmpty(name.clone())),
            Expr::StringIsEmptyValue(value) => Ok(Expr::StringIsEmptyValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::StringSlice { name, start, end } => Ok(Expr::StringSlice {
                name: name.clone(),
                start: Box::new(self.lower_expr(start, line)?),
                end: Box::new(self.lower_expr(end, line)?),
            }),
            Expr::StringSliceValue { value, start, end } => Ok(Expr::StringSliceValue {
                value: Box::new(self.lower_expr(value, line)?),
                start: Box::new(self.lower_expr(start, line)?),
                end: Box::new(self.lower_expr(end, line)?),
            }),
            Expr::StringTrim(name) => Ok(Expr::StringTrim(name.clone())),
            Expr::StringTrimValue(value) => Ok(Expr::StringTrimValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::StringTrimStart(name) => Ok(Expr::StringTrimStart(name.clone())),
            Expr::StringTrimStartValue(value) => Ok(Expr::StringTrimStartValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::StringTrimEnd(name) => Ok(Expr::StringTrimEnd(name.clone())),
            Expr::StringTrimEndValue(value) => Ok(Expr::StringTrimEndValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::StringToUpper(name) => Ok(Expr::StringToUpper(name.clone())),
            Expr::StringToUpperValue(value) => Ok(Expr::StringToUpperValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::StringToLower(name) => Ok(Expr::StringToLower(name.clone())),
            Expr::StringToLowerValue(value) => Ok(Expr::StringToLowerValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::StringRepeat { name, count } => Ok(Expr::StringRepeat {
                name: name.clone(),
                count: Box::new(self.lower_expr(count, line)?),
            }),
            Expr::StringRepeatValue { value, count } => Ok(Expr::StringRepeatValue {
                value: Box::new(self.lower_expr(value, line)?),
                count: Box::new(self.lower_expr(count, line)?),
            }),
            Expr::StringSplit { name, separator } => Ok(Expr::StringSplit {
                name: name.clone(),
                separator: Box::new(self.lower_expr(separator, line)?),
            }),
            Expr::StringSplitValue { value, separator } => Ok(Expr::StringSplitValue {
                value: Box::new(self.lower_expr(value, line)?),
                separator: Box::new(self.lower_expr(separator, line)?),
            }),
            Expr::StringReplace { name, from, to } => Ok(Expr::StringReplace {
                name: name.clone(),
                from: Box::new(self.lower_expr(from, line)?),
                to: Box::new(self.lower_expr(to, line)?),
            }),
            Expr::StringReplaceValue { value, from, to } => Ok(Expr::StringReplaceValue {
                value: Box::new(self.lower_expr(value, line)?),
                from: Box::new(self.lower_expr(from, line)?),
                to: Box::new(self.lower_expr(to, line)?),
            }),
            Expr::PathBasename(name) => Ok(Expr::PathBasename(name.clone())),
            Expr::PathBasenameValue(value) => Ok(Expr::PathBasenameValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::PathDirname(name) => Ok(Expr::PathDirname(name.clone())),
            Expr::PathDirnameValue(value) => Ok(Expr::PathDirnameValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::PathStem(name) => Ok(Expr::PathStem(name.clone())),
            Expr::PathStemValue(value) => {
                Ok(Expr::PathStemValue(Box::new(self.lower_expr(value, line)?)))
            }
            Expr::PathExtname(name) => Ok(Expr::PathExtname(name.clone())),
            Expr::PathExtnameValue(value) => Ok(Expr::PathExtnameValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::PathIsAbsolute(name) => Ok(Expr::PathIsAbsolute(name.clone())),
            Expr::PathIsAbsoluteValue(value) => Ok(Expr::PathIsAbsoluteValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::PathExists(path) => Ok(Expr::PathExists(Box::new(self.lower_expr(path, line)?))),
            Expr::ProcessEnv { name } => Ok(Expr::ProcessEnv {
                name: Box::new(self.lower_expr(name, line)?),
            }),
            Expr::FsIsFile { path } => Ok(Expr::FsIsFile {
                path: Box::new(self.lower_expr(path, line)?),
            }),
            Expr::FsIsDir { path } => Ok(Expr::FsIsDir {
                path: Box::new(self.lower_expr(path, line)?),
            }),
            Expr::FsSize { path } => Ok(Expr::FsSize {
                path: Box::new(self.lower_expr(path, line)?),
            }),
            Expr::FsReadLines { path } => Ok(Expr::FsReadLines {
                path: Box::new(self.lower_expr(path, line)?),
            }),
            Expr::FsList { path } => Ok(Expr::FsList {
                path: Box::new(self.lower_expr(path, line)?),
            }),
            Expr::FsWriteLines { path, lines } => Ok(Expr::FsWriteLines {
                path: Box::new(self.lower_expr(path, line)?),
                lines: Box::new(self.lower_expr(lines, line)?),
            }),
            Expr::FsAppendLines { path, lines } => Ok(Expr::FsAppendLines {
                path: Box::new(self.lower_expr(path, line)?),
                lines: Box::new(self.lower_expr(lines, line)?),
            }),
            Expr::JsonParse { value } => Ok(Expr::JsonParse {
                value: Box::new(self.lower_expr(value, line)?),
            }),
            Expr::JsonStringify { name } => Ok(Expr::JsonStringify { name: name.clone() }),
            Expr::JsonStringifyValue { value } => Ok(Expr::JsonStringifyValue {
                value: Box::new(self.lower_expr(value, line)?),
            }),
            Expr::NewtypeCtor { name, value } if self.constructor_overrides.contains(name) => {
                Ok(Expr::Call {
                    name: name.clone(),
                    args: vec![self.lower_expr(value, line)?],
                })
            }
            Expr::NewtypeCtor { name, value } if self.variants.contains_key(name) => {
                let field_types = self
                    .variants
                    .get(name)
                    .map(|variant| variant.fields.clone())
                    .unwrap_or_default();
                Ok(Expr::Variant {
                    name: name.clone(),
                    args: vec![self.lower_expr(value, line)?],
                    field_types,
                })
            }
            Expr::NewtypeCtor { name, value } => Ok(Expr::NewtypeCtor {
                name: name.clone(),
                value: Box::new(self.lower_expr(value, line)?),
            }),
            Expr::Variant {
                name,
                args,
                field_types,
            } => Ok(Expr::Variant {
                name: name.clone(),
                args: args
                    .iter()
                    .map(|arg| self.lower_expr(arg, line))
                    .collect::<Result<Vec<_>, _>>()?,
                field_types: field_types.clone(),
            }),
            Expr::Cast { expr, ty } => Ok(Expr::Cast {
                expr: Box::new(self.lower_expr(expr, line)?),
                ty: ty.clone(),
            }),
            Expr::Lambda { .. } => Err(CompileError::new(
                line,
                "lambda type cannot be inferred; provide a function type annotation or pass it to a typed function parameter".to_string(),
            )),
            Expr::Closure { name, captures } => Ok(Expr::Closure {
                name: name.clone(),
                captures: captures.clone(),
            }),
            Expr::Do { steps, result } => {
                let desugared = self.desugar_do(steps, result, line)?;
                self.lower_expr(&desugared, line)
            }
            Expr::LetIn {
                name,
                annotation,
                value,
                body,
            } => {
                let value_ty = self.binding_expr_type(annotation.as_ref(), value, line)?;
                let binding_ty =
                    self.check_annotation(annotation.clone(), value_ty, value, line)?;
                let value = self.lower_expr_expected(value, &binding_ty, line)?;
                let mut body_checker = self.clone();
                body_checker.define(name, binding_ty.clone(), false, line)?;
                Ok(Expr::LetIn {
                    name: name.clone(),
                    annotation: Some(binding_ty),
                    value: Box::new(value),
                    body: Box::new(body_checker.lower_expr(body, line)?),
                })
            }
            Expr::IfElse {
                condition,
                then_expr,
                else_expr,
            } => Ok(Expr::IfElse {
                condition: Box::new(self.lower_expr(condition, line)?),
                then_expr: Box::new(self.lower_expr(then_expr, line)?),
                else_expr: Box::new(self.lower_expr(else_expr, line)?),
            }),
            Expr::Match { value, arms } => {
                let value_ty =
                    if matches!(value.as_ref(), Expr::Command { .. } | Expr::Pipeline { .. }) {
                        command_result_type()
                    } else {
                        self.check_expr(value, line)?
                    };
                let mut lowered_arms = Vec::with_capacity(arms.len());
                for arm in arms {
                    let mut arm_checker = self.clone();
                    if let Some(pattern) = &arm.pattern {
                        for (name, ty) in
                            self.check_match_pattern(pattern, value, &value_ty, line)?
                        {
                            arm_checker.define(&name, ty, false, line)?;
                        }
                    }
                    lowered_arms.push(MatchArm {
                        pattern: arm
                            .pattern
                            .as_ref()
                            .map(|pattern| self.lower_expr(pattern, line))
                            .transpose()?,
                        guard: arm
                            .guard
                            .as_ref()
                            .map(|guard| arm_checker.lower_expr(guard, line))
                            .transpose()?,
                        expr: arm_checker.lower_expr(&arm.expr, line)?,
                    });
                }
                Ok(Expr::Match {
                    value: Box::new(self.lower_expr(value, line)?),
                    arms: lowered_arms,
                })
            }
            Expr::MatchGuardResult(value) => {
                Ok(Expr::MatchGuardResult(Box::new(self.lower_expr(value, line)?)))
            }
            Expr::Not(expr) => Ok(Expr::Not(Box::new(self.lower_expr(expr, line)?))),
            Expr::BitNot(expr) => Ok(Expr::BitNot(Box::new(self.lower_expr(expr, line)?))),
            Expr::Binary { left, op, right } => Ok(Expr::Binary {
                left: Box::new(self.lower_expr(left, line)?),
                op: *op,
                right: Box::new(self.lower_expr(right, line)?),
            }),
            Expr::Ident(name)
                if self
                    .variants
                    .get(name)
                    .is_some_and(|variant| variant.fields.is_empty()) =>
            {
                Ok(Expr::Variant {
                    name: name.clone(),
                    args: Vec::new(),
                    field_types: Vec::new(),
                })
            }
            Expr::Int(_)
            | Expr::Float(_)
            | Expr::Bool(_)
            | Expr::String(_)
            | Expr::RawString(_)
            | Expr::Unit
            | Expr::None
            | Expr::Command { .. }
            | Expr::CommandResult { .. }
            | Expr::AsyncCommand(_)
            | Expr::Await(_)
            | Expr::Pipeline { .. }
            | Expr::TryPipeline { .. }
            | Expr::PipelineResult { .. }
            | Expr::HasCommand(_)
            | Expr::TupleField { .. }
            | Expr::Value(_)
            | Expr::EnvDefault { .. }
            | Expr::Env(_)
            | Expr::ProcessArgs
            | Expr::CliParse
            | Expr::Ident(_) => Ok(expr.clone()),
            Expr::Field { name, field } if !self.bindings.contains_key(name) => {
                let qualified = format!("{name}.{field}");
                if self.functions.contains_key(&qualified) {
                    Ok(Expr::Ident(qualified))
                } else {
                    Ok(expr.clone())
                }
            }
            Expr::Field { .. } => Ok(expr.clone()),
        }
    }

    fn lower_call_args(
        &self,
        name: &str,
        args: &[Expr],
        line: usize,
    ) -> Result<Vec<Expr>, CompileError> {
        if let Some(sig) = self
            .functions
            .get(name)
            .filter(|sig| sig.type_params.is_empty())
        {
            return args
                .iter()
                .enumerate()
                .map(|(index, arg)| {
                    sig.params
                        .get(index)
                        .map(|param| self.lower_expr_expected(arg, &param.ty, line))
                        .unwrap_or_else(|| self.lower_expr(arg, line))
                })
                .collect();
        }
        if let Some(Binding {
            ty: Type::Function(params, _),
            ..
        }) = self.bindings.get(name)
        {
            return args
                .iter()
                .enumerate()
                .map(|(index, arg)| {
                    params
                        .get(index)
                        .map(|expected| self.lower_expr_expected(arg, expected, line))
                        .unwrap_or_else(|| self.lower_expr(arg, line))
                })
                .collect();
        }
        args.iter().map(|arg| self.lower_expr(arg, line)).collect()
    }

    fn resolve_method_name(
        &self,
        receiver: &str,
        method: &str,
        line: usize,
    ) -> Result<String, CompileError> {
        let receiver_ty = self.check_expr(&Expr::Ident(receiver.to_string()), line)?;
        if let Some(candidates) = self
            .method_impls
            .get(&(method.to_string(), receiver_ty.name()))
        {
            if candidates.len() == 1 {
                return Ok(candidates[0].1.clone());
            }
            let traits = candidates
                .iter()
                .map(|(trait_name, _)| trait_name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(CompileError::new(
                line,
                format!(
                    "ambiguous method `{method}` for {}; use one of: {traits}",
                    receiver_ty.name()
                ),
            ));
        }
        if self.functions.contains_key(method) {
            return Ok(method.to_string());
        }
        Err(CompileError::new(
            line,
            format!("undefined function `{method}`"),
        ))
    }

    fn resolve_scoped_method_name(
        &self,
        trait_name: &str,
        method: &str,
        args: &[Expr],
        line: usize,
    ) -> Result<String, CompileError> {
        let Some(receiver) = args.first() else {
            return Err(CompileError::new(
                line,
                format!("trait method `{trait_name}.{method}` requires a receiver argument"),
            ));
        };
        let receiver_ty = self.check_expr(receiver, line)?;
        let Some(candidates) = self
            .method_impls
            .get(&(method.to_string(), receiver_ty.name()))
        else {
            return Err(CompileError::new(
                line,
                format!(
                    "type {} does not implement trait `{trait_name}` method `{method}`",
                    receiver_ty.name()
                ),
            ));
        };
        candidates
            .iter()
            .find(|(candidate_trait, _)| candidate_trait == trait_name)
            .map(|(_, lowered_name)| lowered_name.clone())
            .ok_or_else(|| {
                CompileError::new(
                    line,
                    format!(
                        "type {} does not implement trait `{trait_name}` method `{method}`",
                        receiver_ty.name()
                    ),
                )
            })
    }

    fn scoped_trait_method_parts<'a>(&self, name: &'a str) -> Option<(&'a str, &'a str)> {
        let (trait_name, method) = name.rsplit_once('.')?;
        if self.traits.contains_key(trait_name) && is_valid_name(method) {
            Some((trait_name, method))
        } else {
            None
        }
    }

    fn check_expr(&self, expr: &Expr, line: usize) -> Result<Type, CompileError> {
        match expr {
            Expr::Int(_) => Ok(Type::Int),
            Expr::Float(_) => Ok(Type::Float),
            Expr::Bool(_) => Ok(Type::Bool),
            Expr::Unit => Ok(Type::Unit),
            Expr::Some(value) => Ok(Type::Applied(
                "Option".to_string(),
                vec![self.check_expr(value, line)?],
            )),
            Expr::None => Ok(Type::Applied("Option".to_string(), vec![Type::Unit])),
            Expr::Ok(value) => Ok(Type::Applied(
                "Result".to_string(),
                vec![self.check_expr(value, line)?, Type::Unit],
            )),
            Expr::Err(value) => Ok(Type::Applied(
                "Result".to_string(),
                vec![Type::Unit, self.check_expr(value, line)?],
            )),
            Expr::ResultOption(value) => self.check_result_option(value, line),
            Expr::TryResult(_) => Err(CompileError::new(
                line,
                "try propagation requires a Result-returning function or lambda".to_string(),
            )),
            Expr::Default { value, fallback } => self.check_default(value, fallback, line),
            Expr::DefaultTry { value, fallback } => {
                self.check_default_try(value, fallback, line)
            }
            Expr::String(value) => {
                self.check_string_interpolations(value, line)?;
                Ok(Type::String)
            }
            Expr::AllowedCommand {
                group,
                command,
                args,
                ..
            } => self.check_allowed_command(group, command, args, line),
            Expr::HasCommand(_) => Ok(Type::Bool),
            Expr::PathExists(path) => {
                self.require_read_access("pathExists", line)?;
                self.check_path_exists(path, line)
            }
            Expr::ProcessEnv { name } => self.check_process_env(name, line),
            Expr::JsonParse { value } => self.check_json_parse(value, line),
            Expr::JsonStringify { name } => self.check_json_stringify(name, line),
            Expr::JsonStringifyValue { value } => self.check_json_stringify_value(value, line),
            Expr::Array(values) => self.check_array(values, line),
            Expr::Map(entries) => self.check_map(entries, line),
            Expr::Record(fields) => self.check_record(fields, line),
            Expr::RecordPattern(_) => Err(CompileError::new(
                line,
                "record patterns are only valid in match arms".to_string(),
            )),
            Expr::Tuple(values) => self.check_tuple(values, line),
            Expr::Index { name, index } => self.check_index(name, index, line),
            Expr::IndexValue { value, index } => self.check_index_value(value, index, line),
            Expr::Slice { name, start, end } | Expr::StringSlice { name, start, end } => {
                self.check_slice(name, start, end, line)
            }
            Expr::ArraySliceValue { value, start, end } => {
                self.check_array_slice_value(value, start, end, line)
            }
            Expr::StringSliceValue { value, start, end } => {
                self.check_string_slice_value(value, start, end, line)
            }
            Expr::TupleField { name, field } => self.check_tuple_field(name, *field, line),
            Expr::TupleFieldValue { value, field } => {
                self.check_tuple_field_value(value, *field, line)
            }
            Expr::Field { name, field } => self.check_field(name, field, line),
            Expr::FieldValue { value, field } => self.check_field_value(value, field, line),
            Expr::NewtypeCtor { name, value } if self.variants.contains_key(name) => {
                self.check_variant(name, std::slice::from_ref(value.as_ref()), line)
            }
            Expr::NewtypeCtor { name, value } => self.check_newtype_ctor(name, value, line),
            Expr::Variant { name, args, .. } => self.check_variant(name, args, line),
            Expr::Cast { expr, ty } => self.check_cast(expr, ty, line),
            Expr::Lambda { .. } => Err(CompileError::new(
                line,
                "lambda type cannot be inferred; provide a function type annotation or pass it to a typed function parameter".to_string(),
            )),
            Expr::Closure { .. } => Err(CompileError::new(
                line,
                "internal closure expression cannot appear in source".to_string(),
            )),
            Expr::Do { steps, result } => {
                let desugared = self.desugar_do(steps, result, line)?;
                self.check_expr(&desugared, line)
            }
            Expr::LetIn {
                name,
                annotation,
                value,
                body,
            } => {
                let value_ty = self.binding_expr_type(annotation.as_ref(), value, line)?;
                let binding_ty =
                    self.check_annotation(annotation.clone(), value_ty, value, line)?;
                let mut body_checker = self.clone();
                body_checker.define(name, binding_ty, false, line)?;
                body_checker.check_expr(body, line)
            }
            Expr::Call { name, args } if self.variants.contains_key(name) => {
                self.check_variant(name, args, line)
            }
            Expr::Call { name, args } => self.check_call(name, args, line),
            Expr::Value(name) => self.check_value_access(name, line),
            Expr::Len(name) | Expr::StringLen(name) => self.check_len(name, line),
            Expr::ArrayLenValue(value) => self
                .check_array_value(value, "len", line)
                .map(|_| Type::Int),
            Expr::MapLenValue(value) => self.check_map_value(value, "len", line).map(|_| Type::Int),
            Expr::StringLenValue(value) => self
                .check_string_transform_value(value, "len", line)
                .map(|_| Type::Int),
            Expr::IsEmpty(name) | Expr::StringIsEmpty(name) => self.check_is_empty(name, line),
            Expr::ArrayIsEmptyValue(value) => self
                .check_array_value(value, "isEmpty", line)
                .map(|_| Type::Bool),
            Expr::MapIsEmptyValue(value) => self
                .check_map_value(value, "isEmpty", line)
                .map(|_| Type::Bool),
            Expr::StringIsEmptyValue(value) => self
                .check_string_transform_value(value, "isEmpty", line)
                .map(|_| Type::Bool),
            Expr::ArrayFirst(name) => self.check_array_edge(name, "first", line),
            Expr::ArrayFirstValue(value) => self.check_array_edge_value(value, "first", line),
            Expr::ArrayLast(name) => self.check_array_edge(name, "last", line),
            Expr::ArrayLastValue(value) => self.check_array_edge_value(value, "last", line),
            Expr::ArrayReverse(name) => self.check_array_reverse(name, line),
            Expr::ArrayReverseValue(value) => {
                self.check_array_transform_value(value, "reverse", line)
            }
            Expr::ArraySort(name) => self.check_array_sort(name, line),
            Expr::ArraySortValue(value) => self.check_array_sort_value(value, line),
            Expr::ArrayUnique(name) => self.check_array_unique(name, line),
            Expr::ArrayUniqueValue(value) => {
                self.check_array_transform_value(value, "unique", line)
            }
            Expr::ArrayMap { name, mapper } => {
                if self.is_qualified_function_call(name, "map") {
                    self.check_call(&format!("{name}.map"), std::slice::from_ref(mapper), line)
                } else if self
                    .lookup_binding(name)
                    .is_some_and(|binding| option_element_type(&binding.ty).is_some())
                {
                    self.check_option_map(name, mapper, line).map(
                        |(_, mapped)| Type::Applied("Option".to_string(), vec![mapped]),
                    )
                } else if self
                    .lookup_binding(name)
                    .is_some_and(|binding| result_types(&binding.ty).is_some())
                {
                    self.check_result_map(name, mapper, line)
                        .map(|(_, error, mapped)| {
                            Type::Applied("Result".to_string(), vec![mapped, error])
                        })
                } else {
                    self.check_array_map(name, mapper, line)
                        .map(|(_, mapped)| Type::Array(Box::new(mapped)))
                }
            }
            Expr::ArrayMapValue { value, mapper } => {
                let value_ty = self.check_expr(value, line)?;
                if option_element_type(&value_ty).is_some() {
                    self.check_option_map_value(value, mapper, line).map(
                        |(_, mapped)| Type::Applied("Option".to_string(), vec![mapped]),
                    )
                } else if result_types(&value_ty).is_some() {
                    self.check_result_map_value(value, mapper, line)
                        .map(|(_, error, mapped)| {
                            Type::Applied("Result".to_string(), vec![mapped, error])
                        })
                } else {
                    self.check_array_map_value(value, mapper, line)
                        .map(|(_, mapped)| Type::Array(Box::new(mapped)))
                }
            }
            Expr::OptionMap { name, mapper } => self
                .check_option_map(name, mapper, line)
                .map(|(_, mapped)| Type::Applied("Option".to_string(), vec![mapped])),
            Expr::OptionMapValue { value, mapper } => self
                .check_option_map_value(value, mapper, line)
                .map(|(_, mapped)| Type::Applied("Option".to_string(), vec![mapped])),
            Expr::ResultMap { name, mapper } => self
                .check_result_map(name, mapper, line)
                .map(|(_, error, mapped)| {
                    Type::Applied("Result".to_string(), vec![mapped, error])
                }),
            Expr::ResultMapValue { value, mapper } => self
                .check_result_map_value(value, mapper, line)
                .map(|(_, error, mapped)| {
                    Type::Applied("Result".to_string(), vec![mapped, error])
                }),
            Expr::OptionFlatMap { name, mapper } => {
                if self.is_qualified_function_call(name, "flatMap") {
                    self.check_call(
                        &format!("{name}.flatMap"),
                        std::slice::from_ref(mapper),
                        line,
                    )
                } else if self
                    .lookup_binding(name)
                    .is_some_and(|binding| result_types(&binding.ty).is_some())
                {
                    self.check_result_flat_map(name, mapper, line)
                        .map(|(_, _, result)| result)
                } else {
                    self.check_option_flat_map(name, mapper, line)
                        .map(|(_, mapped)| mapped)
                }
            }
            Expr::OptionFlatMapValue { value, mapper } => {
                let value_ty = self.check_expr(value, line)?;
                if result_types(&value_ty).is_some() {
                    self.check_result_flat_map_value(value, mapper, line)
                        .map(|(_, _, result)| result)
                } else {
                    self.check_option_flat_map_value(value, mapper, line)
                        .map(|(_, mapped)| mapped)
                }
            }
            Expr::ResultFlatMap { name, mapper } => self
                .check_result_flat_map(name, mapper, line)
                .map(|(_, _, result)| result),
            Expr::ResultFlatMapValue { value, mapper } => self
                .check_result_flat_map_value(value, mapper, line)
                .map(|(_, _, result)| result),
            Expr::OptionAp { name, value } => {
                if self.is_qualified_function_call(name, "ap") {
                    self.check_call(&format!("{name}.ap"), std::slice::from_ref(value), line)
                } else if self
                    .lookup_binding(name)
                    .is_some_and(|binding| result_types(&binding.ty).is_some())
                {
                    self.check_result_ap(name, value, line)
                } else {
                    self.check_option_ap(name, value, line)
                }
            }
            Expr::OptionApValue { function, value } => {
                let function_ty = self.check_expr(function, line)?;
                if result_types(&function_ty).is_some() {
                    self.check_result_ap_value(function, value, line)
                } else {
                    self.check_option_ap_value(function, value, line)
                }
            }
            Expr::ResultAp { name, value } => self.check_result_ap(name, value, line),
            Expr::ResultApValue { function, value } => {
                self.check_result_ap_value(function, value, line)
            }
            Expr::OptionOrElse { name, fallback } => {
                if self.is_qualified_function_call(name, "orElse") {
                    self.check_call(
                        &format!("{name}.orElse"),
                        std::slice::from_ref(fallback),
                        line,
                    )
                } else {
                    self.check_option_or_else(name, fallback, line)
                }
            }
            Expr::OptionOrElseValue { value, fallback } => {
                self.check_option_or_else_value(value, fallback, line)
            }
            Expr::OptionOrElseTry { value, fallback } => {
                self.check_option_or_else_try(value, fallback, line)
            }
            Expr::ArrayTake { name, count } => self.check_array_count(name, count, "take", line),
            Expr::ArrayTakeValue { value, count } => {
                self.check_array_count_value(value, count, "take", line)
            }
            Expr::ArrayDrop { name, count } => self.check_array_count(name, count, "drop", line),
            Expr::ArrayDropValue { value, count } => {
                self.check_array_count_value(value, count, "drop", line)
            }
            Expr::Join { name, separator } => self.check_join(name, separator, line),
            Expr::JoinValue { value, separator } => self.check_join_value(value, separator, line),
            Expr::ArrayPush { .. } => Err(CompileError::new(
                line,
                "push is only valid as a statement".to_string(),
            )),
            Expr::ArrayPop { .. } => Err(CompileError::new(
                line,
                "pop is only valid as a statement".to_string(),
            )),
            Expr::ArrayContains { name, value } => self.check_array_contains(name, value, line),
            Expr::ArrayContainsValue { value, item } => {
                self.check_array_contains_value(value, item, line)
            }
            Expr::ArrayIndexOf { name, value } => self.check_index_of(name, value, line),
            Expr::ArrayIndexOfValue { value, item } => {
                self.check_array_index_of_value(value, item, line)
            }
            Expr::MapKeys(name) => self.check_map_keys(name, line),
            Expr::MapKeysValue(value) => self.check_map_keys_value(value, line),
            Expr::MapValues(name) => self.check_map_values(name, line),
            Expr::MapValuesValue(value) => self.check_map_values_value(value, line),
            Expr::MapHas { name, key } => self.check_map_has(name, key, line),
            Expr::MapHasValue { value, key } => self.check_map_has_value(value, key, line),
            Expr::MapSet { name, key, value } => {
                if self.is_qualified_function_call(name, "set") {
                    self.check_call(
                        &format!("{name}.set"),
                        &[key.as_ref().clone(), value.as_ref().clone()],
                        line,
                    )
                } else {
                    Err(CompileError::new(
                        line,
                        "set is only valid as a statement".to_string(),
                    ))
                }
            }
            Expr::MapRemove { name, key } => {
                if self.is_qualified_function_call(name, "remove") {
                    self.check_call(
                        &format!("{name}.remove"),
                        std::slice::from_ref(key.as_ref()),
                        line,
                    )
                } else {
                    Err(CompileError::new(
                        line,
                        "remove is only valid as a statement".to_string(),
                    ))
                }
            }
            Expr::StringContains { name, needle } => self.check_string_contains(name, needle, line),
            Expr::StringContainsValue { value, needle } => {
                self.check_string_predicate_value(value, needle, "contains", "needle", line)
            }
            Expr::StringIndexOf { name, needle } => self.check_string_index_of(name, needle, line),
            Expr::StringIndexOfValue { value, needle } => {
                self.check_string_index_of_value(value, needle, line)
            }
            Expr::StringStartsWith { name, prefix } => {
                self.check_string_starts_with(name, prefix, line)
            }
            Expr::StringStartsWithValue { value, prefix } => {
                self.check_string_predicate_value(value, prefix, "startsWith", "prefix", line)
            }
            Expr::StringEndsWith { name, suffix } => {
                self.check_string_ends_with(name, suffix, line)
            }
            Expr::StringEndsWithValue { value, suffix } => {
                self.check_string_predicate_value(value, suffix, "endsWith", "suffix", line)
            }
            Expr::StringTrim(name) => self.check_string_trim(name, line),
            Expr::StringTrimValue(value) => self.check_string_transform_value(value, "trim", line),
            Expr::StringTrimStart(name) => self.check_string_transform(name, "trimStart", line),
            Expr::StringTrimStartValue(value) => {
                self.check_string_transform_value(value, "trimStart", line)
            }
            Expr::StringTrimEnd(name) => self.check_string_transform(name, "trimEnd", line),
            Expr::StringTrimEndValue(value) => {
                self.check_string_transform_value(value, "trimEnd", line)
            }
            Expr::StringToUpper(name) => self.check_string_to_upper(name, line),
            Expr::StringToUpperValue(value) => {
                self.check_string_transform_value(value, "toUpper", line)
            }
            Expr::StringToLower(name) => self.check_string_to_lower(name, line),
            Expr::StringToLowerValue(value) => {
                self.check_string_transform_value(value, "toLower", line)
            }
            Expr::StringRepeat { name, count } => self.check_string_repeat(name, count, line),
            Expr::StringRepeatValue { value, count } => {
                self.check_string_repeat_value(value, count, line)
            }
            Expr::StringSplit { name, separator } => self.check_string_split(name, separator, line),
            Expr::StringSplitValue { value, separator } => {
                self.check_string_split_value(value, separator, line)
            }
            Expr::StringReplace { name, from, to } => {
                self.check_string_replace(name, from, to, line)
            }
            Expr::StringReplaceValue { value, from, to } => {
                self.check_string_replace_value(value, from, to, line)
            }
            Expr::PathBasename(name) => self.check_path_transform(name, "basename", line),
            Expr::PathBasenameValue(value) => {
                self.check_path_transform_value(value, "basename", line)
            }
            Expr::PathDirname(name) => self.check_path_transform(name, "dirname", line),
            Expr::PathDirnameValue(value) => {
                self.check_path_transform_value(value, "dirname", line)
            }
            Expr::PathStem(name) => self.check_path_transform(name, "stem", line),
            Expr::PathStemValue(value) => self.check_path_transform_value(value, "stem", line),
            Expr::PathExtname(name) => self.check_path_transform(name, "extname", line),
            Expr::PathExtnameValue(value) => {
                self.check_path_transform_value(value, "extname", line)
            }
            Expr::PathIsAbsolute(name) => self.check_path_predicate(name, "isAbsolute", line),
            Expr::PathIsAbsoluteValue(value) => self
                .check_path_transform_value(value, "isAbsolute", line)
                .map(|_| Type::Bool),
            Expr::RawString(_) | Expr::Env(_) | Expr::EnvDefault { .. } => Ok(Type::String),
            Expr::Command { .. }
            | Expr::CommandResult { .. }
            | Expr::AsyncCommand(_)
            | Expr::Pipeline { .. }
            | Expr::TryPipeline { .. }
            | Expr::PipelineResult { .. } => Err(unsafe_execution_error(line)),
            Expr::ProcessArgs => Ok(Type::Array(Box::new(Type::String))),
            Expr::FsIsFile { path } | Expr::FsIsDir { path } => {
                self.require_read_access("filesystem read", line)?;
                self.check_fs_path(path, line).map(|_| Type::Bool)
            }
            Expr::FsSize { path } => {
                self.require_read_access("fs.size", line)?;
                self.check_fs_path(path, line).map(|_| Type::Int)
            }
            Expr::FsReadLines { path } => {
                self.require_read_access("fs.readLines", line)?;
                self.check_fs_path(path, line)
                    .map(|_| Type::Array(Box::new(Type::String)))
            }
            Expr::FsList { path } => {
                self.require_read_access("fs.list", line)?;
                self.check_fs_path(path, line)
                    .map(|_| Type::Array(Box::new(Type::String)))
            }
            Expr::FsWriteLines { path, lines } => {
                self.require_write_access("fs.writeLines", line)?;
                self.check_fs_write_lines(path, lines, "fs.writeLines", line)
            }
            Expr::FsAppendLines { path, lines } => {
                self.require_write_access("fs.appendLines", line)?;
                self.check_fs_write_lines(path, lines, "fs.appendLines", line)
            }
            Expr::CliParse => Ok(Type::Map(Box::new(Type::String), Box::new(Type::String))),
            Expr::Await(name) => self.check_await(name, line),
            Expr::IfElse {
                condition,
                then_expr,
                else_expr,
            } => self.check_if_expr(condition, then_expr, else_expr, line),
            Expr::Match { value, arms } => self.check_match(value, arms, line),
            Expr::MatchGuardResult(value) => {
                let ty = self.check_expr(value, line)?;
                let Some((ok, _)) = result_types(&ty) else {
                    return Err(CompileError::new(
                        line,
                        format!("match guard propagation expects Result, found {}", ty.name()),
                    ));
                };
                if *ok == Type::Bool {
                    Ok(Type::Bool)
                } else {
                    Err(CompileError::new(
                        line,
                        format!("match guard must be Bool, found {}", ok.name()),
                    ))
                }
            }
            Expr::Ident(name) => {
                if self
                    .variants
                    .get(name)
                    .is_some_and(|variant| variant.fields.is_empty())
                {
                    return self.check_variant(name, &[], line);
                }
                self.lookup_binding(name)
                    .map(|binding| binding.ty.clone())
                    .or_else(|| self.functions.get(name).map(FunctionSig::function_type))
                    .ok_or_else(|| CompileError::new(line, format!("undefined variable `{name}`")))
            }
            Expr::Not(expr) => {
                let ty = self.check_expr(expr, line)?;
                if ty == Type::Bool {
                    Ok(Type::Bool)
                } else {
                    Err(CompileError::new(
                        line,
                        format!("operator `!` requires Bool operand, found {}", ty.name()),
                    ))
                }
            }
            Expr::BitNot(expr) => {
                let ty = self.check_expr(expr, line)?;
                if self.is_integer_numeric(&ty) {
                    Ok(Type::Int)
                } else {
                    Err(CompileError::new(
                        line,
                        format!("operator `~` requires Int operands, found {}", ty.name()),
                    ))
                }
            }
            Expr::Binary { left, op, right } => self.check_binary(left, *op, right, line),
        }
    }

    fn check_binary(
        &self,
        left: &Expr,
        op: BinaryOp,
        right: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let left_ty = self.check_expr(left, line)?;
        let right_ty = self.check_expr(right, line)?;
        match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div => {
                if self.is_numeric(&left_ty) && self.is_numeric(&right_ty) {
                    if left_ty == Type::Float || right_ty == Type::Float {
                        Ok(Type::Float)
                    } else {
                        Ok(Type::Int)
                    }
                } else {
                    Err(CompileError::new(
                        line,
                        format!(
                            "operator `{}` requires numeric operands, found {} and {}",
                            op.bash(),
                            left_ty.name(),
                            right_ty.name()
                        ),
                    ))
                }
            }
            BinaryOp::Mod => {
                if self.is_integer_numeric(&left_ty) && self.is_integer_numeric(&right_ty) {
                    Ok(Type::Int)
                } else {
                    Err(CompileError::new(
                        line,
                        format!(
                            "operator `{}` requires Int operands, found {} and {}",
                            op.bash(),
                            left_ty.name(),
                            right_ty.name()
                        ),
                    ))
                }
            }
            BinaryOp::Concat => {
                if self.is_string_like(&left_ty) && self.is_string_like(&right_ty) {
                    Ok(Type::String)
                } else {
                    Err(CompileError::new(
                        line,
                        format!(
                            "operator `++` requires String or Path operands, found {} and {}",
                            left_ty.name(),
                            right_ty.name()
                        ),
                    ))
                }
            }
            BinaryOp::BitAnd
            | BinaryOp::BitOr
            | BinaryOp::BitXor
            | BinaryOp::Shl
            | BinaryOp::Shr => {
                if self.is_integer_numeric(&left_ty) && self.is_integer_numeric(&right_ty) {
                    Ok(Type::Int)
                } else {
                    Err(CompileError::new(
                        line,
                        format!(
                            "operator `{}` requires Int operands, found {} and {}",
                            op.bash(),
                            left_ty.name(),
                            right_ty.name()
                        ),
                    ))
                }
            }
            BinaryOp::Eq | BinaryOp::Ne => {
                if self.is_comparable_by_equality(&left_ty, &right_ty) {
                    Ok(Type::Bool)
                } else {
                    Err(CompileError::new(
                        line,
                        format!(
                            "operator `{}` requires matching operand types, found {} and {}",
                            op.bash(),
                            left_ty.name(),
                            right_ty.name()
                        ),
                    ))
                }
            }
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                if self.is_numeric(&left_ty) && self.is_numeric(&right_ty) {
                    Ok(Type::Bool)
                } else {
                    Err(CompileError::new(
                        line,
                        format!(
                            "operator `{}` requires numeric operands, found {} and {}",
                            op.bash(),
                            left_ty.name(),
                            right_ty.name()
                        ),
                    ))
                }
            }
            BinaryOp::And | BinaryOp::Or => {
                if left_ty == Type::Bool && right_ty == Type::Bool {
                    Ok(Type::Bool)
                } else {
                    Err(CompileError::new(
                        line,
                        format!(
                            "operator `{}` requires Bool operands, found {} and {}",
                            op.bash(),
                            left_ty.name(),
                            right_ty.name()
                        ),
                    ))
                }
            }
        }
    }

    fn check_cast(&self, expr: &Expr, target: &Type, line: usize) -> Result<Type, CompileError> {
        let actual = self.check_expr(expr, line)?;
        let target = self.resolve_type(target, line)?;
        if self.is_castable(&target, &actual, expr) {
            Ok(target)
        } else {
            Err(CompileError::new(
                line,
                format!("cannot cast {} to {}", actual.name(), target.name()),
            ))
        }
    }

    fn check_array(&self, values: &[Expr], line: usize) -> Result<Type, CompileError> {
        let Some(first) = values.first() else {
            return Ok(Type::Array(Box::new(Type::Unit)));
        };
        let element_ty = self.check_expr(first, line)?;
        for value in &values[1..] {
            let value_ty = self.check_expr(value, line)?;
            if !self.is_assignable(&element_ty, &value_ty, value)
                || !self.is_assignable(&value_ty, &element_ty, first)
            {
                return Err(CompileError::new(
                    line,
                    format!(
                        "array elements must have matching types, found {} and {}",
                        element_ty.name(),
                        value_ty.name()
                    ),
                ));
            }
        }
        Ok(Type::Array(Box::new(element_ty)))
    }

    fn check_await(&self, name: &str, line: usize) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined future `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Future(value) => Ok((**value).clone()),
            other => Err(CompileError::new(
                line,
                format!("await expects Future, found {}", other.name()),
            )),
        }
    }

    fn check_allowed_command(
        &self,
        group: &str,
        command: &str,
        args: &[Expr],
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(policy) = self.policy.command(group, command) else {
            return Err(CompileError::new(
                line,
                format!("command `{group}.{command}` is not allowed by the execution policy"),
            ));
        };
        for index in policy.read_args() {
            if *index >= args.len() {
                return Err(CompileError::new(
                    line,
                    format!(
                        "policy for command `{group}.{command}` requires read path argument {}",
                        index + 1
                    ),
                ));
            }
            self.require_read_access(&format!("command `{group}.{command}`"), line)?;
        }
        for index in policy.write_args() {
            if *index >= args.len() {
                return Err(CompileError::new(
                    line,
                    format!(
                        "policy for command `{group}.{command}` requires write path argument {}",
                        index + 1
                    ),
                ));
            }
            self.require_write_access(&format!("command `{group}.{command}`"), line)?;
        }
        for arg in args {
            let ty = self.check_expr(arg, line)?;
            if !matches!(
                ty,
                Type::String | Type::Path | Type::Int | Type::Float | Type::Bool | Type::ExitCode
            ) {
                return Err(CompileError::new(
                    line,
                    format!(
                        "command `{group}.{command}` arguments must be scalar, found {}",
                        ty.name()
                    ),
                ));
            }
        }
        Ok(Type::String)
    }

    fn require_read_access(&self, operation: &str, line: usize) -> Result<(), CompileError> {
        if self.policy.has_read_access() {
            Ok(())
        } else {
            Err(CompileError::new(
                line,
                format!("{operation} requires at least one allowed filesystem read root"),
            ))
        }
    }

    fn require_write_access(&self, operation: &str, line: usize) -> Result<(), CompileError> {
        if self.policy.has_write_access() {
            Ok(())
        } else {
            Err(CompileError::new(
                line,
                format!("{operation} requires at least one allowed filesystem write root"),
            ))
        }
    }

    fn check_path_exists(&self, path: &Expr, line: usize) -> Result<Type, CompileError> {
        let path_ty = self.check_expr(path, line)?;
        if matches!(path_ty, Type::String | Type::Path) {
            Ok(Type::Bool)
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "pathExists expects String or Path, found {}",
                    path_ty.name()
                ),
            ))
        }
    }

    fn check_fs_path(&self, path: &Expr, line: usize) -> Result<(), CompileError> {
        let path_ty = self.check_expr(path, line)?;
        if matches!(path_ty, Type::String | Type::Path) {
            Ok(())
        } else {
            Err(CompileError::new(
                line,
                format!("fs path must be String or Path, found {}", path_ty.name()),
            ))
        }
    }

    fn check_fs_write_lines(
        &self,
        path: &Expr,
        lines: &Expr,
        function: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        self.check_fs_path(path, line)?;
        let lines_ty = self.check_expr(lines, line)?;
        match lines_ty {
            Type::Array(element) if self.is_string_like(&element) => Ok(Type::Unit),
            Type::Array(element) => Err(CompileError::new(
                line,
                format!(
                    "{function} lines must be [String] or [Path], found [{}]",
                    element.name()
                ),
            )),
            other => Err(CompileError::new(
                line,
                format!("{function} lines must be Array, found {}", other.name()),
            )),
        }
    }

    fn check_process_env(&self, name: &Expr, line: usize) -> Result<Type, CompileError> {
        let name_ty = self.check_expr(name, line)?;
        if matches!(name_ty, Type::String | Type::Path) {
            Ok(Type::String)
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "process.env name must be String or Path, found {}",
                    name_ty.name()
                ),
            ))
        }
    }

    fn check_json_parse(&self, value: &Expr, line: usize) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, line)?;
        if matches!(value_ty, Type::String | Type::Path) {
            Ok(Type::Map(Box::new(Type::String), Box::new(Type::String)))
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "json.parse value must be String or Path, found {}",
                    value_ty.name()
                ),
            ))
        }
    }

    fn check_json_stringify(&self, name: &str, line: usize) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Map(key, value) if **key == Type::String && **value == Type::String => {
                Ok(Type::String)
            }
            other => Err(CompileError::new(
                line,
                format!(
                    "json.stringify expects Map[String, String], found {}",
                    other.name()
                ),
            )),
        }
    }

    fn check_json_stringify_value(&self, value: &Expr, line: usize) -> Result<Type, CompileError> {
        if !matches!(value, Expr::Map(_) | Expr::JsonParse { .. }) {
            return Err(CompileError::new(
                line,
                "json.stringify value must be a map literal or json.parse result".to_string(),
            ));
        }
        match self.check_expr(value, line)? {
            Type::Map(key, value) if *key == Type::String && *value == Type::String => {
                Ok(Type::String)
            }
            other => Err(CompileError::new(
                line,
                format!(
                    "json.stringify expects Map[String, String], found {}",
                    other.name()
                ),
            )),
        }
    }

    fn check_map(&self, entries: &[(Expr, Expr)], line: usize) -> Result<Type, CompileError> {
        let Some((first_key, first_value)) = entries.first() else {
            return Ok(Type::Map(Box::new(Type::Unit), Box::new(Type::Unit)));
        };
        let key_ty = self.check_expr(first_key, line)?;
        let value_ty = self.check_expr(first_value, line)?;
        for (key, value) in &entries[1..] {
            let next_key_ty = self.check_expr(key, line)?;
            let next_value_ty = self.check_expr(value, line)?;
            if !self.is_assignable(&key_ty, &next_key_ty, key)
                || !self.is_assignable(&next_key_ty, &key_ty, first_key)
            {
                return Err(CompileError::new(
                    line,
                    format!(
                        "map keys must have matching types, found {} and {}",
                        key_ty.name(),
                        next_key_ty.name()
                    ),
                ));
            }
            if !self.is_assignable(&value_ty, &next_value_ty, value)
                || !self.is_assignable(&next_value_ty, &value_ty, first_value)
            {
                return Err(CompileError::new(
                    line,
                    format!(
                        "map values must have matching types, found {} and {}",
                        value_ty.name(),
                        next_value_ty.name()
                    ),
                ));
            }
        }
        Ok(Type::Map(Box::new(key_ty), Box::new(value_ty)))
    }

    fn check_record(&self, fields: &[(String, Expr)], line: usize) -> Result<Type, CompileError> {
        let mut typed_fields = Vec::new();
        for (name, value) in fields {
            if typed_fields.iter().any(|(existing, _)| existing == name) {
                return Err(CompileError::new(
                    line,
                    format!("record field `{name}` is already defined"),
                ));
            }
            typed_fields.push((name.clone(), self.check_expr(value, line)?));
        }
        Ok(Type::Record(typed_fields))
    }

    fn check_tuple(&self, values: &[Expr], line: usize) -> Result<Type, CompileError> {
        if values.len() < 2 {
            return Err(CompileError::new(
                line,
                "tuple literal requires at least two elements".to_string(),
            ));
        }
        let mut elements = Vec::new();
        for value in values {
            elements.push(self.check_expr(value, line)?);
        }
        Ok(Type::Tuple(elements))
    }

    fn check_index(&self, name: &str, index: &Expr, line: usize) -> Result<Type, CompileError> {
        let index_ty = self.check_expr(index, line)?;
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Array(element) => {
                if self.is_integer_numeric(&index_ty) {
                    Ok((**element).clone())
                } else {
                    Err(CompileError::new(
                        line,
                        format!("array index must be Int, found {}", index_ty.name()),
                    ))
                }
            }
            Type::Map(key, value) => {
                if self.is_assignable(key, &index_ty, index) {
                    Ok((**value).clone())
                } else {
                    Err(CompileError::new(
                        line,
                        format!("map key must be {}, found {}", key.name(), index_ty.name()),
                    ))
                }
            }
            other => Err(CompileError::new(
                line,
                format!("cannot index `{name}` of type {}", other.name()),
            )),
        }
    }

    fn check_index_value(
        &self,
        value: &Expr,
        index: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let index_ty = self.check_expr(index, line)?;
        let value_ty = self.check_expr(value, line)?;
        match value_ty {
            Type::Array(element) => {
                if !matches!(value, Expr::Array(_)) {
                    return Err(CompileError::new(
                        line,
                        "index value must be an array literal or named array".to_string(),
                    ));
                }
                if self.is_integer_numeric(&index_ty) {
                    Ok(*element)
                } else {
                    Err(CompileError::new(
                        line,
                        format!("array index must be Int, found {}", index_ty.name()),
                    ))
                }
            }
            Type::Map(key, map_value) => {
                if !matches!(value, Expr::Map(_)) {
                    return Err(CompileError::new(
                        line,
                        "index value must be a map literal or named map".to_string(),
                    ));
                }
                if self.is_assignable(&key, &index_ty, index) {
                    Ok(*map_value)
                } else {
                    Err(CompileError::new(
                        line,
                        format!("map key must be {}, found {}", key.name(), index_ty.name()),
                    ))
                }
            }
            other => Err(CompileError::new(
                line,
                format!("cannot index value of type {}", other.name()),
            )),
        }
    }

    fn check_slice(
        &self,
        name: &str,
        start: &Expr,
        end: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let start_ty = self.check_expr(start, line)?;
        if !self.is_integer_numeric(&start_ty) {
            return Err(CompileError::new(
                line,
                format!("slice start must be Int, found {}", start_ty.name()),
            ));
        }
        let end_ty = self.check_expr(end, line)?;
        if !self.is_integer_numeric(&end_ty) {
            return Err(CompileError::new(
                line,
                format!("slice end must be Int, found {}", end_ty.name()),
            ));
        }
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Array(element) => Ok(Type::Array(element.clone())),
            ty if self.is_string_like(ty) => Ok(Type::String),
            other => Err(CompileError::new(
                line,
                format!("type {} has no slice method", other.name()),
            )),
        }
    }

    fn check_string_slice_value(
        &self,
        value: &Expr,
        start: &Expr,
        end: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let start_ty = self.check_expr(start, line)?;
        if !self.is_integer_numeric(&start_ty) {
            return Err(CompileError::new(
                line,
                format!("slice start must be Int, found {}", start_ty.name()),
            ));
        }
        let end_ty = self.check_expr(end, line)?;
        if !self.is_integer_numeric(&end_ty) {
            return Err(CompileError::new(
                line,
                format!("slice end must be Int, found {}", end_ty.name()),
            ));
        }
        let value_ty = self.check_expr(value, line)?;
        if self.is_string_like(&value_ty) {
            Ok(Type::String)
        } else {
            Err(CompileError::new(
                line,
                format!("type {} has no slice method", value_ty.name()),
            ))
        }
    }

    fn check_array_slice_value(
        &self,
        value: &Expr,
        start: &Expr,
        end: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let start_ty = self.check_expr(start, line)?;
        if !self.is_integer_numeric(&start_ty) {
            return Err(CompileError::new(
                line,
                format!("slice start must be Int, found {}", start_ty.name()),
            ));
        }
        let end_ty = self.check_expr(end, line)?;
        if !self.is_integer_numeric(&end_ty) {
            return Err(CompileError::new(
                line,
                format!("slice end must be Int, found {}", end_ty.name()),
            ));
        }
        self.check_array_value(value, "slice", line)
    }

    fn lower_len(&self, name: &str) -> Expr {
        self.lookup_binding(name)
            .filter(|binding| self.is_string_like(&binding.ty))
            .map(|_| Expr::StringLen(name.to_string()))
            .unwrap_or_else(|| Expr::Len(name.to_string()))
    }

    fn lower_is_empty(&self, name: &str) -> Expr {
        self.lookup_binding(name)
            .filter(|binding| self.is_string_like(&binding.ty))
            .map(|_| Expr::StringIsEmpty(name.to_string()))
            .unwrap_or_else(|| Expr::IsEmpty(name.to_string()))
    }

    fn check_len(&self, name: &str, line: usize) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Array(_) | Type::Map(_, _) => Ok(Type::Int),
            ty if self.is_string_like(ty) => Ok(Type::Int),
            _ => Err(CompileError::new(
                line,
                format!("type {} has no len method", binding.ty.name()),
            )),
        }
    }

    fn check_is_empty(&self, name: &str, line: usize) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Array(_) | Type::Map(_, _) => Ok(Type::Bool),
            ty if self.is_string_like(ty) => Ok(Type::Bool),
            _ => Err(CompileError::new(
                line,
                format!("type {} has no isEmpty method", binding.ty.name()),
            )),
        }
    }

    fn check_array_edge(
        &self,
        name: &str,
        method: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Array(element) => Ok((**element).clone()),
            _ => Err(CompileError::new(
                line,
                format!("type {} has no {method} method", binding.ty.name()),
            )),
        }
    }

    fn check_array_edge_value(
        &self,
        value: &Expr,
        method: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, line)?;
        let Type::Array(element) = value_ty else {
            return Err(CompileError::new(
                line,
                format!("type {} has no {method} method", value_ty.name()),
            ));
        };
        if *element == Type::Unit {
            return Err(CompileError::new(
                line,
                format!("{method} requires a non-empty array literal"),
            ));
        }
        if !matches!(value, Expr::Array(_)) {
            return Err(CompileError::new(
                line,
                format!("{method} value must be an array literal or named array"),
            ));
        }
        Ok(*element)
    }

    fn check_array_reverse(&self, name: &str, line: usize) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Array(element) => Ok(Type::Array(element.clone())),
            _ => Err(CompileError::new(
                line,
                format!("type {} has no reverse method", binding.ty.name()),
            )),
        }
    }

    fn check_array_sort(&self, name: &str, line: usize) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Array(element) if self.is_string_like(element) => {
                Ok(Type::Array(element.clone()))
            }
            Type::Array(element) => Err(CompileError::new(
                line,
                format!(
                    "sort array elements must be String or Path, found {}",
                    element.name()
                ),
            )),
            _ => Err(CompileError::new(
                line,
                format!("type {} has no sort method", binding.ty.name()),
            )),
        }
    }

    fn check_array_unique(&self, name: &str, line: usize) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Array(element) => Ok(Type::Array(element.clone())),
            _ => Err(CompileError::new(
                line,
                format!("type {} has no unique method", binding.ty.name()),
            )),
        }
    }

    fn check_array_transform_value(
        &self,
        value: &Expr,
        method: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, line)?;
        let Type::Array(element) = value_ty else {
            return Err(CompileError::new(
                line,
                format!("type {} has no {method} method", value_ty.name()),
            ));
        };
        if !matches!(value, Expr::Array(_)) {
            return Err(CompileError::new(
                line,
                format!("{method} value must be an array literal or named array"),
            ));
        }
        Ok(Type::Array(element))
    }

    fn check_array_sort_value(&self, value: &Expr, line: usize) -> Result<Type, CompileError> {
        let value_ty = self.check_array_transform_value(value, "sort", line)?;
        match value_ty {
            Type::Array(element) if self.is_string_like(&element) => Ok(Type::Array(element)),
            Type::Array(element) => Err(CompileError::new(
                line,
                format!(
                    "sort array elements must be String or Path, found {}",
                    element.name()
                ),
            )),
            other => Err(CompileError::new(
                line,
                format!("type {} has no sort method", other.name()),
            )),
        }
    }

    fn check_array_map(
        &self,
        name: &str,
        mapper: &Expr,
        line: usize,
    ) -> Result<(Type, Type), CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        let element = match binding.ty {
            Type::Array(element) => element,
            other => {
                return Err(CompileError::new(
                    line,
                    format!("type {} has no map method", other.name()),
                ))
            }
        };
        let mapped = self.check_map_mapper(mapper, &element, "map", line)?;
        Ok((*element, mapped))
    }

    fn check_array_map_value(
        &self,
        value: &Expr,
        mapper: &Expr,
        line: usize,
    ) -> Result<(Type, Type), CompileError> {
        let value_ty = self.check_expr(value, line)?;
        let Type::Array(element) = value_ty else {
            return Err(CompileError::new(
                line,
                format!("type {} has no map method", value_ty.name()),
            ));
        };
        if *element == Type::Unit {
            return Err(CompileError::new(
                line,
                "map requires a non-empty array literal or typed array".to_string(),
            ));
        }
        if !matches!(value, Expr::Array(_)) {
            return Err(CompileError::new(
                line,
                "map value must be an array literal or named array".to_string(),
            ));
        }
        let mapped = self.check_map_mapper(mapper, &element, "map", line)?;
        Ok((*element, mapped))
    }

    fn check_option_map(
        &self,
        name: &str,
        mapper: &Expr,
        line: usize,
    ) -> Result<(Type, Type), CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        let Some(element) = option_element_type(&binding.ty) else {
            return Err(CompileError::new(
                line,
                format!("type {} has no map method", binding.ty.name()),
            ));
        };
        if *element == Type::Unit {
            return Err(CompileError::new(
                line,
                "map on None requires a typed Option".to_string(),
            ));
        }
        let mapped = self.check_map_mapper(mapper, element, "map", line)?;
        Ok((element.clone(), mapped))
    }

    fn check_option_map_value(
        &self,
        value: &Expr,
        mapper: &Expr,
        line: usize,
    ) -> Result<(Type, Type), CompileError> {
        let value_ty = self.check_expr(value, line)?;
        let Some(element) = option_element_type(&value_ty) else {
            return Err(CompileError::new(
                line,
                format!("type {} has no map method", value_ty.name()),
            ));
        };
        if *element == Type::Unit {
            return Err(CompileError::new(
                line,
                "map on None requires a typed Option".to_string(),
            ));
        }
        let mapped = self.check_map_mapper(mapper, element, "map", line)?;
        Ok((element.clone(), mapped))
    }

    fn check_result_map(
        &self,
        name: &str,
        mapper: &Expr,
        line: usize,
    ) -> Result<(Type, Type, Type), CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        let Some((element, error)) = result_types(&binding.ty) else {
            return Err(CompileError::new(
                line,
                format!("type {} has no map method", binding.ty.name()),
            ));
        };
        if *element == Type::Unit {
            return Err(CompileError::new(
                line,
                "map on Err requires a typed Result".to_string(),
            ));
        }
        let mapped = self.check_map_mapper(mapper, element, "map", line)?;
        Ok((element.clone(), error.clone(), mapped))
    }

    fn check_result_map_value(
        &self,
        value: &Expr,
        mapper: &Expr,
        line: usize,
    ) -> Result<(Type, Type, Type), CompileError> {
        let value_ty = self.check_expr(value, line)?;
        let Some((element, error)) = result_types(&value_ty) else {
            return Err(CompileError::new(
                line,
                format!("type {} has no map method", value_ty.name()),
            ));
        };
        if *element == Type::Unit {
            return Err(CompileError::new(
                line,
                "map on Err requires a typed Result".to_string(),
            ));
        }
        let mapped = self.check_map_mapper(mapper, element, "map", line)?;
        Ok((element.clone(), error.clone(), mapped))
    }

    fn check_option_flat_map(
        &self,
        name: &str,
        mapper: &Expr,
        line: usize,
    ) -> Result<(Type, Type), CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        let Some(element) = option_element_type(&binding.ty) else {
            return Err(CompileError::new(
                line,
                format!("type {} has no flatMap method", binding.ty.name()),
            ));
        };
        if *element == Type::Unit {
            return Err(CompileError::new(
                line,
                "flatMap on None requires a typed Option".to_string(),
            ));
        }
        let mapped = self.check_map_mapper(mapper, element, "flatMap", line)?;
        self.check_flat_map_result(element, mapped, line)
    }

    fn check_option_flat_map_value(
        &self,
        value: &Expr,
        mapper: &Expr,
        line: usize,
    ) -> Result<(Type, Type), CompileError> {
        let value_ty = self.check_expr(value, line)?;
        let Some(element) = option_element_type(&value_ty) else {
            return Err(CompileError::new(
                line,
                format!("type {} has no flatMap method", value_ty.name()),
            ));
        };
        if *element == Type::Unit {
            return Err(CompileError::new(
                line,
                "flatMap on None requires a typed Option".to_string(),
            ));
        }
        let mapped = self.check_map_mapper(mapper, element, "flatMap", line)?;
        self.check_flat_map_result(element, mapped, line)
    }

    fn check_result_flat_map(
        &self,
        name: &str,
        mapper: &Expr,
        line: usize,
    ) -> Result<(Type, Type, Type), CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        let Some((element, error)) = result_types(&binding.ty) else {
            return Err(CompileError::new(
                line,
                format!("type {} has no flatMap method", binding.ty.name()),
            ));
        };
        if *element == Type::Unit {
            return Err(CompileError::new(
                line,
                "flatMap on Err requires a typed Result".to_string(),
            ));
        }
        let mapped = self.check_map_mapper(mapper, element, "flatMap", line)?;
        self.check_result_flat_map_result(element, error, mapped, mapper, line)
    }

    fn check_result_flat_map_value(
        &self,
        value: &Expr,
        mapper: &Expr,
        line: usize,
    ) -> Result<(Type, Type, Type), CompileError> {
        let value_ty = self.check_expr(value, line)?;
        let Some((element, error)) = result_types(&value_ty) else {
            return Err(CompileError::new(
                line,
                format!("type {} has no flatMap method", value_ty.name()),
            ));
        };
        if *element == Type::Unit {
            return Err(CompileError::new(
                line,
                "flatMap on Err requires a typed Result".to_string(),
            ));
        }
        let mapped = self.check_map_mapper(mapper, element, "flatMap", line)?;
        self.check_result_flat_map_result(element, error, mapped, mapper, line)
    }

    fn check_result_flat_map_result(
        &self,
        element: &Type,
        error: &Type,
        mapped: Type,
        mapper: &Expr,
        line: usize,
    ) -> Result<(Type, Type, Type), CompileError> {
        let Some((mapped_element, mapped_error)) = result_types(&mapped) else {
            return Err(CompileError::new(
                line,
                format!("flatMap mapper must return Result, found {}", mapped.name()),
            ));
        };
        let mapped_element = mapped_element.clone();
        let result_error = if *error == Type::Unit {
            mapped_error.clone()
        } else if *mapped_error == Type::Unit || self.is_assignable(error, mapped_error, mapper) {
            error.clone()
        } else {
            return Err(CompileError::new(
                line,
                format!(
                    "flatMap mapper error must be assignable to {}, found {}",
                    error.name(),
                    mapped_error.name()
                ),
            ));
        };
        Ok((
            element.clone(),
            mapped,
            Type::Applied("Result".to_string(), vec![mapped_element, result_error]),
        ))
    }

    fn check_option_ap(&self, name: &str, value: &Expr, line: usize) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        let value_ty = self.check_expr(value, line)?;
        self.check_option_ap_types(&binding.ty, &value_ty, value, line)
    }

    fn check_option_ap_value(
        &self,
        function: &Expr,
        value: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let function_ty = self.check_expr(function, line)?;
        let value_ty = self.check_expr(value, line)?;
        self.check_option_ap_types(&function_ty, &value_ty, value, line)
    }

    fn check_option_ap_types(
        &self,
        function_ty: &Type,
        value_ty: &Type,
        value: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(function) = option_element_type(function_ty) else {
            return Err(CompileError::new(
                line,
                format!("type {} has no ap method", function_ty.name()),
            ));
        };
        if *function == Type::Unit {
            return Err(CompileError::new(
                line,
                "ap on None requires a typed Option function".to_string(),
            ));
        }
        let Type::Function(params, return_type) = function else {
            return Err(CompileError::new(
                line,
                format!(
                    "ap receiver must contain a function, found {}",
                    function.name()
                ),
            ));
        };
        if params.len() != 1 {
            return Err(CompileError::new(
                line,
                format!("ap function expects 1 parameter, found {}", params.len()),
            ));
        }
        let Some(element) = option_element_type(value_ty) else {
            return Err(CompileError::new(
                line,
                format!("ap argument must be Option, found {}", value_ty.name()),
            ));
        };
        if *element != Type::Unit && !self.is_assignable(&params[0], element, value) {
            return Err(CompileError::new(
                line,
                format!(
                    "ap argument must contain {}, found {}",
                    params[0].name(),
                    element.name()
                ),
            ));
        }
        Ok(Type::Applied(
            "Option".to_string(),
            vec![(**return_type).clone()],
        ))
    }

    fn check_result_ap(&self, name: &str, value: &Expr, line: usize) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        let value_ty = self.check_expr(value, line)?;
        self.check_result_ap_types(&binding.ty, &value_ty, value, line)
    }

    fn check_result_ap_value(
        &self,
        function: &Expr,
        value: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let function_ty = self.check_expr(function, line)?;
        let value_ty = self.check_expr(value, line)?;
        self.check_result_ap_types(&function_ty, &value_ty, value, line)
    }

    fn check_result_ap_types(
        &self,
        function_ty: &Type,
        value_ty: &Type,
        value: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some((function, function_error)) = result_types(function_ty) else {
            return Err(CompileError::new(
                line,
                format!("type {} has no ap method", function_ty.name()),
            ));
        };
        if *function == Type::Unit {
            return Err(CompileError::new(
                line,
                "ap on Err requires a typed Result function".to_string(),
            ));
        }
        let Type::Function(params, return_type) = function else {
            return Err(CompileError::new(
                line,
                format!(
                    "ap receiver must contain a function, found {}",
                    function.name()
                ),
            ));
        };
        if params.len() != 1 {
            return Err(CompileError::new(
                line,
                format!("ap function expects 1 parameter, found {}", params.len()),
            ));
        }
        let Some((element, value_error)) = result_types(value_ty) else {
            return Err(CompileError::new(
                line,
                format!("ap argument must be Result, found {}", value_ty.name()),
            ));
        };
        if *element != Type::Unit && !self.is_assignable(&params[0], element, value) {
            return Err(CompileError::new(
                line,
                format!(
                    "ap argument must contain {}, found {}",
                    params[0].name(),
                    element.name()
                ),
            ));
        }
        let error = if *function_error == Type::Unit {
            value_error.clone()
        } else if *value_error == Type::Unit
            || self.is_assignable(function_error, value_error, value)
        {
            function_error.clone()
        } else {
            return Err(CompileError::new(
                line,
                format!(
                    "ap argument error must be assignable to {}, found {}",
                    function_error.name(),
                    value_error.name()
                ),
            ));
        };
        Ok(Type::Applied(
            "Result".to_string(),
            vec![(**return_type).clone(), error],
        ))
    }

    fn check_option_or_else(
        &self,
        name: &str,
        fallback: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        let fallback_ty = self.check_expr(fallback, line)?;
        self.check_option_or_else_types(&binding.ty, &fallback_ty, fallback, line)
    }

    fn check_option_or_else_value(
        &self,
        value: &Expr,
        fallback: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, line)?;
        let fallback_ty = self.check_expr(fallback, line)?;
        self.check_option_or_else_types(&value_ty, &fallback_ty, fallback, line)
    }

    fn check_option_or_else_try(
        &self,
        value: &Expr,
        fallback: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, line)?;
        let Some(value_element) = option_element_type(&value_ty) else {
            return Err(CompileError::new(
                line,
                format!("type {} has no orElse method", value_ty.name()),
            ));
        };
        let fallback_ty = self.check_expr(fallback, line)?;
        let Some((fallback_ok, fallback_error)) = result_types(&fallback_ty) else {
            return Err(CompileError::new(
                line,
                format!(
                    "propagating orElse fallback must return Result, found {}",
                    fallback_ty.name()
                ),
            ));
        };
        let Some(fallback_element) = option_element_type(fallback_ok) else {
            return Err(CompileError::new(
                line,
                format!(
                    "propagating orElse fallback must return Option, found {}",
                    fallback_ok.name()
                ),
            ));
        };
        if *value_element != Type::Unit
            && *fallback_element != Type::Unit
            && !self.is_assignable(value_element, fallback_element, fallback)
        {
            return Err(CompileError::new(
                line,
                format!(
                    "orElse fallback element mismatch: expected {}, found {}",
                    value_element.name(),
                    fallback_element.name()
                ),
            ));
        }
        let element = if *value_element == Type::Unit {
            fallback_element.clone()
        } else {
            value_element.clone()
        };
        Ok(Type::Applied(
            "Result".to_string(),
            vec![
                Type::Applied("Option".to_string(), vec![element]),
                fallback_error.clone(),
            ],
        ))
    }

    fn check_option_or_else_types(
        &self,
        value_ty: &Type,
        fallback_ty: &Type,
        fallback: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(value_element) = option_element_type(value_ty) else {
            return Err(CompileError::new(
                line,
                format!("type {} has no orElse method", value_ty.name()),
            ));
        };
        let Some(fallback_element) = option_element_type(fallback_ty) else {
            return Err(CompileError::new(
                line,
                format!(
                    "orElse fallback must be Option, found {}",
                    fallback_ty.name()
                ),
            ));
        };
        if *value_element == Type::Unit && *fallback_element == Type::Unit {
            return Err(CompileError::new(
                line,
                "orElse with two None values requires a typed Option".to_string(),
            ));
        }
        if *value_element == Type::Unit {
            return Ok(fallback_ty.clone());
        }
        if *fallback_element == Type::Unit
            || self.is_assignable(value_element, fallback_element, fallback)
        {
            return Ok(value_ty.clone());
        }
        Err(CompileError::new(
            line,
            format!(
                "orElse fallback mismatch: expected {}, found {}",
                value_ty.name(),
                fallback_ty.name()
            ),
        ))
    }

    fn check_flat_map_result(
        &self,
        element: &Type,
        mapped: Type,
        line: usize,
    ) -> Result<(Type, Type), CompileError> {
        let Some(mapped_element) = option_element_type(&mapped) else {
            return Err(CompileError::new(
                line,
                format!("flatMap mapper must return Option, found {}", mapped.name()),
            ));
        };
        if *mapped_element == Type::Unit {
            return Err(CompileError::new(
                line,
                "flatMap mapper returning None requires a typed Option".to_string(),
            ));
        }
        Ok((element.clone(), mapped))
    }

    fn check_map_mapper(
        &self,
        mapper: &Expr,
        element: &Type,
        method: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        if let Expr::Lambda { params, body } = mapper {
            if params.len() != 1 {
                return Err(CompileError::new(
                    line,
                    format!(
                        "{method} lambda expects 1 parameter, found {}",
                        params.len()
                    ),
                ));
            }
            let (mut checker, _) =
                self.lambda_checker(params, std::slice::from_ref(element), line)?;
            let mut body = (**body).clone();
            if let Some(lifted) = checker.lift_lazy_try_branch(&mut body) {
                return checker.check_expr(&lifted, line);
            }
            return checker.check_expr(&body, line);
        }
        let mapper_ty = self.check_expr(mapper, line)?;
        let Type::Function(params, return_type) = mapper_ty else {
            return Err(CompileError::new(
                line,
                format!(
                    "{method} mapper must be a function, found {}",
                    mapper_ty.name()
                ),
            ));
        };
        if params.len() != 1 {
            return Err(CompileError::new(
                line,
                format!(
                    "{method} mapper expects 1 parameter, found {}",
                    params.len()
                ),
            ));
        }
        if !self.is_assignable(&params[0], element, mapper) {
            return Err(CompileError::new(
                line,
                format!(
                    "{method} mapper parameter must accept {}, found {}",
                    element.name(),
                    params[0].name()
                ),
            ));
        }
        Ok(*return_type)
    }

    fn lower_map_mapper(
        &self,
        mapper: &Expr,
        element: &Type,
        mapped: &Type,
        line: usize,
    ) -> Result<Expr, CompileError> {
        if matches!(mapper, Expr::Lambda { .. }) {
            return self.lower_expr_expected(
                mapper,
                &Type::Function(vec![element.clone()], Box::new(mapped.clone())),
                line,
            );
        }
        self.lower_expr(mapper, line)
    }

    fn check_array_count(
        &self,
        name: &str,
        count: &Expr,
        method: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        let count_ty = self.check_expr(count, line)?;
        if count_ty != Type::Int {
            return Err(CompileError::new(
                line,
                format!("{method} count must be Int, found {}", count_ty.name()),
            ));
        }
        match &binding.ty {
            Type::Array(element) => Ok(Type::Array(element.clone())),
            _ => Err(CompileError::new(
                line,
                format!("type {} has no {method} method", binding.ty.name()),
            )),
        }
    }

    fn check_array_count_value(
        &self,
        value: &Expr,
        count: &Expr,
        method: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let count_ty = self.check_expr(count, line)?;
        if count_ty != Type::Int {
            return Err(CompileError::new(
                line,
                format!("{method} count must be Int, found {}", count_ty.name()),
            ));
        }
        self.check_array_value(value, method, line)
    }

    fn check_array_value(
        &self,
        value: &Expr,
        method: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, line)?;
        let Type::Array(element) = value_ty else {
            return Err(CompileError::new(
                line,
                format!("type {} has no {method} method", value_ty.name()),
            ));
        };
        if !matches!(value, Expr::Array(_)) {
            return Err(CompileError::new(
                line,
                format!("{method} value must be an array literal or named array"),
            ));
        }
        Ok(Type::Array(element))
    }

    fn check_map_value(
        &self,
        value: &Expr,
        method: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, line)?;
        let Type::Map(key, value) = value_ty else {
            return Err(CompileError::new(
                line,
                format!("type {} has no {method} method", value_ty.name()),
            ));
        };
        Ok(Type::Map(key, value))
    }

    fn check_join(&self, name: &str, separator: &Expr, line: usize) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        let separator_ty = self.check_expr(separator, line)?;
        if !self.is_string_like(&separator_ty) {
            return Err(CompileError::new(
                line,
                format!(
                    "join separator must be String or Path, found {}",
                    separator_ty.name()
                ),
            ));
        }
        match &binding.ty {
            Type::Array(element) if self.is_string_like(element) => Ok(Type::String),
            Type::Array(element) => Err(CompileError::new(
                line,
                format!(
                    "join array elements must be String or Path, found {}",
                    element.name()
                ),
            )),
            _ => Err(CompileError::new(
                line,
                format!("type {} has no join method", binding.ty.name()),
            )),
        }
    }

    fn check_join_value(
        &self,
        value: &Expr,
        separator: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        if !matches!(value, Expr::Array(_)) {
            return Err(CompileError::new(
                line,
                "join value must be an array literal or named array".to_string(),
            ));
        }
        let separator_ty = self.check_expr(separator, line)?;
        if !self.is_string_like(&separator_ty) {
            return Err(CompileError::new(
                line,
                format!(
                    "join separator must be String or Path, found {}",
                    separator_ty.name()
                ),
            ));
        }
        match self.check_expr(value, line)? {
            Type::Array(element) if self.is_string_like(&element) => Ok(Type::String),
            Type::Array(element) => Err(CompileError::new(
                line,
                format!(
                    "join array elements must be String or Path, found {}",
                    element.name()
                ),
            )),
            other => Err(CompileError::new(
                line,
                format!("type {} has no join method", other.name()),
            )),
        }
    }

    fn check_array_push(
        &self,
        name: &str,
        value: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        if !binding.mutable {
            return Err(CompileError::new(
                line,
                format!("cannot push to const array `{name}`"),
            ));
        }
        let Type::Array(element) = &binding.ty else {
            return Err(CompileError::new(
                line,
                format!("type {} has no push method", binding.ty.name()),
            ));
        };
        let value_ty = self.check_expr(value, line)?;
        if self.is_assignable(element, &value_ty, value) {
            Ok(Type::Unit)
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "push value type mismatch: expected {}, found {}",
                    element.name(),
                    value_ty.name()
                ),
            ))
        }
    }

    fn check_array_pop(&self, name: &str, line: usize) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        if !binding.mutable {
            return Err(CompileError::new(
                line,
                format!("cannot pop from const array `{name}`"),
            ));
        }
        let Type::Array(_) = &binding.ty else {
            return Err(CompileError::new(
                line,
                format!("type {} has no pop method", binding.ty.name()),
            ));
        };
        Ok(Type::Unit)
    }

    fn check_array_contains(
        &self,
        name: &str,
        value: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        let Type::Array(element) = &binding.ty else {
            return Err(CompileError::new(
                line,
                format!("type {} has no contains method", binding.ty.name()),
            ));
        };
        let value_ty = self.check_expr(value, line)?;
        if self.is_assignable(element, &value_ty, value) {
            Ok(Type::Bool)
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "contains value type mismatch: expected {}, found {}",
                    element.name(),
                    value_ty.name()
                ),
            ))
        }
    }

    fn check_array_contains_value(
        &self,
        value: &Expr,
        item: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        self.check_array_value_item(value, item, "contains", line)
            .map(|_| Type::Bool)
    }

    fn check_index_of(&self, name: &str, value: &Expr, line: usize) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Array(element) => {
                let value_ty = self.check_expr(value, line)?;
                if self.is_assignable(element, &value_ty, value) {
                    Ok(Type::Int)
                } else {
                    Err(CompileError::new(
                        line,
                        format!(
                            "indexOf value type mismatch: expected {}, found {}",
                            element.name(),
                            value_ty.name()
                        ),
                    ))
                }
            }
            ty if self.is_string_like(ty) => self.check_string_index_of(name, value, line),
            _ => Err(CompileError::new(
                line,
                format!("type {} has no indexOf method", binding.ty.name()),
            )),
        }
    }

    fn check_array_index_of_value(
        &self,
        value: &Expr,
        item: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        self.check_array_value_item(value, item, "indexOf", line)
            .map(|_| Type::Int)
    }

    fn check_array_value_item(
        &self,
        value: &Expr,
        item: &Expr,
        method: &str,
        line: usize,
    ) -> Result<(), CompileError> {
        let value_ty = self.check_expr(value, line)?;
        let Type::Array(element) = value_ty else {
            return Err(CompileError::new(
                line,
                format!("type {} has no {method} method", value_ty.name()),
            ));
        };
        if !matches!(value, Expr::Array(_)) {
            return Err(CompileError::new(
                line,
                format!("{method} value must be an array literal or named array"),
            ));
        }
        let item_ty = self.check_expr(item, line)?;
        if self.is_assignable(&element, &item_ty, item) {
            Ok(())
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "{method} value type mismatch: expected {}, found {}",
                    element.name(),
                    item_ty.name()
                ),
            ))
        }
    }

    fn check_map_keys(&self, name: &str, line: usize) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Map(key, _) => Ok(Type::Array(key.clone())),
            _ => Err(CompileError::new(
                line,
                format!("type {} has no keys method", binding.ty.name()),
            )),
        }
    }

    fn check_map_values(&self, name: &str, line: usize) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Map(_, value) => Ok(Type::Array(value.clone())),
            _ => Err(CompileError::new(
                line,
                format!("type {} has no values method", binding.ty.name()),
            )),
        }
    }

    fn check_map_has(&self, name: &str, key: &Expr, line: usize) -> Result<Type, CompileError> {
        let key_ty = self.check_expr(key, line)?;
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Map(expected, _) if self.is_assignable(expected, &key_ty, key) => Ok(Type::Bool),
            Type::Map(expected, _) => Err(CompileError::new(
                line,
                format!(
                    "map key must be {}, found {}",
                    expected.name(),
                    key_ty.name()
                ),
            )),
            _ => Err(CompileError::new(
                line,
                format!("type {} has no has method", binding.ty.name()),
            )),
        }
    }

    fn check_map_keys_value(&self, value: &Expr, line: usize) -> Result<Type, CompileError> {
        match self.check_expr(value, line)? {
            Type::Map(key, _) => Ok(Type::Array(key)),
            other => Err(CompileError::new(
                line,
                format!("type {} has no keys method", other.name()),
            )),
        }
    }

    fn check_map_values_value(&self, value: &Expr, line: usize) -> Result<Type, CompileError> {
        match self.check_expr(value, line)? {
            Type::Map(_, value) => Ok(Type::Array(value)),
            other => Err(CompileError::new(
                line,
                format!("type {} has no values method", other.name()),
            )),
        }
    }

    fn check_map_has_value(
        &self,
        value: &Expr,
        key: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let key_ty = self.check_expr(key, line)?;
        match self.check_expr(value, line)? {
            Type::Map(expected, _) if self.is_assignable(&expected, &key_ty, key) => Ok(Type::Bool),
            Type::Map(expected, _) => Err(CompileError::new(
                line,
                format!(
                    "map key must be {}, found {}",
                    expected.name(),
                    key_ty.name()
                ),
            )),
            other => Err(CompileError::new(
                line,
                format!("type {} has no has method", other.name()),
            )),
        }
    }

    fn check_map_set(
        &self,
        name: &str,
        key: &Expr,
        value: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        if !binding.mutable {
            return Err(CompileError::new(
                line,
                format!("cannot set const map `{name}`"),
            ));
        }
        let Type::Map(expected_key, expected_value) = &binding.ty else {
            return Err(CompileError::new(
                line,
                format!("type {} has no set method", binding.ty.name()),
            ));
        };
        let key_ty = self.check_expr(key, line)?;
        if !self.is_assignable(expected_key, &key_ty, key) {
            return Err(CompileError::new(
                line,
                format!(
                    "map key must be {}, found {}",
                    expected_key.name(),
                    key_ty.name()
                ),
            ));
        }
        let value_ty = self.check_expr(value, line)?;
        if !self.is_assignable(expected_value, &value_ty, value) {
            return Err(CompileError::new(
                line,
                format!(
                    "map value must be {}, found {}",
                    expected_value.name(),
                    value_ty.name()
                ),
            ));
        }
        Ok(Type::Unit)
    }

    fn check_map_remove(&self, name: &str, key: &Expr, line: usize) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        if !binding.mutable {
            return Err(CompileError::new(
                line,
                format!("cannot remove from const map `{name}`"),
            ));
        }
        let Type::Map(expected_key, _) = &binding.ty else {
            return Err(CompileError::new(
                line,
                format!("type {} has no remove method", binding.ty.name()),
            ));
        };
        let key_ty = self.check_expr(key, line)?;
        if self.is_assignable(expected_key, &key_ty, key) {
            Ok(Type::Unit)
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "map key must be {}, found {}",
                    expected_key.name(),
                    key_ty.name()
                ),
            ))
        }
    }

    fn check_string_contains(
        &self,
        name: &str,
        needle: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        self.check_string_predicate(name, needle, "contains", "needle", line)
    }

    fn check_string_index_of(
        &self,
        name: &str,
        needle: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        if !self.is_string_like(&binding.ty) {
            return Err(CompileError::new(
                line,
                format!("type {} has no indexOf method", binding.ty.name()),
            ));
        }
        let needle_ty = self.check_expr(needle, line)?;
        if self.is_string_like(&needle_ty) {
            Ok(Type::Int)
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "indexOf needle must be String or Path, found {}",
                    needle_ty.name()
                ),
            ))
        }
    }

    fn check_string_starts_with(
        &self,
        name: &str,
        prefix: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        self.check_string_predicate(name, prefix, "startsWith", "prefix", line)
    }

    fn check_string_ends_with(
        &self,
        name: &str,
        suffix: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        self.check_string_predicate(name, suffix, "endsWith", "suffix", line)
    }

    fn check_string_predicate(
        &self,
        name: &str,
        value: &Expr,
        method: &str,
        arg_name: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        if let Type::Array(element) = &binding.ty {
            if method != "contains" {
                return Err(CompileError::new(
                    line,
                    format!("type {} has no {method} method", binding.ty.name()),
                ));
            }
            let value_ty = self.check_expr(value, line)?;
            return if self.is_assignable(element, &value_ty, value) {
                Ok(Type::Bool)
            } else {
                Err(CompileError::new(
                    line,
                    format!(
                        "contains value type mismatch: expected {}, found {}",
                        element.name(),
                        value_ty.name()
                    ),
                ))
            };
        }
        if !self.is_string_like(&binding.ty) {
            return Err(CompileError::new(
                line,
                format!("type {} has no {method} method", binding.ty.name()),
            ));
        }
        let value_ty = self.check_expr(value, line)?;
        if self.is_string_like(&value_ty) {
            Ok(Type::Bool)
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "{method} {arg_name} must be String or Path, found {}",
                    value_ty.name()
                ),
            ))
        }
    }

    fn check_string_predicate_value(
        &self,
        receiver: &Expr,
        value: &Expr,
        method: &str,
        arg_name: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let receiver_ty = self.check_expr(receiver, line)?;
        if !self.is_string_like(&receiver_ty) {
            return Err(CompileError::new(
                line,
                format!("type {} has no {method} method", receiver_ty.name()),
            ));
        }
        let value_ty = self.check_expr(value, line)?;
        if self.is_string_like(&value_ty) {
            Ok(Type::Bool)
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "{method} {arg_name} must be String or Path, found {}",
                    value_ty.name()
                ),
            ))
        }
    }

    fn check_string_index_of_value(
        &self,
        receiver: &Expr,
        needle: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let receiver_ty = self.check_expr(receiver, line)?;
        if !self.is_string_like(&receiver_ty) {
            return Err(CompileError::new(
                line,
                format!("type {} has no indexOf method", receiver_ty.name()),
            ));
        }
        let needle_ty = self.check_expr(needle, line)?;
        if self.is_string_like(&needle_ty) {
            Ok(Type::Int)
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "indexOf needle must be String or Path, found {}",
                    needle_ty.name()
                ),
            ))
        }
    }

    fn check_string_trim(&self, name: &str, line: usize) -> Result<Type, CompileError> {
        self.check_string_transform(name, "trim", line)
    }

    fn check_string_to_upper(&self, name: &str, line: usize) -> Result<Type, CompileError> {
        self.check_string_transform(name, "toUpper", line)
    }

    fn check_string_to_lower(&self, name: &str, line: usize) -> Result<Type, CompileError> {
        self.check_string_transform(name, "toLower", line)
    }

    fn check_string_repeat(
        &self,
        name: &str,
        count: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        if !self.is_string_like(&binding.ty) {
            return Err(CompileError::new(
                line,
                format!("type {} has no repeat method", binding.ty.name()),
            ));
        }
        let count_ty = self.check_expr(count, line)?;
        if count_ty == Type::Int {
            Ok(Type::String)
        } else {
            Err(CompileError::new(
                line,
                format!("repeat count must be Int, found {}", count_ty.name()),
            ))
        }
    }

    fn check_string_repeat_value(
        &self,
        value: &Expr,
        count: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, line)?;
        if !self.is_string_like(&value_ty) {
            return Err(CompileError::new(
                line,
                format!("type {} has no repeat method", value_ty.name()),
            ));
        }
        let count_ty = self.check_expr(count, line)?;
        if count_ty == Type::Int {
            Ok(Type::String)
        } else {
            Err(CompileError::new(
                line,
                format!("repeat count must be Int, found {}", count_ty.name()),
            ))
        }
    }

    fn check_string_transform(
        &self,
        name: &str,
        method: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        if self.is_string_like(&binding.ty) {
            Ok(Type::String)
        } else {
            Err(CompileError::new(
                line,
                format!("type {} has no {method} method", binding.ty.name()),
            ))
        }
    }

    fn check_string_transform_value(
        &self,
        value: &Expr,
        method: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, line)?;
        if self.is_string_like(&value_ty) {
            Ok(Type::String)
        } else {
            Err(CompileError::new(
                line,
                format!("type {} has no {method} method", value_ty.name()),
            ))
        }
    }

    fn check_string_split(
        &self,
        name: &str,
        separator: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        if !self.is_string_like(&binding.ty) {
            return Err(CompileError::new(
                line,
                format!("type {} has no split method", binding.ty.name()),
            ));
        }
        let separator_ty = self.check_expr(separator, line)?;
        if self.is_string_like(&separator_ty) {
            Ok(Type::Array(Box::new(Type::String)))
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "split separator must be String or Path, found {}",
                    separator_ty.name()
                ),
            ))
        }
    }

    fn check_string_split_value(
        &self,
        value: &Expr,
        separator: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, line)?;
        if !self.is_string_like(&value_ty) {
            return Err(CompileError::new(
                line,
                format!("type {} has no split method", value_ty.name()),
            ));
        }
        let separator_ty = self.check_expr(separator, line)?;
        if self.is_string_like(&separator_ty) {
            Ok(Type::Array(Box::new(Type::String)))
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "split separator must be String or Path, found {}",
                    separator_ty.name()
                ),
            ))
        }
    }

    fn check_string_replace(
        &self,
        name: &str,
        from: &Expr,
        to: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        if !self.is_string_like(&binding.ty) {
            return Err(CompileError::new(
                line,
                format!("type {} has no replace method", binding.ty.name()),
            ));
        }
        let from_ty = self.check_expr(from, line)?;
        if !self.is_string_like(&from_ty) {
            return Err(CompileError::new(
                line,
                format!(
                    "replace search must be String or Path, found {}",
                    from_ty.name()
                ),
            ));
        }
        let to_ty = self.check_expr(to, line)?;
        if !self.is_string_like(&to_ty) {
            return Err(CompileError::new(
                line,
                format!(
                    "replace replacement must be String or Path, found {}",
                    to_ty.name()
                ),
            ));
        }
        Ok(Type::String)
    }

    fn check_string_replace_value(
        &self,
        value: &Expr,
        from: &Expr,
        to: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, line)?;
        if !self.is_string_like(&value_ty) {
            return Err(CompileError::new(
                line,
                format!("type {} has no replace method", value_ty.name()),
            ));
        }
        let from_ty = self.check_expr(from, line)?;
        if !self.is_string_like(&from_ty) {
            return Err(CompileError::new(
                line,
                format!(
                    "replace search must be String or Path, found {}",
                    from_ty.name()
                ),
            ));
        }
        let to_ty = self.check_expr(to, line)?;
        if !self.is_string_like(&to_ty) {
            return Err(CompileError::new(
                line,
                format!(
                    "replace replacement must be String or Path, found {}",
                    to_ty.name()
                ),
            ));
        }
        Ok(Type::String)
    }

    fn check_path_transform(
        &self,
        name: &str,
        method: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        if self.is_string_like(&binding.ty) {
            Ok(Type::String)
        } else {
            Err(CompileError::new(
                line,
                format!("type {} has no {method} method", binding.ty.name()),
            ))
        }
    }

    fn check_path_transform_value(
        &self,
        value: &Expr,
        method: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, line)?;
        if self.is_string_like(&value_ty) {
            Ok(Type::String)
        } else {
            Err(CompileError::new(
                line,
                format!("type {} has no {method} method", value_ty.name()),
            ))
        }
    }

    fn check_path_predicate(
        &self,
        name: &str,
        method: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        if self.is_string_like(&binding.ty) {
            Ok(Type::Bool)
        } else {
            Err(CompileError::new(
                line,
                format!("type {} has no {method} method", binding.ty.name()),
            ))
        }
    }

    fn check_tuple_field(
        &self,
        name: &str,
        field: usize,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Tuple(elements) => elements.get(field - 1).cloned().ok_or_else(|| {
                CompileError::new(
                    line,
                    format!(
                        "tuple `{name}` has no field _{field}; it has {} fields",
                        elements.len()
                    ),
                )
            }),
            other => Err(CompileError::new(
                line,
                format!(
                    "cannot access tuple field on `{name}` of type {}",
                    other.name()
                ),
            )),
        }
    }

    fn check_tuple_field_value(
        &self,
        value: &Expr,
        field: usize,
        line: usize,
    ) -> Result<Type, CompileError> {
        match self.check_expr(value, line)? {
            Type::Tuple(elements) => elements.get(field - 1).cloned().ok_or_else(|| {
                CompileError::new(
                    line,
                    format!(
                        "tuple value has no field _{field}; it has {} fields",
                        elements.len()
                    ),
                )
            }),
            other => Err(CompileError::new(
                line,
                format!(
                    "cannot access tuple field on value of type {}",
                    other.name()
                ),
            )),
        }
    }

    fn check_field(&self, name: &str, field: &str, line: usize) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            let qualified = format!("{name}.{field}");
            if let Some(sig) = self.functions.get(&qualified) {
                return Ok(sig.function_type());
            }
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Record(fields) => fields
                .iter()
                .find(|(candidate, _)| candidate == field)
                .map(|(_, ty)| ty.clone())
                .ok_or_else(|| {
                    CompileError::new(line, format!("record `{name}` has no field `{field}`"))
                }),
            other => Err(CompileError::new(
                line,
                format!(
                    "cannot access field `{field}` on `{name}` of type {}",
                    other.name()
                ),
            )),
        }
    }

    fn check_field_value(
        &self,
        value: &Expr,
        field: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        if !matches!(value, Expr::Record(_)) {
            let ty = self.check_expr(value, line)?;
            return Err(CompileError::new(
                line,
                format!(
                    "cannot access field `{field}` on value of type {}; use a named record or record literal",
                    ty.name()
                ),
            ));
        }
        match self.check_expr(value, line)? {
            Type::Record(fields) => fields
                .into_iter()
                .find(|(candidate, _)| candidate == field)
                .map(|(_, ty)| ty)
                .ok_or_else(|| {
                    CompileError::new(line, format!("record value has no field `{field}`"))
                }),
            other => Err(CompileError::new(
                line,
                format!(
                    "cannot access field `{field}` on value of type {}",
                    other.name()
                ),
            )),
        }
    }

    fn check_newtype_ctor(
        &self,
        name: &str,
        value: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        if self.constructor_overrides.contains(name) {
            return self.check_call(name, std::slice::from_ref(value), line);
        }
        let Some(ty) = self.types.get(name) else {
            return Err(CompileError::new(line, format!("unknown type `{name}`")));
        };
        let Type::Brand { base, .. } = ty else {
            return Err(CompileError::new(
                line,
                format!("type `{name}` is not a newtype"),
            ));
        };
        let value_ty = self.check_expr(value, line)?;
        if self.is_assignable(base, &value_ty, value) {
            Ok(ty.clone())
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "newtype constructor `{name}` expected {}, found {}",
                    base.name(),
                    value_ty.name()
                ),
            ))
        }
    }

    fn check_call(&self, name: &str, args: &[Expr], line: usize) -> Result<Type, CompileError> {
        if let Some((trait_name, method)) = self.scoped_trait_method_parts(name) {
            let lowered_name = self.resolve_scoped_method_name(trait_name, method, args, line)?;
            return self.check_call(&lowered_name, args, line);
        }
        if let Some((receiver, method)) = method_call_parts(name) {
            if !self.functions.contains_key(name) {
                let lowered_name = self.resolve_method_name(receiver, method, line)?;
                let mut lowered_args = Vec::with_capacity(args.len() + 1);
                lowered_args.push(Expr::Ident(receiver.to_string()));
                lowered_args.extend_from_slice(args);
                return self.check_call(&lowered_name, &lowered_args, line);
            }
        }
        let Some(sig) = self.functions.get(name) else {
            return self.check_function_value_call(name, args, line);
        };
        if !sig.type_params.is_empty() {
            return self.check_generic_call(name, sig, args, line);
        }
        let min_args = sig
            .params
            .iter()
            .filter(|param| !param.variadic)
            .filter(|param| param.default.is_none())
            .count();
        let has_rest = sig.params.last().is_some_and(|param| param.variadic);
        let max_args = if has_rest {
            usize::MAX
        } else {
            sig.params.len()
        };
        if args.len() < min_args || args.len() > max_args {
            let expected = if has_rest {
                format!("{min_args}..")
            } else {
                format!("{min_args}..{}", sig.params.len())
            };
            return Err(CompileError::new(
                line,
                format!(
                    "function `{name}` expects {expected} arguments, found {}",
                    args.len()
                ),
            ));
        }
        for (arg, param) in args
            .iter()
            .zip(sig.params.iter().filter(|param| !param.variadic))
        {
            let actual = self.check_expr_expected(arg, &param.ty, line)?;
            if !self.is_assignable(&param.ty, &actual, arg) {
                return Err(CompileError::new(
                    line,
                    format!(
                        "argument `{}` for function `{name}`: expected {}, found {}",
                        param.name,
                        param.ty.name(),
                        actual.name()
                    ),
                ));
            }
        }
        if let Some(rest_param) = sig.params.last().filter(|param| param.variadic) {
            let Type::Array(element_ty) = &rest_param.ty else {
                return Err(CompileError::new(
                    line,
                    format!("rest parameter `{}` must have Array type", rest_param.name),
                ));
            };
            let fixed_len = sig.params.len() - 1;
            for arg in args.iter().skip(fixed_len) {
                let actual = self.check_expr(arg, line)?;
                if !self.is_assignable(element_ty, &actual, arg) {
                    return Err(CompileError::new(
                        line,
                        format!(
                            "rest argument for function `{name}`: expected {}, found {}",
                            element_ty.name(),
                            actual.name()
                        ),
                    ));
                }
            }
        }
        Ok(sig.return_type.clone())
    }

    fn check_generic_call(
        &self,
        name: &str,
        sig: &FunctionSig,
        args: &[Expr],
        line: usize,
    ) -> Result<Type, CompileError> {
        let has_rest = sig.params.last().is_some_and(|param| param.variadic);
        let fixed_len = if has_rest {
            sig.params.len() - 1
        } else {
            sig.params.len()
        };
        if (!has_rest && args.len() != sig.params.len()) || (has_rest && args.len() < fixed_len) {
            let expected = if has_rest {
                format!("{fixed_len}..")
            } else {
                sig.params.len().to_string()
            };
            return Err(CompileError::new(
                line,
                format!(
                    "generic function `{name}` expects {expected} arguments, found {}",
                    args.len()
                ),
            ));
        }
        let mut inferred = HashMap::new();
        for (arg, param) in args.iter().take(fixed_len).zip(&sig.params[..fixed_len]) {
            let actual = self.check_expr(arg, line)?;
            self.infer_generic_type(&param.ty, &actual, arg, &mut inferred)
                .map_err(|message| {
                    CompileError::new(
                        line,
                        format!("argument `{}` for function `{name}`: {message}", param.name),
                    )
                })?;
        }
        if let Some(rest_param) = sig.params.last().filter(|param| param.variadic) {
            let Type::Array(element_ty) = &rest_param.ty else {
                return Err(CompileError::new(
                    line,
                    format!("rest parameter `{}` must have Array type", rest_param.name),
                ));
            };
            for arg in args.iter().skip(fixed_len) {
                let actual = self.check_expr(arg, line)?;
                self.infer_generic_type(element_ty, &actual, arg, &mut inferred)
                    .map_err(|message| {
                        CompileError::new(
                            line,
                            format!(
                                "rest argument for function `{name}` parameter `{}`: {message}",
                                rest_param.name
                            ),
                        )
                    })?;
            }
        }
        for type_param in &sig.type_params {
            let Some(inferred_ty) = inferred.get(&type_param.name) else {
                return Err(CompileError::new(
                    line,
                    format!(
                        "could not infer generic type `{}` for function `{name}`",
                        type_param.name
                    ),
                ));
            };
            for bound in &type_param.bounds {
                if !self
                    .trait_impls
                    .contains(&(bound.clone(), inferred_ty.name()))
                {
                    return Err(CompileError::new(
                        line,
                        format!(
                            "type {} does not implement trait `{bound}`",
                            inferred_ty.name()
                        ),
                    ));
                }
            }
        }
        Ok(substitute_generics(&sig.return_type, &inferred))
    }

    fn infer_generic_type(
        &self,
        expected: &Type,
        actual: &Type,
        expr: &Expr,
        inferred: &mut HashMap<String, Type>,
    ) -> Result<(), String> {
        match expected {
            Type::Generic(name) => {
                if let Some(existing) = inferred.get(name) {
                    if self.is_assignable(existing, actual, expr)
                        && self.is_assignable(actual, existing, expr)
                    {
                        Ok(())
                    } else {
                        Err(format!(
                            "generic type `{name}` expected {}, found {}",
                            existing.name(),
                            actual.name()
                        ))
                    }
                } else {
                    inferred.insert(name.clone(), actual.clone());
                    Ok(())
                }
            }
            Type::Array(expected) => {
                let Type::Array(actual) = actual else {
                    return Err(format!(
                        "expected {}, found {}",
                        expected.name(),
                        actual.name()
                    ));
                };
                self.infer_generic_type(expected, actual, expr, inferred)
            }
            Type::Future(expected) => {
                let Type::Future(actual) = actual else {
                    return Err(format!(
                        "expected {}, found {}",
                        expected.name(),
                        actual.name()
                    ));
                };
                self.infer_generic_type(expected, actual, expr, inferred)
            }
            Type::Map(expected_key, expected_value) => {
                let Type::Map(actual_key, actual_value) = actual else {
                    return Err(format!(
                        "expected {}, found {}",
                        Type::Map(expected_key.clone(), expected_value.clone()).name(),
                        actual.name()
                    ));
                };
                self.infer_generic_type(expected_key, actual_key, expr, inferred)?;
                self.infer_generic_type(expected_value, actual_value, expr, inferred)
            }
            Type::Tuple(expected) => {
                let Type::Tuple(actual) = actual else {
                    return Err(format!(
                        "expected {}, found {}",
                        Type::Tuple(expected.clone()).name(),
                        actual.name()
                    ));
                };
                if expected.len() != actual.len() {
                    return Err(format!(
                        "expected {}, found {}",
                        Type::Tuple(expected.clone()).name(),
                        Type::Tuple(actual.clone()).name()
                    ));
                }
                for (expected, actual) in expected.iter().zip(actual) {
                    self.infer_generic_type(expected, actual, expr, inferred)?;
                }
                Ok(())
            }
            Type::Record(expected) => {
                let Type::Record(actual) = actual else {
                    return Err(format!(
                        "expected {}, found {}",
                        Type::Record(expected.clone()).name(),
                        actual.name()
                    ));
                };
                for (field, expected_ty) in expected {
                    let Some((_, actual_ty)) = actual
                        .iter()
                        .find(|(actual_field, _)| actual_field == field)
                    else {
                        return Err(format!("record field `{field}` is missing"));
                    };
                    self.infer_generic_type(expected_ty, actual_ty, expr, inferred)?;
                }
                Ok(())
            }
            Type::Function(expected_params, expected_return) => {
                let Type::Function(actual_params, actual_return) = actual else {
                    return Err(format!(
                        "expected {}, found {}",
                        expected.name(),
                        actual.name()
                    ));
                };
                if expected_params.len() != actual_params.len() {
                    return Err(format!(
                        "expected {}, found {}",
                        expected.name(),
                        actual.name()
                    ));
                }
                for (expected, actual) in expected_params.iter().zip(actual_params) {
                    self.infer_generic_type(expected, actual, expr, inferred)?;
                }
                self.infer_generic_type(expected_return, actual_return, expr, inferred)
            }
            Type::Union(expected_types) => {
                for expected in expected_types {
                    let mut candidate = inferred.clone();
                    if self
                        .infer_generic_type(expected, actual, expr, &mut candidate)
                        .is_ok()
                    {
                        *inferred = candidate;
                        return Ok(());
                    }
                }
                Err(format!(
                    "expected {}, found {}",
                    expected.name(),
                    actual.name()
                ))
            }
            Type::Intersection(expected_types) => {
                for expected in expected_types {
                    self.infer_generic_type(expected, actual, expr, inferred)?;
                }
                Ok(())
            }
            _ if self.is_assignable(expected, actual, expr) => Ok(()),
            _ => Err(format!(
                "expected {}, found {}",
                expected.name(),
                actual.name()
            )),
        }
    }

    fn check_function_value_call(
        &self,
        name: &str,
        args: &[Expr],
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined function `{name}`"),
            ));
        };
        let Type::Function(params, return_type) = &binding.ty else {
            return Err(CompileError::new(line, format!("`{name}` is not callable")));
        };
        if args.len() != params.len() {
            return Err(CompileError::new(
                line,
                format!(
                    "function value `{name}` expects {} arguments, found {}",
                    params.len(),
                    args.len()
                ),
            ));
        }
        for (index, (arg, expected)) in args.iter().zip(params).enumerate() {
            let actual = self.check_expr_expected(arg, expected, line)?;
            if !self.is_assignable(expected, &actual, arg) {
                return Err(CompileError::new(
                    line,
                    format!(
                        "argument {} for function value `{name}`: expected {}, found {}",
                        index + 1,
                        expected.name(),
                        actual.name()
                    ),
                ));
            }
        }
        Ok((**return_type).clone())
    }

    fn check_variant(&self, name: &str, args: &[Expr], line: usize) -> Result<Type, CompileError> {
        let Some(variant) = self.variants.get(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variant `{name}`"),
            ));
        };
        if args.len() != variant.fields.len() {
            return Err(CompileError::new(
                line,
                format!(
                    "variant `{name}` expects {} arguments, found {}",
                    variant.fields.len(),
                    args.len()
                ),
            ));
        }
        for (index, (arg, expected)) in args.iter().zip(&variant.fields).enumerate() {
            let actual = self.check_expr_expected(arg, expected, line)?;
            if !self.is_assignable(expected, &actual, arg) {
                return Err(CompileError::new(
                    line,
                    format!(
                        "argument {} for variant `{name}`: expected {}, found {}",
                        index + 1,
                        expected.name(),
                        actual.name()
                    ),
                ));
            }
        }
        Ok(Type::Named(variant.sum_type.clone()))
    }

    fn check_if_expr(
        &self,
        condition: &Expr,
        then_expr: &Expr,
        else_expr: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        self.check_condition(condition, line)?;
        let then_ty = self.check_expr(then_expr, line)?;
        let else_ty = self.check_expr(else_expr, line)?;
        if self.is_assignable(&then_ty, &else_ty, else_expr) {
            Ok(then_ty)
        } else if self.is_assignable(&else_ty, &then_ty, then_expr) {
            Ok(else_ty)
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "if expression branches must have matching types, found {} and {}",
                    then_ty.name(),
                    else_ty.name()
                ),
            ))
        }
    }

    fn check_match(
        &self,
        value: &Expr,
        arms: &[MatchArm],
        line: usize,
    ) -> Result<Type, CompileError> {
        if arms.is_empty() {
            return Err(CompileError::new(
                line,
                "match expression requires at least one arm".to_string(),
            ));
        }
        let value_ty = if matches!(value, Expr::Command { .. } | Expr::Pipeline { .. }) {
            command_result_type()
        } else {
            self.check_expr(value, line)?
        };
        let mut first_ty = None;
        let mut guard_error = None;
        for arm in arms {
            let mut arm_checker = self.clone();
            if let Some(pattern) = &arm.pattern {
                for (name, ty) in self.check_match_pattern(pattern, value, &value_ty, line)? {
                    arm_checker.define(&name, ty, false, line)?;
                }
            }
            if let Some(guard) = &arm.guard {
                arm_checker.check_condition(guard, line)?;
                if let Expr::MatchGuardResult(value) = guard {
                    let ty = arm_checker.check_expr(value, line)?;
                    let (_, error) =
                        result_types(&ty).expect("MatchGuardResult requires Result value");
                    guard_error = Some(match guard_error {
                        None => error.clone(),
                        Some(Type::Unit) => error.clone(),
                        Some(existing) if *error == Type::Unit => existing,
                        Some(existing)
                            if self.is_assignable(&existing, error, value)
                                && self.is_assignable(error, &existing, value) =>
                        {
                            existing
                        }
                        Some(existing) => {
                            return Err(CompileError::new(
                                line,
                                format!(
                                    "match guard error types must match, found {} and {}",
                                    existing.name(),
                                    error.name()
                                ),
                            ));
                        }
                    });
                }
            }
            let arm_ty = arm_checker.check_expr(&arm.expr, line)?;
            if let Some(expected) = &first_ty {
                if self.is_assignable(expected, &arm_ty, &arm.expr) {
                    continue;
                }
                if self.is_assignable(&arm_ty, expected, &arm.expr) {
                    first_ty = Some(arm_ty);
                } else {
                    return Err(CompileError::new(
                        line,
                        format!(
                            "match arms must have matching types, found {} and {}",
                            expected.name(),
                            arm_ty.name()
                        ),
                    ));
                }
            } else {
                first_ty = Some(arm_ty);
            }
        }
        if !arms
            .iter()
            .any(|arm| arm.pattern.is_none() && arm.guard.is_none())
        {
            match self.missing_match_cases(&value_ty, arms) {
                Some(missing) if missing.is_empty() => {}
                Some(missing) => {
                    return Err(CompileError::new(
                        line,
                        format!(
                            "non-exhaustive match; missing cases: {}",
                            missing.join(", ")
                        ),
                    ));
                }
                None => {
                    return Err(CompileError::new(
                        line,
                        "match expression requires wildcard `_` arm".to_string(),
                    ));
                }
            }
        }
        let first_ty = first_ty.expect("match expression has at least one arm");
        if let Some(guard_error) = guard_error {
            let Some((ok, error)) = result_types(&first_ty) else {
                unreachable!("propagating match guards lift arm results");
            };
            if *error != Type::Unit
                && guard_error != Type::Unit
                && !self.is_assignable(error, &guard_error, value)
            {
                return Err(CompileError::new(
                    line,
                    format!(
                        "match guard error {} is not assignable to arm error {}",
                        guard_error.name(),
                        error.name()
                    ),
                ));
            }
            return Ok(Type::Applied(
                "Result".to_string(),
                vec![
                    ok.clone(),
                    if *error == Type::Unit {
                        guard_error
                    } else {
                        error.clone()
                    },
                ],
            ));
        }
        Ok(first_ty)
    }

    fn check_match_pattern(
        &self,
        pattern: &Expr,
        _value: &Expr,
        value_ty: &Type,
        line: usize,
    ) -> Result<Vec<(String, Type)>, CompileError> {
        match pattern {
            Expr::Call { name, args } | Expr::Variant { name, args, .. }
                if self.variants.contains_key(name) =>
            {
                self.check_variant_pattern(name, args, value_ty, line)
            }
            Expr::Ident(name)
                if self
                    .variants
                    .get(name)
                    .is_some_and(|variant| variant.fields.is_empty()) =>
            {
                self.check_variant_pattern(name, &[], value_ty, line)
            }
            Expr::NewtypeCtor { name, value } if self.variants.contains_key(name) => self
                .check_variant_pattern(name, std::slice::from_ref(value.as_ref()), value_ty, line),
            Expr::Some(inner) => {
                let Some(element_ty) = option_element_type(value_ty) else {
                    return Err(match_pattern_mismatch(line, value_ty, pattern));
                };
                self.check_match_constructor_payload(inner, element_ty, line)
            }
            Expr::None => {
                if option_element_type(value_ty).is_some() {
                    Ok(Vec::new())
                } else {
                    Err(match_pattern_mismatch(line, value_ty, pattern))
                }
            }
            Expr::Ok(inner) => {
                let Some((ok_ty, _)) = result_types(value_ty) else {
                    return Err(match_pattern_mismatch(line, value_ty, pattern));
                };
                self.check_match_constructor_payload(inner, ok_ty, line)
            }
            Expr::Err(inner) => {
                let Some((_, err_ty)) = result_types(value_ty) else {
                    return Err(match_pattern_mismatch(line, value_ty, pattern));
                };
                self.check_match_constructor_payload(inner, err_ty, line)
            }
            Expr::Tuple(patterns) => {
                let Type::Tuple(elements) = value_ty else {
                    return Err(match_pattern_mismatch(line, value_ty, pattern));
                };
                if patterns.len() != elements.len() {
                    return Err(CompileError::new(
                        line,
                        format!(
                            "match pattern type mismatch: expected {}, found {}",
                            value_ty.name(),
                            Type::Tuple(
                                patterns
                                    .iter()
                                    .map(|pattern| self
                                        .check_expr(pattern, line)
                                        .unwrap_or(Type::Unit))
                                    .collect()
                            )
                            .name()
                        ),
                    ));
                }
                let mut bindings = Vec::new();
                for (pattern, expected_ty) in patterns.iter().zip(elements) {
                    if matches!(pattern, Expr::Ident(name) if name == "_") {
                        continue;
                    }
                    if let Expr::Ident(name) = pattern {
                        bindings.push((name.clone(), expected_ty.clone()));
                        continue;
                    }
                    let pattern_ty = self.check_expr(pattern, line)?;
                    if !self.is_comparable_by_equality(expected_ty, &pattern_ty)
                        && !self.is_assignable(expected_ty, &pattern_ty, pattern)
                    {
                        return Err(CompileError::new(
                            line,
                            format!(
                                "match pattern type mismatch: expected {}, found {}",
                                expected_ty.name(),
                                pattern_ty.name()
                            ),
                        ));
                    }
                }
                Ok(bindings)
            }
            Expr::RecordPattern(patterns) => {
                let Type::Record(fields) = value_ty else {
                    return Err(match_pattern_mismatch(line, value_ty, pattern));
                };
                let mut bindings = Vec::new();
                for (field, pattern) in patterns {
                    let Some((_, expected_ty)) =
                        fields.iter().find(|(candidate, _)| candidate == field)
                    else {
                        return Err(CompileError::new(
                            line,
                            format!("match record pattern field `{field}` is missing"),
                        ));
                    };
                    match pattern {
                        None => bindings.push((field.clone(), expected_ty.clone())),
                        Some(Expr::Ident(name)) if name == "_" => {}
                        Some(Expr::Ident(name)) => {
                            bindings.push((name.clone(), expected_ty.clone()));
                        }
                        Some(pattern) => {
                            let pattern_ty = self.check_expr(pattern, line)?;
                            if !self.is_comparable_by_equality(expected_ty, &pattern_ty)
                                && !self.is_assignable(expected_ty, &pattern_ty, pattern)
                            {
                                return Err(CompileError::new(
                                    line,
                                    format!(
                                        "match pattern type mismatch: expected {}, found {}",
                                        expected_ty.name(),
                                        pattern_ty.name()
                                    ),
                                ));
                            }
                        }
                    }
                }
                Ok(bindings)
            }
            _ => {
                let pattern_ty = self.check_expr(pattern, line)?;
                if self.is_comparable_by_equality(value_ty, &pattern_ty) {
                    Ok(Vec::new())
                } else {
                    Err(CompileError::new(
                        line,
                        format!(
                            "match pattern type mismatch: expected {}, found {}",
                            value_ty.name(),
                            pattern_ty.name()
                        ),
                    ))
                }
            }
        }
    }

    fn missing_match_cases(&self, value_ty: &Type, arms: &[MatchArm]) -> Option<Vec<String>> {
        let covered = arms
            .iter()
            .filter(|arm| arm.guard.is_none())
            .filter_map(|arm| arm.pattern.as_ref())
            .collect::<Vec<_>>();
        let expected = match value_ty {
            Type::Bool => vec!["true".to_string(), "false".to_string()],
            Type::Applied(name, args) if name == "Option" && args.len() == 1 => {
                vec!["Some".to_string(), "None".to_string()]
            }
            Type::Applied(name, args) if name == "Result" && args.len() == 2 => {
                vec!["Ok".to_string(), "Err".to_string()]
            }
            Type::Named(name) => self.sum_types.get(name)?.clone(),
            _ => return None,
        };
        Some(
            expected
                .into_iter()
                .filter(|case| {
                    !covered
                        .iter()
                        .any(|pattern| self.pattern_covers_case(pattern, case))
                })
                .collect(),
        )
    }

    fn pattern_covers_case(&self, pattern: &Expr, case: &str) -> bool {
        match pattern {
            Expr::Bool(true) => case == "true",
            Expr::Bool(false) => case == "false",
            Expr::Some(_) => case == "Some",
            Expr::None => case == "None",
            Expr::Ok(_) => case == "Ok",
            Expr::Err(_) => case == "Err",
            Expr::Call { name, .. }
            | Expr::Variant { name, .. }
            | Expr::NewtypeCtor { name, .. } => name == case,
            Expr::Ident(name)
                if self
                    .variants
                    .get(name)
                    .is_some_and(|variant| variant.fields.is_empty()) =>
            {
                name == case
            }
            _ => false,
        }
    }

    fn check_variant_pattern(
        &self,
        name: &str,
        args: &[Expr],
        value_ty: &Type,
        line: usize,
    ) -> Result<Vec<(String, Type)>, CompileError> {
        let Some(variant) = self.variants.get(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variant `{name}`"),
            ));
        };
        if value_ty != &Type::Named(variant.sum_type.clone()) {
            return Err(CompileError::new(
                line,
                format!(
                    "match variant `{name}` belongs to {}, found {}",
                    variant.sum_type,
                    value_ty.name()
                ),
            ));
        }
        if args.len() != variant.fields.len() {
            return Err(CompileError::new(
                line,
                format!(
                    "variant pattern `{name}` expects {} arguments, found {}",
                    variant.fields.len(),
                    args.len()
                ),
            ));
        }
        let mut bindings = Vec::new();
        for (pattern, expected) in args.iter().zip(&variant.fields) {
            if matches!(pattern, Expr::Ident(candidate) if candidate == "_") {
                continue;
            }
            if let Expr::Ident(binding) = pattern {
                bindings.push((binding.clone(), expected.clone()));
                continue;
            }
            let actual = self.check_expr(pattern, line)?;
            if !self.is_comparable_by_equality(expected, &actual)
                && !self.is_assignable(expected, &actual, pattern)
            {
                return Err(CompileError::new(
                    line,
                    format!(
                        "match pattern type mismatch: expected {}, found {}",
                        expected.name(),
                        actual.name()
                    ),
                ));
            }
        }
        Ok(bindings)
    }

    fn check_match_constructor_payload(
        &self,
        payload: &Expr,
        expected_ty: &Type,
        line: usize,
    ) -> Result<Vec<(String, Type)>, CompileError> {
        if let Expr::Ident(name) = payload {
            return if name == "_" {
                Ok(Vec::new())
            } else {
                Ok(vec![(name.clone(), expected_ty.clone())])
            };
        }
        if matches!(payload, Expr::Tuple(_) | Expr::RecordPattern(_)) {
            return self.check_match_pattern(payload, payload, expected_ty, line);
        }
        let payload_ty = self.check_expr(payload, line)?;
        if self.is_comparable_by_equality(expected_ty, &payload_ty)
            || self.is_assignable(expected_ty, &payload_ty, payload)
        {
            Ok(Vec::new())
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "match pattern type mismatch: expected {}, found {}",
                    expected_ty.name(),
                    payload_ty.name()
                ),
            ))
        }
    }

    fn check_default(
        &self,
        value: &Expr,
        fallback: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, line)?;
        if matches!(value, Expr::Command { .. } | Expr::Pipeline { .. }) {
            let fallback_ty = self.check_expr(fallback, line)?;
            return if self.is_assignable(&Type::String, &fallback_ty, fallback) {
                Ok(Type::String)
            } else {
                Err(CompileError::new(
                    line,
                    format!(
                        "operator `??` fallback mismatch: expected String, found {}",
                        fallback_ty.name()
                    ),
                ))
            };
        }
        let Some(element_ty) = default_success_type(&value_ty) else {
            return Err(CompileError::new(
                line,
                format!(
                    "operator `??` requires Option or Result value, found {}",
                    value_ty.name()
                ),
            ));
        };
        let fallback_ty = self.check_expr(fallback, line)?;
        if self.is_assignable(element_ty, &fallback_ty, fallback) {
            Ok(element_ty.clone())
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "operator `??` fallback mismatch: expected {}, found {}",
                    element_ty.name(),
                    fallback_ty.name()
                ),
            ))
        }
    }

    fn check_default_try(
        &self,
        value: &Expr,
        fallback: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let success_ty = if matches!(value, Expr::Command { .. } | Expr::Pipeline { .. }) {
            Type::String
        } else {
            let value_ty = self.check_expr(value, line)?;
            let Some(success_ty) = default_success_type(&value_ty) else {
                return Err(CompileError::new(
                    line,
                    format!(
                        "operator `??` requires Option or Result value, found {}",
                        value_ty.name()
                    ),
                ));
            };
            success_ty.clone()
        };
        let fallback_ty = self.check_expr(fallback, line)?;
        let Some((fallback_ok, fallback_error)) = result_types(&fallback_ty) else {
            return Err(CompileError::new(
                line,
                format!(
                    "propagating `??` fallback must return Result, found {}",
                    fallback_ty.name()
                ),
            ));
        };
        if !self.is_assignable(&success_ty, fallback_ok, fallback) {
            return Err(CompileError::new(
                line,
                format!(
                    "operator `??` fallback mismatch: expected {}, found {}",
                    success_ty.name(),
                    fallback_ok.name()
                ),
            ));
        }
        Ok(Type::Applied(
            "Result".to_string(),
            vec![success_ty, fallback_error.clone()],
        ))
    }

    fn check_result_option(&self, value: &Expr, line: usize) -> Result<Type, CompileError> {
        if matches!(value, Expr::Command { .. } | Expr::Pipeline { .. }) {
            return Ok(Type::Applied("Option".to_string(), vec![Type::String]));
        }
        let value_ty = self.check_expr(value, line)?;
        let Some((ok_ty, _)) = result_types(&value_ty) else {
            return Err(CompileError::new(
                line,
                format!(
                    "operator `?` requires Result value, found {}",
                    value_ty.name()
                ),
            ));
        };
        Ok(Type::Applied("Option".to_string(), vec![ok_ty.clone()]))
    }

    fn check_value_access(&self, name: &str, line: usize) -> Result<Type, CompileError> {
        let Some(binding) = self.bindings.get(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Brand { base, .. } => Ok((**base).clone()),
            other => Err(CompileError::new(
                line,
                format!(
                    "cannot access `.value` on `{name}` of type {}",
                    other.name()
                ),
            )),
        }
    }

    fn resolve_type(&self, ty: &Type, line: usize) -> Result<Type, CompileError> {
        self.resolve_type_with_generics(ty, &HashSet::new(), line)
    }

    fn resolve_type_with_generics(
        &self,
        ty: &Type,
        generics: &HashSet<String>,
        line: usize,
    ) -> Result<Type, CompileError> {
        match ty {
            Type::Named(name) if generics.contains(name) => Ok(Type::Generic(name.clone())),
            Type::Named(name) => self
                .types
                .get(name)
                .cloned()
                .ok_or_else(|| CompileError::new(line, format!("unknown type `{name}`"))),
            Type::Applied(name, args) if name == "Option" && args.len() == 1 => Ok(Type::Applied(
                name.clone(),
                vec![self.resolve_type_with_generics(&args[0], generics, line)?],
            )),
            Type::Applied(name, args) if name == "Result" && args.len() == 2 => Ok(Type::Applied(
                name.clone(),
                vec![
                    self.resolve_type_with_generics(&args[0], generics, line)?,
                    self.resolve_type_with_generics(&args[1], generics, line)?,
                ],
            )),
            Type::Applied(name, args) => {
                let Some((type_params, body)) = self.generic_types.get(name) else {
                    return Err(CompileError::new(line, format!("unknown type `{name}`")));
                };
                if args.len() != type_params.len() {
                    return Err(CompileError::new(
                        line,
                        format!(
                            "type `{name}` expects {} type arguments, found {}",
                            type_params.len(),
                            args.len()
                        ),
                    ));
                }
                let mut inferred = HashMap::new();
                for (type_param, arg) in type_params.iter().zip(args) {
                    inferred.insert(
                        type_param.clone(),
                        self.resolve_type_with_generics(arg, generics, line)?,
                    );
                }
                Ok(substitute_generics(body, &inferred))
            }
            Type::Array(element) => Ok(Type::Array(Box::new(
                self.resolve_type_with_generics(element, generics, line)?,
            ))),
            Type::Future(value) => Ok(Type::Future(Box::new(
                self.resolve_type_with_generics(value, generics, line)?,
            ))),
            Type::Map(key, value) => Ok(Type::Map(
                Box::new(self.resolve_type_with_generics(key, generics, line)?),
                Box::new(self.resolve_type_with_generics(value, generics, line)?),
            )),
            Type::Record(fields) => {
                let mut resolved = Vec::new();
                for (name, ty) in fields {
                    resolved.push((
                        name.clone(),
                        self.resolve_type_with_generics(ty, generics, line)?,
                    ));
                }
                Ok(Type::Record(resolved))
            }
            Type::Tuple(elements) => {
                let mut resolved = Vec::new();
                for element in elements {
                    resolved.push(self.resolve_type_with_generics(element, generics, line)?);
                }
                Ok(Type::Tuple(resolved))
            }
            Type::Function(params, return_type) => {
                let mut resolved_params = Vec::new();
                for param in params {
                    resolved_params.push(self.resolve_type_with_generics(param, generics, line)?);
                }
                Ok(Type::Function(
                    resolved_params,
                    Box::new(self.resolve_type_with_generics(return_type, generics, line)?),
                ))
            }
            Type::Union(types) => {
                let mut resolved = Vec::new();
                for ty in types {
                    resolved.push(self.resolve_type_with_generics(ty, generics, line)?);
                }
                Ok(Type::Union(resolved))
            }
            Type::Intersection(types) => {
                let mut resolved = Vec::new();
                for ty in types {
                    resolved.push(self.resolve_type_with_generics(ty, generics, line)?);
                }
                Ok(Type::Intersection(resolved))
            }
            other => Ok(other.clone()),
        }
    }

    fn is_assignable(&self, expected: &Type, actual: &Type, expr: &Expr) -> bool {
        if expected == actual {
            return true;
        }
        match (expected, actual) {
            (Type::Union(types), actual) => types
                .iter()
                .any(|expected| self.is_assignable(expected, actual, expr)),
            (expected, Type::Union(types)) => types
                .iter()
                .all(|actual| self.is_assignable(expected, actual, expr)),
            (Type::Intersection(types), actual) => types
                .iter()
                .all(|expected| self.is_assignable(expected, actual, expr)),
            (expected, Type::Intersection(types)) => types
                .iter()
                .any(|actual| self.is_assignable(expected, actual, expr)),
            (Type::Float, Type::Int) => true,
            (Type::Path, Type::String) => true,
            (Type::String, Type::Path) => true,
            (Type::ExitCode, Type::Int) => {
                matches!(expr, Expr::Int(value) if (0..=255).contains(value))
            }
            (Type::Array(expected), Type::Array(actual)) if **actual == Type::Unit => {
                !matches!(**expected, Type::Unit)
            }
            (Type::Array(expected), Type::Array(actual)) => {
                self.is_assignable(expected, actual, expr)
            }
            (expected, Type::Applied(name, args))
                if name == "Option" && args.len() == 1 && args[0] == Type::Unit =>
            {
                option_element_type(expected).is_some()
            }
            (expected, Type::Applied(name, args))
                if name == "Result" && args.len() == 2 && args[1] == Type::Unit =>
            {
                result_types(expected).is_some_and(|(ok, _)| self.is_assignable(ok, &args[0], expr))
            }
            (expected, Type::Applied(name, args))
                if name == "Result" && args.len() == 2 && args[0] == Type::Unit =>
            {
                result_types(expected)
                    .is_some_and(|(_, err)| self.is_assignable(err, &args[1], expr))
            }
            (
                Type::Applied(expected_name, expected_args),
                Type::Applied(actual_name, actual_args),
            ) if expected_name == "Option"
                && actual_name == "Option"
                && expected_args.len() == 1
                && actual_args.len() == 1 =>
            {
                self.is_assignable(&expected_args[0], &actual_args[0], expr)
            }
            (
                Type::Applied(expected_name, expected_args),
                Type::Applied(actual_name, actual_args),
            ) if expected_name == "Result"
                && actual_name == "Result"
                && expected_args.len() == 2
                && actual_args.len() == 2 =>
            {
                self.is_assignable(&expected_args[0], &actual_args[0], expr)
                    && self.is_assignable(&expected_args[1], &actual_args[1], expr)
            }
            (Type::Future(expected), Type::Future(actual)) => {
                self.is_assignable(expected, actual, expr)
            }
            (Type::Map(expected_key, expected_value), Type::Map(actual_key, actual_value))
                if **actual_key == Type::Unit && **actual_value == Type::Unit =>
            {
                !matches!(**expected_key, Type::Unit) && !matches!(**expected_value, Type::Unit)
            }
            (Type::Map(expected_key, expected_value), Type::Map(actual_key, actual_value)) => {
                self.is_assignable(expected_key, actual_key, expr)
                    && self.is_assignable(expected_value, actual_value, expr)
            }
            (Type::Record(expected), Type::Record(actual)) if expected.len() == actual.len() => {
                expected.iter().all(|(name, expected_ty)| {
                    actual
                        .iter()
                        .find(|(actual_name, _)| actual_name == name)
                        .is_some_and(|(_, actual_ty)| {
                            self.is_assignable(expected_ty, actual_ty, expr)
                        })
                })
            }
            (Type::Tuple(expected), Type::Tuple(actual)) if expected.len() == actual.len() => {
                expected
                    .iter()
                    .zip(actual)
                    .all(|(expected, actual)| self.is_assignable(expected, actual, expr))
            }
            (
                Type::Function(expected_params, expected_return),
                Type::Function(actual_params, actual_return),
            ) if expected_params.len() == actual_params.len() => {
                expected_params
                    .iter()
                    .zip(actual_params)
                    .all(|(expected, actual)| self.is_assignable(expected, actual, expr))
                    && self.is_assignable(expected_return, actual_return, expr)
            }
            (Type::Brand { name: expected, .. }, Type::Brand { name: actual, .. }) => {
                expected == actual
            }
            _ => false,
        }
    }

    fn is_numeric(&self, ty: &Type) -> bool {
        matches!(ty, Type::Int | Type::Float | Type::ExitCode)
    }

    fn is_integer_numeric(&self, ty: &Type) -> bool {
        matches!(ty, Type::Int | Type::ExitCode)
    }

    fn is_string_like(&self, ty: &Type) -> bool {
        matches!(ty, Type::String | Type::Path)
    }

    fn is_castable(&self, target: &Type, actual: &Type, expr: &Expr) -> bool {
        if self.is_assignable(target, actual, expr) || self.is_assignable(actual, target, expr) {
            return true;
        }
        match (target, actual) {
            (Type::Brand { base, .. }, _) => self.is_assignable(base, actual, expr),
            (_, Type::Brand { base, .. }) => self.is_assignable(target, base, expr),
            _ => false,
        }
    }

    fn is_comparable_by_equality(&self, left: &Type, right: &Type) -> bool {
        left == right
            || (self.is_numeric(left) && self.is_numeric(right))
            || matches!(
                (left, right),
                (Type::String, Type::Path) | (Type::Path, Type::String)
            )
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
