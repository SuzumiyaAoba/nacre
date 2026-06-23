use std::collections::HashSet;

use crate::{Expr, MatchArm, Program, Statement};

pub(crate) fn lower_method_calls(program: &Program) -> Program {
    let mut functions = HashSet::new();
    collect_function_names(program, &mut functions);
    lower_program(program, &functions)
}

fn collect_function_names(program: &Program, functions: &mut HashSet<String>) {
    for statement in program.statements() {
        match statement {
            Statement::Export(inner) => collect_function_names(
                &Program::new(vec![inner.as_ref().clone()], vec![1]),
                functions,
            ),
            Statement::Function { name, body, .. } => {
                functions.insert(name.clone());
                collect_function_names(body, functions);
            }
            Statement::ExternalFunction { name, .. } => {
                functions.insert(name.clone());
            }
            Statement::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_function_names(then_branch, functions);
                if let Some(else_branch) = else_branch {
                    collect_function_names(else_branch, functions);
                }
            }
            Statement::Impl { methods, .. } => {
                for method in methods {
                    functions.insert(method.name.clone());
                    collect_function_names(&method.body, functions);
                }
            }
            Statement::InherentImpl { methods, .. } => {
                for method in methods {
                    functions.insert(method.name.clone());
                    collect_function_names(&method.body, functions);
                }
            }
            Statement::Block { body } => collect_function_names(body, functions),
            Statement::Defer(statement) => collect_function_names(
                &Program::new(vec![statement.as_ref().clone()], vec![1]),
                functions,
            ),
            Statement::While { body, .. } | Statement::For { body, .. } => {
                collect_function_names(body, functions);
            }
            Statement::Use { .. }
            | Statement::Trait { .. }
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
            | Statement::Return(_)
            | Statement::Raw(_) => {}
        }
    }
}

fn lower_program(program: &Program, functions: &HashSet<String>) -> Program {
    Program::new(
        program
            .statements()
            .iter()
            .map(|statement| lower_statement(statement, functions))
            .collect(),
        program.statement_lines().to_vec(),
    )
}

