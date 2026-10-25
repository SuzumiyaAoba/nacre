use super::names::argument_count;
use super::type_utils::command_output_type;
use super::validation::{interpolation_names, unsafe_execution_error};
use super::TypeChecker;
use crate::symbols::is_identifier;
use crate::{CompileError, DoStep, Expr, MatchArm, Program, Statement, Type};

impl TypeChecker {
    pub(super) fn check_await(&self, name: &str, line: usize) -> Result<Type, CompileError> {
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

    pub(super) fn check_async(&self, value: &Expr, line: usize) -> Result<Type, CompileError> {
        match value {
            Expr::AllowedCommand { .. } | Expr::Call { .. } => {
                let value_ty = self.check_expr(value, line)?;
                Ok(Type::Future(Box::new(value_ty)))
            }
            Expr::Command { .. }
            | Expr::CommandResult { .. }
            | Expr::AsyncCommand(_)
            | Expr::Pipeline { .. }
            | Expr::TryPipeline { .. }
            | Expr::PipelineResult { .. } => Err(unsafe_execution_error(line)),
            _ => Err(CompileError::new(
                line,
                "async only supports policy-approved command calls and pure function calls"
                    .to_string(),
            )),
        }
    }

    pub(super) fn check_allowed_command(
        &self,
        group: &str,
        command: &str,
        args: &[Expr],
        result: bool,
        line: usize,
    ) -> Result<Type, CompileError> {
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
        if self.enforce_capabilities {
            let Some(policy) = self.policy.command(group, command) else {
                return Err(CompileError::new(
                    line,
                    format!("command `{group}.{command}` is not allowed by the execution policy"),
                ));
            };
            if args.len() != policy.args() {
                return Err(CompileError::new(
                    line,
                    format!(
                        "command `{group}.{command}` expects {} by policy, found {}",
                        argument_count(policy.args()),
                        args.len()
                    ),
                ));
            }
            if !policy.read_args().is_empty() {
                self.require_read_access(&format!("command `{group}.{command}`"), line)?;
            }
            if !policy.write_args().is_empty() {
                self.require_write_access(&format!("command `{group}.{command}`"), line)?;
            }
        }
        if result {
            Ok(command_output_type())
        } else {
            Ok(Type::String)
        }
    }

    pub(super) fn require_read_access(
        &self,
        operation: &str,
        line: usize,
    ) -> Result<(), CompileError> {
        if !self.enforce_capabilities {
            return Ok(());
        }
        if self.policy.has_read_access() {
            Ok(())
        } else {
            Err(CompileError::new(
                line,
                format!("{operation} requires at least one allowed filesystem read root"),
            ))
        }
    }

    pub(super) fn require_write_access(
        &self,
        operation: &str,
        line: usize,
    ) -> Result<(), CompileError> {
        if !self.enforce_capabilities {
            return Ok(());
        }
        if self.policy.has_write_access() {
            Ok(())
        } else {
            Err(CompileError::new(
                line,
                format!("{operation} requires at least one allowed filesystem write root"),
            ))
        }
    }

    pub(super) fn require_env_access(&self, name: &str, line: usize) -> Result<(), CompileError> {
        if !is_identifier(name) {
            return Err(CompileError::new(
                line,
                format!("invalid environment variable name `{name}`"),
            ));
        }
        if !self.enforce_capabilities {
            return Ok(());
        }
        if self.policy.can_read_env(name) {
            Ok(())
        } else {
            Err(CompileError::new(
                line,
                format!("environment variable `{name}` is not allowed by the execution policy"),
            ))
        }
    }

    pub(super) fn require_process_args_access_in_statement(
        &self,
        statement: &Statement,
        line: usize,
    ) -> Result<(), CompileError> {
        if !self.enforce_capabilities {
            return Ok(());
        }
        if statement_uses_process_args(statement) && !self.policy.can_read_process_args() {
            Err(CompileError::new(
                line,
                "process arguments are not allowed by the execution policy".to_string(),
            ))
        } else {
            Ok(())
        }
    }

    pub(super) fn check_path_exists(&self, path: &Expr, line: usize) -> Result<Type, CompileError> {
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

    pub(super) fn check_fs_path(&self, path: &Expr, line: usize) -> Result<(), CompileError> {
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

    pub(super) fn check_fs_write_lines(
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

    pub(super) fn check_process_env(&self, name: &Expr, line: usize) -> Result<Type, CompileError> {
        let Some(name) = static_env_name(name) else {
            let name_ty = self.check_expr(name, line)?;
            return if matches!(name_ty, Type::String | Type::Path) {
                Err(CompileError::new(
                    line,
                    "process.env requires a static string literal environment variable name"
                        .to_string(),
                ))
            } else {
                Err(CompileError::new(
                    line,
                    format!(
                        "process.env name must be String or Path, found {}",
                        name_ty.name()
                    ),
                ))
            };
        };
        self.require_env_access(name, line)?;
        Ok(Type::String)
    }
}

pub(super) fn static_env_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::String(value) | Expr::RawString(value) => Some(value),
        _ => None,
    }
}

pub(super) fn statement_uses_process_args(statement: &Statement) -> bool {
    match statement {
        Statement::Function { body, .. } => program_uses_process_args(body),
        Statement::Impl { methods, .. } => methods
            .iter()
            .any(|method| program_uses_process_args(&method.body)),
        Statement::Const { expr, .. }
        | Statement::Let { expr, .. }
        | Statement::Destructure { expr, .. }
        | Statement::Assign { expr, .. }
        | Statement::Expr(expr)
        | Statement::TryResult(expr)
        | Statement::Return(expr) => expr_uses_process_args(expr),
        Statement::Block { body } | Statement::While { body, .. } | Statement::For { body, .. } => {
            program_uses_process_args(body)
        }
        Statement::Defer(statement) => statement_uses_process_args(statement),
        Statement::If {
            condition,
            then_branch,
            else_branch,
        } => {
            expr_uses_process_args(condition)
                || program_uses_process_args(then_branch)
                || else_branch.as_ref().is_some_and(program_uses_process_args)
        }
        _ => false,
    }
}

pub(super) fn program_uses_process_args(program: &Program) -> bool {
    program.statements().iter().any(statement_uses_process_args)
}

pub(super) fn do_step_uses_process_args(step: &DoStep) -> bool {
    match step {
        DoStep::Bind { expr, .. } | DoStep::Let { expr, .. } => expr_uses_process_args(expr),
    }
}

pub(super) fn match_arm_uses_process_args(arm: &MatchArm) -> bool {
    arm.pattern.as_ref().is_some_and(expr_uses_process_args)
        || arm.guard.as_ref().is_some_and(expr_uses_process_args)
        || expr_uses_process_args(&arm.expr)
}

pub(super) fn expr_uses_process_args(expr: &Expr) -> bool {
    match expr {
        Expr::ProcessArgs | Expr::CliParse => true,
        Expr::Ident(name)
        | Expr::Value(name)
        | Expr::Len(name)
        | Expr::IsEmpty(name)
        | Expr::ArrayFirst(name)
        | Expr::ArrayLast(name)
        | Expr::ArrayReverse(name)
        | Expr::ArraySort(name)
        | Expr::ArrayUnique(name)
        | Expr::ArrayPop { name }
        | Expr::MapKeys(name)
        | Expr::MapValues(name)
        | Expr::StringLen(name)
        | Expr::StringIsEmpty(name)
        | Expr::StringTrim(name)
        | Expr::StringTrimStart(name)
        | Expr::StringTrimEnd(name)
        | Expr::StringToUpper(name)
        | Expr::StringToLower(name)
        | Expr::PathBasename(name)
        | Expr::PathDirname(name)
        | Expr::PathStem(name)
        | Expr::PathExtname(name)
        | Expr::PathIsAbsolute(name)
        | Expr::Field { name, .. } => name == "args",
        Expr::String(value) => interpolation_mentions_args(value),
        Expr::Some(value)
        | Expr::Ok(value)
        | Expr::Err(value)
        | Expr::Async(value)
        | Expr::ResultOption(value)
        | Expr::TryResult(value)
        | Expr::PathExists(value)
        | Expr::IndexValue { value, .. }
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
        | Expr::MatchGuardResult(value)
        | Expr::Not(value)
        | Expr::BitNot(value)
        | Expr::Cast { expr: value, .. }
        | Expr::NewtypeCtor { value, .. }
        | Expr::TupleFieldValue { value, .. }
        | Expr::FieldValue { value, .. } => expr_uses_process_args(value),
        Expr::Default { value, fallback }
        | Expr::DefaultTry { value, fallback }
        | Expr::OptionOrElseValue { value, fallback }
        | Expr::OptionOrElseTry { value, fallback }
        | Expr::ArrayContainsValue {
            value,
            item: fallback,
        }
        | Expr::ArrayIndexOfValue {
            value,
            item: fallback,
        }
        | Expr::MapHasValue {
            value,
            key: fallback,
        } => expr_uses_process_args(value) || expr_uses_process_args(fallback),
        Expr::Index { name, index }
        | Expr::Slice {
            name, start: index, ..
        }
        | Expr::ArrayTake { name, count: index }
        | Expr::ArrayDrop { name, count: index }
        | Expr::Join {
            name,
            separator: index,
        }
        | Expr::ArrayContains { name, value: index }
        | Expr::ArrayIndexOf { name, value: index }
        | Expr::MapHas { name, key: index }
        | Expr::StringContains {
            name,
            needle: index,
        }
        | Expr::StringIndexOf {
            name,
            needle: index,
        }
        | Expr::StringStartsWith {
            name,
            prefix: index,
        }
        | Expr::StringEndsWith {
            name,
            suffix: index,
        }
        | Expr::StringRepeat { name, count: index }
        | Expr::StringSplit {
            name,
            separator: index,
        } => name == "args" || expr_uses_process_args(index),
        Expr::ArraySliceValue { value, start, end }
        | Expr::StringSliceValue { value, start, end } => {
            expr_uses_process_args(value)
                || expr_uses_process_args(start)
                || expr_uses_process_args(end)
        }
        Expr::StringSlice { name, start, end } => {
            name == "args" || expr_uses_process_args(start) || expr_uses_process_args(end)
        }
        Expr::StringReplace { name, from, to } => {
            name == "args" || expr_uses_process_args(from) || expr_uses_process_args(to)
        }
        Expr::StringReplaceValue { value, from, to } => {
            expr_uses_process_args(value)
                || expr_uses_process_args(from)
                || expr_uses_process_args(to)
        }
        Expr::OptionMap { name, mapper }
        | Expr::OptionFlatMap { name, mapper }
        | Expr::ResultMap { name, mapper }
        | Expr::ResultFlatMap { name, mapper }
        | Expr::ArrayMap { name, mapper }
        | Expr::ArrayFlatMap { name, mapper } => name == "args" || expr_uses_process_args(mapper),
        Expr::ArrayFilter { name, predicate }
        | Expr::ArrayFind { name, predicate }
        | Expr::ArrayAny { name, predicate }
        | Expr::ArrayAll { name, predicate } => name == "args" || expr_uses_process_args(predicate),
        Expr::OptionMapValue { value, mapper }
        | Expr::OptionFlatMapValue { value, mapper }
        | Expr::ResultMapValue { value, mapper }
        | Expr::ResultFlatMapValue { value, mapper }
        | Expr::ArrayMapValue { value, mapper }
        | Expr::ArrayFlatMapValue { value, mapper } => {
            expr_uses_process_args(value) || expr_uses_process_args(mapper)
        }
        Expr::ArrayFilterValue { value, predicate }
        | Expr::ArrayFindValue { value, predicate }
        | Expr::ArrayAnyValue { value, predicate }
        | Expr::ArrayAllValue { value, predicate } => {
            expr_uses_process_args(value) || expr_uses_process_args(predicate)
        }
        Expr::ArrayFold {
            name,
            initial,
            reducer,
        } => name == "args" || expr_uses_process_args(initial) || expr_uses_process_args(reducer),
        Expr::ArrayFoldValue {
            value,
            initial,
            reducer,
        } => {
            expr_uses_process_args(value)
                || expr_uses_process_args(initial)
                || expr_uses_process_args(reducer)
        }
        Expr::OptionAp { name, value }
        | Expr::ResultAp { name, value }
        | Expr::OptionOrElse {
            name,
            fallback: value,
        }
        | Expr::ArrayPush { name, value } => name == "args" || expr_uses_process_args(value),
        Expr::OptionApValue { function, value } | Expr::ResultApValue { function, value } => {
            expr_uses_process_args(function) || expr_uses_process_args(value)
        }
        Expr::ArrayTakeValue { value, count }
        | Expr::ArrayDropValue { value, count }
        | Expr::StringRepeatValue { value, count } => {
            expr_uses_process_args(value) || expr_uses_process_args(count)
        }
        Expr::JoinValue { value, separator } | Expr::StringSplitValue { value, separator } => {
            expr_uses_process_args(value) || expr_uses_process_args(separator)
        }
        Expr::MapSet { name, key, value } => {
            name == "args" || expr_uses_process_args(key) || expr_uses_process_args(value)
        }
        Expr::MapRemove { name, key } => name == "args" || expr_uses_process_args(key),
        Expr::StringContainsValue { value, needle }
        | Expr::StringIndexOfValue { value, needle }
        | Expr::StringStartsWithValue {
            value,
            prefix: needle,
        }
        | Expr::StringEndsWithValue {
            value,
            suffix: needle,
        } => expr_uses_process_args(value) || expr_uses_process_args(needle),
        Expr::FsWriteLines { path, lines } | Expr::FsAppendLines { path, lines } => {
            expr_uses_process_args(path) || expr_uses_process_args(lines)
        }
        Expr::LetIn { value, body, .. } => {
            expr_uses_process_args(value) || expr_uses_process_args(body)
        }
        Expr::IfElse {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_uses_process_args(condition)
                || expr_uses_process_args(then_expr)
                || expr_uses_process_args(else_expr)
        }
        Expr::Binary { left, right, .. } => {
            expr_uses_process_args(left) || expr_uses_process_args(right)
        }
        Expr::Array(values)
        | Expr::Tuple(values)
        | Expr::Variant { args: values, .. }
        | Expr::Call { args: values, .. }
        | Expr::AllowedCommand { args: values, .. } => values.iter().any(expr_uses_process_args),
        Expr::Range { start, end, .. } => {
            expr_uses_process_args(start) || expr_uses_process_args(end)
        }
        Expr::NamedArg { value, .. } => expr_uses_process_args(value),
        Expr::Map(entries) => entries
            .iter()
            .any(|(key, value)| expr_uses_process_args(key) || expr_uses_process_args(value)),
        Expr::Record(fields) => fields
            .iter()
            .any(|(_, value)| expr_uses_process_args(value)),
        Expr::RecordPattern(fields) => fields
            .iter()
            .any(|(_, value)| value.as_ref().is_some_and(expr_uses_process_args)),
        Expr::ArrayPattern { patterns, .. } => patterns.iter().any(expr_uses_process_args),
        Expr::AliasPattern { pattern, .. } => expr_uses_process_args(pattern),
        Expr::Lambda { body, .. } => expr_uses_process_args(body),
        Expr::Do { steps, result } => {
            steps.iter().any(do_step_uses_process_args) || expr_uses_process_args(result)
        }
        Expr::Pipeline { input, .. }
        | Expr::TryPipeline { input, .. }
        | Expr::PipelineResult { input, .. } => input
            .as_ref()
            .is_some_and(|input| expr_uses_process_args(input)),
        Expr::Match { value, arms } => {
            expr_uses_process_args(value) || arms.iter().any(match_arm_uses_process_args)
        }
        Expr::Closure { captures, .. } => captures
            .iter()
            .any(|capture| capture.source == "args" || capture.target == "args"),
        Expr::RawString(_)
        | Expr::Int(_)
        | Expr::Float(_)
        | Expr::Bool(_)
        | Expr::Unit
        | Expr::None
        | Expr::Command { .. }
        | Expr::CommandResult { .. }
        | Expr::AsyncCommand(_)
        | Expr::Await(_)
        | Expr::HasCommand(_)
        | Expr::TupleField { .. }
        | Expr::EnvDefault { .. }
        | Expr::Env(_)
        | Expr::JsonStringify { .. } => false,
    }
}

pub(super) fn interpolation_mentions_args(value: &str) -> bool {
    interpolation_names(value, 0).is_ok_and(|names| names.iter().any(|name| name == "args"))
}
