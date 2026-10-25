use super::TypeChecker;
use crate::{DoStep, Expr, Program, Statement};

impl TypeChecker {
    pub(super) fn expand_nested_try_statement(&mut self, statement: &Statement) -> Vec<Statement> {
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
            Statement::Defer(statement) => {
                **statement = self
                    .expand_nested_try_statement(statement)
                    .pop()
                    .expect("defer statement expansion preserves a statement");
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

    pub(super) fn expand_nested_try_program(&mut self, program: &Program) -> Program {
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

    pub(super) fn extract_nested_try_results(
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
            | Expr::Async(value)
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
            | Expr::ArrayPattern {
                patterns: values, ..
            }
            | Expr::Variant { args: values, .. }
            | Expr::Call { args: values, .. }
            | Expr::AllowedCommand { args: values, .. } => {
                for value in values {
                    self.extract_nested_try_results(value, false, prefix);
                }
            }
            Expr::AliasPattern { pattern, .. } => {
                self.extract_nested_try_results(pattern, false, prefix);
            }
            Expr::Range { start, end, .. } => {
                self.extract_nested_try_results(start, false, prefix);
                self.extract_nested_try_results(end, false, prefix);
            }
            Expr::NamedArg { value, .. } => {
                self.extract_nested_try_results(value, false, prefix);
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
            | Expr::ArrayFilter {
                predicate: index, ..
            }
            | Expr::ArrayFlatMap { mapper: index, .. }
            | Expr::ArrayFind {
                predicate: index, ..
            }
            | Expr::ArrayAny {
                predicate: index, ..
            }
            | Expr::ArrayAll {
                predicate: index, ..
            }
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
            | Expr::ArrayFilterValue {
                value,
                predicate: mapper,
            }
            | Expr::ArrayFlatMapValue { value, mapper }
            | Expr::ArrayFindValue {
                value,
                predicate: mapper,
            }
            | Expr::ArrayAnyValue {
                value,
                predicate: mapper,
            }
            | Expr::ArrayAllValue {
                value,
                predicate: mapper,
            }
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
            Expr::ArrayFold {
                initial, reducer, ..
            } => {
                self.extract_nested_try_results(initial, false, prefix);
                self.extract_nested_try_results(reducer, false, prefix);
            }
            Expr::ArrayFoldValue {
                value,
                initial,
                reducer,
            } => {
                self.extract_nested_try_results(value, false, prefix);
                self.extract_nested_try_results(initial, false, prefix);
                self.extract_nested_try_results(reducer, false, prefix);
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

    pub(super) fn lift_lazy_try_branch(&mut self, expr: &mut Expr) -> Option<Expr> {
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
}