fn lower_statement(statement: &Statement, functions: &HashSet<String>) -> Statement {
    match statement {
        Statement::Export(inner) => lower_statement(inner, functions),
        Statement::Function {
            name,
            override_constructor,
            type_params,
            params,
            return_type,
            body,
        } => Statement::Function {
            name: name.clone(),
            override_constructor: *override_constructor,
            type_params: type_params.clone(),
            params: params.clone(),
            return_type: return_type.clone(),
            body: lower_program(body, functions),
        },
        Statement::ExternalFunction {
            name,
            type_params,
            params,
            return_type,
        } => Statement::ExternalFunction {
            name: name.clone(),
            type_params: type_params.clone(),
            params: params.clone(),
            return_type: return_type.clone(),
        },
        Statement::Impl {
            trait_name,
            for_type,
            methods,
        } => Statement::Impl {
            trait_name: trait_name.clone(),
            for_type: for_type.clone(),
            methods: methods
                .iter()
                .map(|method| crate::ImplMethod {
                    name: method.name.clone(),
                    params: method.params.clone(),
                    return_type: method.return_type.clone(),
                    body: lower_program(&method.body, functions),
                })
                .collect(),
        },
        Statement::InherentImpl {
            for_type,
            consts,
            methods,
        } => Statement::InherentImpl {
            for_type: for_type.clone(),
            consts: consts
                .iter()
                .map(|value| crate::ImplConst {
                    name: value.name.clone(),
                    annotation: value.annotation.clone(),
                    expr: lower_expr(&value.expr, functions),
                })
                .collect(),
            methods: methods
                .iter()
                .map(|method| crate::ImplMethod {
                    name: method.name.clone(),
                    params: method.params.clone(),
                    return_type: method.return_type.clone(),
                    body: lower_program(&method.body, functions),
                })
                .collect(),
        },
        Statement::Const {
            name,
            annotation,
            expr,
        } => Statement::Const {
            name: name.clone(),
            annotation: annotation.clone(),
            expr: lower_expr(expr, functions),
        },
        Statement::Let {
            name,
            annotation,
            expr,
        } => Statement::Let {
            name: name.clone(),
            annotation: annotation.clone(),
            expr: lower_expr(expr, functions),
        },
        Statement::Destructure {
            mutable,
            pattern,
            expr,
        } => Statement::Destructure {
            mutable: *mutable,
            pattern: pattern.clone(),
            expr: lower_expr(expr, functions),
        },
        Statement::Assign { target, expr } => Statement::Assign {
            target: target.clone(),
            expr: lower_expr(expr, functions),
        },
        Statement::Expr(expr) => Statement::Expr(lower_expr(expr, functions)),
        Statement::If {
            condition,
            then_branch,
            else_branch,
        } => Statement::If {
            condition: lower_expr(condition, functions),
            then_branch: lower_program(then_branch, functions),
            else_branch: else_branch
                .as_ref()
                .map(|branch| lower_program(branch, functions)),
        },
        Statement::Block { body } => Statement::Block {
            body: lower_program(body, functions),
        },
        Statement::Defer(statement) => {
            Statement::Defer(Box::new(lower_statement(statement, functions)))
        }
        Statement::While { condition, body } => Statement::While {
            condition: lower_expr(condition, functions),
            body: lower_program(body, functions),
        },
        Statement::For {
            binding,
            iterable,
            body,
        } => Statement::For {
            binding: binding.clone(),
            iterable: lower_expr(iterable, functions),
            body: lower_program(body, functions),
        },
        Statement::Return(expr) => Statement::Return(lower_expr(expr, functions)),
        Statement::TryResult(expr) => Statement::TryResult(lower_expr(expr, functions)),
        Statement::TryPipeline { input, commands } => Statement::TryPipeline {
            input: input
                .as_ref()
                .map(|input| Box::new(lower_expr(input, functions))),
            commands: commands.clone(),
        },
        Statement::TryPipelineResult { input, commands } => Statement::TryPipelineResult {
            input: input
                .as_ref()
                .map(|input| Box::new(lower_expr(input, functions))),
            commands: commands.clone(),
        },
        Statement::Use { .. }
        | Statement::Trait { .. }
        | Statement::TypeAlias { .. }
        | Statement::SumType { .. }
        | Statement::Newtype { .. }
        | Statement::TryCommand(_)
        | Statement::TryCommandResult(_)
        | Statement::Command(_)
        | Statement::Redirect { .. }
        | Statement::Require { .. }
        | Statement::RequireOneOf { .. }
        | Statement::Break
        | Statement::Continue
        | Statement::Raw(_) => statement.clone(),
    }
}

fn lower_expr(expr: &Expr, functions: &HashSet<String>) -> Expr {
    match expr {
        Expr::AllowedCommand {
            group,
            command,
            args,
            result,
            program,
            read_args,
            write_args,
        } => Expr::AllowedCommand {
            group: group.clone(),
            command: command.clone(),
            args: args.iter().map(|arg| lower_expr(arg, functions)).collect(),
            result: *result,
            program: program.clone(),
            read_args: read_args.clone(),
            write_args: write_args.clone(),
        },
        Expr::Call { name, args } => lower_call(name, args, functions),
        Expr::NamedArg { name, value } => Expr::NamedArg {
            name: name.clone(),
            value: Box::new(lower_expr(value, functions)),
        },
        Expr::Range {
            start,
            end,
            inclusive,
        } => Expr::Range {
            start: Box::new(lower_expr(start, functions)),
            end: Box::new(lower_expr(end, functions)),
            inclusive: *inclusive,
        },
        Expr::Array(values) => Expr::Array(
            values
                .iter()
                .map(|value| lower_expr(value, functions))
                .collect(),
        ),
        Expr::Map(entries) => Expr::Map(
            entries
                .iter()
                .map(|(key, value)| (lower_expr(key, functions), lower_expr(value, functions)))
                .collect(),
        ),
        Expr::Record(fields) => Expr::Record(
            fields
                .iter()
                .map(|(name, value)| (name.clone(), lower_expr(value, functions)))
                .collect(),
        ),
        Expr::RecordPattern(fields) => Expr::RecordPattern(
            fields
                .iter()
                .map(|(name, value)| {
                    (
                        name.clone(),
                        value.as_ref().map(|value| lower_expr(value, functions)),
                    )
                })
                .collect(),
        ),
        Expr::ArrayPattern { patterns, rest } => Expr::ArrayPattern {
            patterns: patterns
                .iter()
                .map(|value| lower_expr(value, functions))
                .collect(),
            rest: rest.clone(),
        },
        Expr::AliasPattern { pattern, alias } => Expr::AliasPattern {
            pattern: Box::new(lower_expr(pattern, functions)),
            alias: alias.clone(),
        },
        Expr::Tuple(values) => Expr::Tuple(
            values
                .iter()
                .map(|value| lower_expr(value, functions))
                .collect(),
        ),
        Expr::Index { name, index } => Expr::Index {
            name: name.clone(),
            index: Box::new(lower_expr(index, functions)),
        },
        Expr::IndexValue { value, index } => Expr::IndexValue {
            value: Box::new(lower_expr(value, functions)),
            index: Box::new(lower_expr(index, functions)),
        },
        Expr::TupleFieldValue { value, field } => Expr::TupleFieldValue {
            value: Box::new(lower_expr(value, functions)),
            field: *field,
        },
        Expr::FieldValue { value, field } => Expr::FieldValue {
            value: Box::new(lower_expr(value, functions)),
            field: field.clone(),
        },
        Expr::Slice { name, start, end } => Expr::Slice {
            name: name.clone(),
            start: Box::new(lower_expr(start, functions)),
            end: Box::new(lower_expr(end, functions)),
        },
        Expr::ArraySliceValue { value, start, end } => Expr::ArraySliceValue {
            value: Box::new(lower_expr(value, functions)),
            start: Box::new(lower_expr(start, functions)),
            end: Box::new(lower_expr(end, functions)),
        },
        Expr::PathExists(path) => Expr::PathExists(Box::new(lower_expr(path, functions))),
        Expr::ProcessEnv { name } => Expr::ProcessEnv {
            name: Box::new(lower_expr(name, functions)),
        },
        Expr::FsIsFile { path } => Expr::FsIsFile {
            path: Box::new(lower_expr(path, functions)),
        },
        Expr::FsIsDir { path } => Expr::FsIsDir {
            path: Box::new(lower_expr(path, functions)),
        },
        Expr::FsSize { path } => Expr::FsSize {
            path: Box::new(lower_expr(path, functions)),
        },
        Expr::FsReadLines { path } => Expr::FsReadLines {
            path: Box::new(lower_expr(path, functions)),
        },
        Expr::FsList { path } => Expr::FsList {
            path: Box::new(lower_expr(path, functions)),
        },
        Expr::FsWriteLines { path, lines } => Expr::FsWriteLines {
            path: Box::new(lower_expr(path, functions)),
            lines: Box::new(lower_expr(lines, functions)),
        },
        Expr::FsAppendLines { path, lines } => Expr::FsAppendLines {
            path: Box::new(lower_expr(path, functions)),
            lines: Box::new(lower_expr(lines, functions)),
        },
        Expr::JsonParse { value } => Expr::JsonParse {
            value: Box::new(lower_expr(value, functions)),
        },
        Expr::JsonStringify { name } => Expr::JsonStringify { name: name.clone() },
        Expr::JsonStringifyValue { value } => Expr::JsonStringifyValue {
            value: Box::new(lower_expr(value, functions)),
        },
        Expr::NewtypeCtor { name, value } => Expr::NewtypeCtor {
            name: name.clone(),
            value: Box::new(lower_expr(value, functions)),
        },
        Expr::Variant {
            name,
            args,
            field_types,
        } => Expr::Variant {
            name: name.clone(),
            args: args.iter().map(|arg| lower_expr(arg, functions)).collect(),
            field_types: field_types.clone(),
        },
        Expr::Cast { expr, ty } => Expr::Cast {
            expr: Box::new(lower_expr(expr, functions)),
            ty: ty.clone(),
        },
        Expr::Lambda { params, body } => Expr::Lambda {
            params: params.clone(),
            body: Box::new(lower_expr(body, functions)),
        },
        Expr::Closure { name, captures } => Expr::Closure {
            name: name.clone(),
            captures: captures.clone(),
        },
        Expr::Do { steps, result } => Expr::Do {
            steps: steps
                .iter()
                .map(|step| match step {
                    crate::DoStep::Bind { name, expr } => crate::DoStep::Bind {
                        name: name.clone(),
                        expr: lower_expr(expr, functions),
                    },
                    crate::DoStep::Let {
                        name,
                        annotation,
                        expr,
                    } => crate::DoStep::Let {
                        name: name.clone(),
                        annotation: annotation.clone(),
                        expr: lower_expr(expr, functions),
                    },
                })
                .collect(),
            result: Box::new(lower_expr(result, functions)),
        },
        Expr::LetIn {
            name,
            annotation,
            value,
            body,
        } => Expr::LetIn {
            name: name.clone(),
            annotation: annotation.clone(),
            value: Box::new(lower_expr(value, functions)),
            body: Box::new(lower_expr(body, functions)),
        },
        Expr::IfElse {
            condition,
            then_expr,
            else_expr,
        } => Expr::IfElse {
            condition: Box::new(lower_expr(condition, functions)),
            then_expr: Box::new(lower_expr(then_expr, functions)),
            else_expr: Box::new(lower_expr(else_expr, functions)),
        },
        Expr::Match { value, arms } => Expr::Match {
            value: Box::new(lower_expr(value, functions)),
            arms: arms
                .iter()
                .map(|arm| MatchArm {
                    pattern: arm
                        .pattern
                        .as_ref()
                        .map(|pattern| lower_expr(pattern, functions)),
                    guard: arm.guard.as_ref().map(|guard| lower_expr(guard, functions)),
                    expr: lower_expr(&arm.expr, functions),
                })
                .collect(),
        },
        Expr::MatchGuardResult(value) => {
            Expr::MatchGuardResult(Box::new(lower_expr(value, functions)))
        }
        Expr::Some(value) => Expr::Some(Box::new(lower_expr(value, functions))),
        Expr::Async(value) => Expr::Async(Box::new(lower_expr(value, functions))),
        Expr::Ok(value) => Expr::Ok(Box::new(lower_expr(value, functions))),
        Expr::Err(value) => Expr::Err(Box::new(lower_expr(value, functions))),
        Expr::ResultOption(value) => Expr::ResultOption(Box::new(lower_expr(value, functions))),
        Expr::TryResult(value) => Expr::TryResult(Box::new(lower_expr(value, functions))),
        Expr::Default { value, fallback } => Expr::Default {
            value: Box::new(lower_expr(value, functions)),
            fallback: Box::new(lower_expr(fallback, functions)),
        },
        Expr::DefaultTry { value, fallback } => Expr::DefaultTry {
            value: Box::new(lower_expr(value, functions)),
            fallback: Box::new(lower_expr(fallback, functions)),
        },
        Expr::Binary { left, op, right } => Expr::Binary {
            left: Box::new(lower_expr(left, functions)),
            op: *op,
            right: Box::new(lower_expr(right, functions)),
        },
        Expr::Not(expr) => Expr::Not(Box::new(lower_expr(expr, functions))),
        Expr::BitNot(expr) => Expr::BitNot(Box::new(lower_expr(expr, functions))),
        Expr::Len(name) => Expr::Len(name.clone()),
        Expr::ArrayLenValue(value) => Expr::ArrayLenValue(Box::new(lower_expr(value, functions))),
        Expr::MapLenValue(value) => Expr::MapLenValue(Box::new(lower_expr(value, functions))),
        Expr::IsEmpty(name) => Expr::IsEmpty(name.clone()),
        Expr::ArrayIsEmptyValue(value) => {
            Expr::ArrayIsEmptyValue(Box::new(lower_expr(value, functions)))
        }
        Expr::MapIsEmptyValue(value) => {
            Expr::MapIsEmptyValue(Box::new(lower_expr(value, functions)))
        }
        Expr::ArrayFirst(name) => Expr::ArrayFirst(name.clone()),
        Expr::ArrayFirstValue(value) => {
            Expr::ArrayFirstValue(Box::new(lower_expr(value, functions)))
        }
        Expr::ArrayLast(name) => Expr::ArrayLast(name.clone()),
        Expr::ArrayLastValue(value) => Expr::ArrayLastValue(Box::new(lower_expr(value, functions))),
        Expr::ArrayReverse(name) => Expr::ArrayReverse(name.clone()),
        Expr::ArrayReverseValue(value) => {
            Expr::ArrayReverseValue(Box::new(lower_expr(value, functions)))
        }
        Expr::ArraySort(name) => Expr::ArraySort(name.clone()),
        Expr::ArraySortValue(value) => Expr::ArraySortValue(Box::new(lower_expr(value, functions))),
        Expr::ArrayUnique(name) => Expr::ArrayUnique(name.clone()),
        Expr::ArrayUniqueValue(value) => {
            Expr::ArrayUniqueValue(Box::new(lower_expr(value, functions)))
        }
        Expr::ArrayMap { name, mapper } => Expr::ArrayMap {
            name: name.clone(),
            mapper: Box::new(lower_expr(mapper, functions)),
        },
        Expr::ArrayMapValue { value, mapper } => Expr::ArrayMapValue {
            value: Box::new(lower_expr(value, functions)),
            mapper: Box::new(lower_expr(mapper, functions)),
        },
        Expr::OptionMap { name, mapper } => Expr::OptionMap {
            name: name.clone(),
            mapper: Box::new(lower_expr(mapper, functions)),
        },
        Expr::OptionMapValue { value, mapper } => Expr::OptionMapValue {
            value: Box::new(lower_expr(value, functions)),
            mapper: Box::new(lower_expr(mapper, functions)),
        },
        Expr::OptionFlatMap { name, mapper } => Expr::OptionFlatMap {
            name: name.clone(),
            mapper: Box::new(lower_expr(mapper, functions)),
        },
        Expr::OptionFlatMapValue { value, mapper } => Expr::OptionFlatMapValue {
            value: Box::new(lower_expr(value, functions)),
            mapper: Box::new(lower_expr(mapper, functions)),
        },
        Expr::ResultMap { name, mapper } => Expr::ResultMap {
            name: name.clone(),
            mapper: Box::new(lower_expr(mapper, functions)),
        },
        Expr::ResultMapValue { value, mapper } => Expr::ResultMapValue {
            value: Box::new(lower_expr(value, functions)),
            mapper: Box::new(lower_expr(mapper, functions)),
        },
        Expr::ResultFlatMap { name, mapper } => Expr::ResultFlatMap {
            name: name.clone(),
            mapper: Box::new(lower_expr(mapper, functions)),
        },
        Expr::ResultFlatMapValue { value, mapper } => Expr::ResultFlatMapValue {
            value: Box::new(lower_expr(value, functions)),
            mapper: Box::new(lower_expr(mapper, functions)),
        },
        Expr::OptionAp { name, value } => Expr::OptionAp {
            name: name.clone(),
            value: Box::new(lower_expr(value, functions)),
        },
        Expr::OptionApValue { function, value } => Expr::OptionApValue {
            function: Box::new(lower_expr(function, functions)),
            value: Box::new(lower_expr(value, functions)),
        },
        Expr::ResultAp { name, value } => Expr::ResultAp {
            name: name.clone(),
            value: Box::new(lower_expr(value, functions)),
        },
        Expr::ResultApValue { function, value } => Expr::ResultApValue {
            function: Box::new(lower_expr(function, functions)),
            value: Box::new(lower_expr(value, functions)),
        },
        Expr::OptionOrElse { name, fallback } => Expr::OptionOrElse {
            name: name.clone(),
            fallback: Box::new(lower_expr(fallback, functions)),
        },
        Expr::OptionOrElseValue { value, fallback } => Expr::OptionOrElseValue {
            value: Box::new(lower_expr(value, functions)),
            fallback: Box::new(lower_expr(fallback, functions)),
        },
        Expr::OptionOrElseTry { value, fallback } => Expr::OptionOrElseTry {
            value: Box::new(lower_expr(value, functions)),
            fallback: Box::new(lower_expr(fallback, functions)),
        },
        Expr::ArrayTake { name, count } => Expr::ArrayTake {
            name: name.clone(),
            count: Box::new(lower_expr(count, functions)),
        },
        Expr::ArrayTakeValue { value, count } => Expr::ArrayTakeValue {
            value: Box::new(lower_expr(value, functions)),
            count: Box::new(lower_expr(count, functions)),
        },
        Expr::ArrayDrop { name, count } => Expr::ArrayDrop {
            name: name.clone(),
            count: Box::new(lower_expr(count, functions)),
        },
        Expr::ArrayDropValue { value, count } => Expr::ArrayDropValue {
            value: Box::new(lower_expr(value, functions)),
            count: Box::new(lower_expr(count, functions)),
        },
        Expr::Join { name, separator } => Expr::Join {
            name: name.clone(),
            separator: Box::new(lower_expr(separator, functions)),
        },
        Expr::JoinValue { value, separator } => Expr::JoinValue {
            value: Box::new(lower_expr(value, functions)),
            separator: Box::new(lower_expr(separator, functions)),
        },
        Expr::ArrayPush { name, value } => Expr::ArrayPush {
            name: name.clone(),
            value: Box::new(lower_expr(value, functions)),
        },
        Expr::ArrayPop { name } => Expr::ArrayPop { name: name.clone() },
        Expr::MapSet { name, key, value } => Expr::MapSet {
            name: name.clone(),
            key: Box::new(lower_expr(key, functions)),
            value: Box::new(lower_expr(value, functions)),
        },
        Expr::MapRemove { name, key } => Expr::MapRemove {
            name: name.clone(),
            key: Box::new(lower_expr(key, functions)),
        },
        Expr::ArrayContains { name, value } => Expr::ArrayContains {
            name: name.clone(),
            value: Box::new(lower_expr(value, functions)),
        },
        Expr::ArrayContainsValue { value, item } => Expr::ArrayContainsValue {
            value: Box::new(lower_expr(value, functions)),
            item: Box::new(lower_expr(item, functions)),
        },
        Expr::ArrayIndexOf { name, value } => Expr::ArrayIndexOf {
            name: name.clone(),
            value: Box::new(lower_expr(value, functions)),
        },
        Expr::ArrayIndexOfValue { value, item } => Expr::ArrayIndexOfValue {
            value: Box::new(lower_expr(value, functions)),
            item: Box::new(lower_expr(item, functions)),
        },
        Expr::MapKeys(name) => Expr::MapKeys(name.clone()),
        Expr::MapKeysValue(value) => Expr::MapKeysValue(Box::new(lower_expr(value, functions))),
        Expr::MapValues(name) => Expr::MapValues(name.clone()),
        Expr::MapValuesValue(value) => Expr::MapValuesValue(Box::new(lower_expr(value, functions))),
        Expr::MapHas { name, key } => Expr::MapHas {
            name: name.clone(),
            key: Box::new(lower_expr(key, functions)),
        },
        Expr::MapHasValue { value, key } => Expr::MapHasValue {
            value: Box::new(lower_expr(value, functions)),
            key: Box::new(lower_expr(key, functions)),
        },
        Expr::StringContains { name, needle } => Expr::StringContains {
            name: name.clone(),
            needle: Box::new(lower_expr(needle, functions)),
        },
        Expr::StringContainsValue { value, needle } => Expr::StringContainsValue {
            value: Box::new(lower_expr(value, functions)),
            needle: Box::new(lower_expr(needle, functions)),
        },
        Expr::StringIndexOf { name, needle } => Expr::StringIndexOf {
            name: name.clone(),
            needle: Box::new(lower_expr(needle, functions)),
        },
        Expr::StringIndexOfValue { value, needle } => Expr::StringIndexOfValue {
            value: Box::new(lower_expr(value, functions)),
            needle: Box::new(lower_expr(needle, functions)),
        },
        Expr::StringStartsWith { name, prefix } => Expr::StringStartsWith {
            name: name.clone(),
            prefix: Box::new(lower_expr(prefix, functions)),
        },
        Expr::StringStartsWithValue { value, prefix } => Expr::StringStartsWithValue {
            value: Box::new(lower_expr(value, functions)),
            prefix: Box::new(lower_expr(prefix, functions)),
        },
        Expr::StringEndsWith { name, suffix } => Expr::StringEndsWith {
            name: name.clone(),
            suffix: Box::new(lower_expr(suffix, functions)),
        },
        Expr::StringEndsWithValue { value, suffix } => Expr::StringEndsWithValue {
            value: Box::new(lower_expr(value, functions)),
            suffix: Box::new(lower_expr(suffix, functions)),
        },
        Expr::StringLen(name) => Expr::StringLen(name.clone()),
        Expr::StringLenValue(value) => Expr::StringLenValue(Box::new(lower_expr(value, functions))),
        Expr::StringIsEmpty(name) => Expr::StringIsEmpty(name.clone()),
        Expr::StringIsEmptyValue(value) => {
            Expr::StringIsEmptyValue(Box::new(lower_expr(value, functions)))
        }
        Expr::StringSlice { name, start, end } => Expr::StringSlice {
            name: name.clone(),
            start: Box::new(lower_expr(start, functions)),
            end: Box::new(lower_expr(end, functions)),
        },
        Expr::StringSliceValue { value, start, end } => Expr::StringSliceValue {
            value: Box::new(lower_expr(value, functions)),
            start: Box::new(lower_expr(start, functions)),
            end: Box::new(lower_expr(end, functions)),
        },
        Expr::StringTrim(name) => Expr::StringTrim(name.clone()),
        Expr::StringTrimValue(value) => {
            Expr::StringTrimValue(Box::new(lower_expr(value, functions)))
        }
        Expr::StringTrimStart(name) => Expr::StringTrimStart(name.clone()),
        Expr::StringTrimStartValue(value) => {
            Expr::StringTrimStartValue(Box::new(lower_expr(value, functions)))
        }
        Expr::StringTrimEnd(name) => Expr::StringTrimEnd(name.clone()),
        Expr::StringTrimEndValue(value) => {
            Expr::StringTrimEndValue(Box::new(lower_expr(value, functions)))
        }
        Expr::StringToUpper(name) => Expr::StringToUpper(name.clone()),
        Expr::StringToUpperValue(value) => {
            Expr::StringToUpperValue(Box::new(lower_expr(value, functions)))
        }
        Expr::StringToLower(name) => Expr::StringToLower(name.clone()),
        Expr::StringToLowerValue(value) => {
            Expr::StringToLowerValue(Box::new(lower_expr(value, functions)))
        }
        Expr::StringRepeat { name, count } => Expr::StringRepeat {
            name: name.clone(),
            count: Box::new(lower_expr(count, functions)),
        },
        Expr::StringRepeatValue { value, count } => Expr::StringRepeatValue {
            value: Box::new(lower_expr(value, functions)),
            count: Box::new(lower_expr(count, functions)),
        },
        Expr::StringSplit { name, separator } => Expr::StringSplit {
            name: name.clone(),
            separator: Box::new(lower_expr(separator, functions)),
        },
        Expr::StringSplitValue { value, separator } => Expr::StringSplitValue {
            value: Box::new(lower_expr(value, functions)),
            separator: Box::new(lower_expr(separator, functions)),
        },
        Expr::StringReplace { name, from, to } => Expr::StringReplace {
            name: name.clone(),
            from: Box::new(lower_expr(from, functions)),
            to: Box::new(lower_expr(to, functions)),
        },
        Expr::StringReplaceValue { value, from, to } => Expr::StringReplaceValue {
            value: Box::new(lower_expr(value, functions)),
            from: Box::new(lower_expr(from, functions)),
            to: Box::new(lower_expr(to, functions)),
        },
        Expr::PathBasename(name) => Expr::PathBasename(name.clone()),
        Expr::PathBasenameValue(value) => {
            Expr::PathBasenameValue(Box::new(lower_expr(value, functions)))
        }
        Expr::PathDirname(name) => Expr::PathDirname(name.clone()),
        Expr::PathDirnameValue(value) => {
            Expr::PathDirnameValue(Box::new(lower_expr(value, functions)))
        }
        Expr::PathStem(name) => Expr::PathStem(name.clone()),
        Expr::PathStemValue(value) => Expr::PathStemValue(Box::new(lower_expr(value, functions))),
        Expr::PathExtname(name) => Expr::PathExtname(name.clone()),
        Expr::PathExtnameValue(value) => {
            Expr::PathExtnameValue(Box::new(lower_expr(value, functions)))
        }
        Expr::PathIsAbsolute(name) => Expr::PathIsAbsolute(name.clone()),
        Expr::PathIsAbsoluteValue(value) => {
            Expr::PathIsAbsoluteValue(Box::new(lower_expr(value, functions)))
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
        | Expr::Field { .. }
        | Expr::Value(_)
        | Expr::EnvDefault { .. }
        | Expr::Env(_)
        | Expr::ProcessArgs
        | Expr::CliParse
        | Expr::Ident(_) => expr.clone(),
    }
}

fn lower_call(name: &str, args: &[Expr], functions: &HashSet<String>) -> Expr {
    let lowered_args = args
        .iter()
        .map(|arg| lower_expr(arg, functions))
        .collect::<Vec<_>>();

    if functions.contains(name) {
        return Expr::Call {
            name: name.to_string(),
            args: lowered_args,
        };
    }

    let Some((receiver, method)) = name.rsplit_once('.') else {
        return Expr::Call {
            name: name.to_string(),
            args: lowered_args,
        };
    };
    if !is_valid_name(receiver) || !is_valid_name(method) {
        return Expr::Call {
            name: name.to_string(),
            args: lowered_args,
        };
    }

    let mut args = Vec::with_capacity(lowered_args.len() + 1);
    args.push(Expr::Ident(receiver.to_string()));
    args.extend(lowered_args);
    Expr::Call {
        name: method.to_string(),
        args,
    }
}

fn is_valid_name(input: &str) -> bool {
    let mut chars = input.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn lowers_method_calls_to_ufcs_calls() {
        let program = parse(
            r#"
fn shout(value: String): String {
return "${value}!"
}
const name = "Nacre"
const loud = name.shout()
"#,
        )
        .unwrap();

        let lowered = lower_method_calls(&program);

        assert!(matches!(
            &lowered.statements()[2],
            Statement::Const {
                expr: Expr::Call { name, args },
                ..
            } if name == "shout" && args == &vec![Expr::Ident("name".into())]
        ));
    }

    #[test]
    fn preserves_existing_qualified_function_calls() {
        let program = Program::new(
            vec![
                Statement::Function {
                    name: "fs.exists".into(),
                    override_constructor: false,
                    type_params: Vec::new(),
                    params: Vec::new(),
                    return_type: crate::Type::Bool,
                    body: Program::new(Vec::new(), Vec::new()),
                },
                Statement::Const {
                    name: "ok".into(),
                    annotation: None,
                    expr: Expr::Call {
                        name: "fs.exists".into(),
                        args: vec![Expr::String("/tmp".into())],
                    },
                },
            ],
            vec![1, 2],
        );

        let lowered = lower_method_calls(&program);

        assert!(matches!(
            &lowered.statements()[1],
            Statement::Const {
                expr: Expr::Call { name, args },
                ..
            } if name == "fs.exists" && args == &vec![Expr::String("/tmp".into())]
        ));
    }

    #[test]
    fn leaves_invalid_method_like_names_unchanged() {
        let program = Program::new(
            vec![
                Statement::Const {
                    name: "bad_receiver".into(),
                    annotation: None,
                    expr: Expr::Call {
                        name: "bad-receiver.method".into(),
                        args: Vec::new(),
                    },
                },
                Statement::Const {
                    name: "bad_method".into(),
                    annotation: None,
                    expr: Expr::Call {
                        name: "receiver.bad-method".into(),
                        args: Vec::new(),
                    },
                },
                Statement::Const {
                    name: "empty".into(),
                    annotation: None,
                    expr: Expr::Call {
                        name: ".method".into(),
                        args: Vec::new(),
                    },
                },
                Statement::Expr(Expr::Int(1)),
            ],
            vec![1, 2, 3, 4],
        );

        let lowered = lower_method_calls(&program);
        let names = lowered
            .statements()
            .iter()
            .map(|statement| match statement {
                Statement::Const {
                    expr: Expr::Call { name, .. },
                    ..
                } => name.as_str(),
                _ => "",
            })
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec!["bad-receiver.method", "receiver.bad-method", ".method", ""]
        );
        assert!(!is_valid_name(""));
    }
}
