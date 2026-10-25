use crate::{DoStep, Expr, Program, Statement};
use std::collections::{HashMap, HashSet};

pub(super) fn reachable_imported_functions(program: &Program) -> HashSet<String> {
    let imported = program
        .statements()
        .iter()
        .filter_map(|statement| match statement {
            Statement::Function { name, .. } if is_imported_function_name(name) => {
                Some(name.clone())
            }
            _ => None,
        })
        .collect::<HashSet<_>>();
    if imported.is_empty() {
        return HashSet::new();
    }

    let mut graph = HashMap::<String, HashSet<String>>::new();
    let mut seed = HashSet::new();
    for statement in program.statements() {
        match statement {
            Statement::Function { name, body, .. } if is_imported_function_name(name) => {
                let mut refs = HashSet::new();
                collect_program_function_refs(body, &imported, &mut refs);
                graph.insert(name.clone(), refs);
            }
            Statement::Function { body, .. } => {
                collect_program_function_refs(body, &imported, &mut seed);
            }
            Statement::Impl { methods, .. } => {
                for method in methods {
                    collect_program_function_refs(&method.body, &imported, &mut seed);
                }
            }
            statement => collect_statement_function_refs(statement, &imported, &mut seed),
        }
    }

    let mut reachable = HashSet::new();
    let mut pending = seed.into_iter().collect::<Vec<_>>();
    while let Some(name) = pending.pop() {
        if !reachable.insert(name.clone()) {
            continue;
        }
        if let Some(refs) = graph.get(&name) {
            pending.extend(refs.iter().cloned());
        }
    }
    reachable
}

pub(super) fn is_imported_function_name(name: &str) -> bool {
    name.contains('.')
}

pub(super) fn collect_program_function_refs(
    program: &Program,
    functions: &HashSet<String>,
    refs: &mut HashSet<String>,
) {
    for statement in program.statements() {
        collect_statement_function_refs(statement, functions, refs);
    }
}

pub(super) fn collect_statement_function_refs(
    statement: &Statement,
    functions: &HashSet<String>,
    refs: &mut HashSet<String>,
) {
    match statement {
        Statement::Export(inner) => collect_statement_function_refs(inner, functions, refs),
        Statement::Function { body, .. } => collect_program_function_refs(body, functions, refs),
        Statement::Impl { methods, .. } => {
            for method in methods {
                collect_program_function_refs(&method.body, functions, refs);
            }
        }
        Statement::InherentImpl {
            consts, methods, ..
        } => {
            for value in consts {
                collect_expr_function_refs(&value.expr, functions, refs);
            }
            for method in methods {
                collect_program_function_refs(&method.body, functions, refs);
            }
        }
        Statement::Const { expr, .. }
        | Statement::Let { expr, .. }
        | Statement::Assign { expr, .. }
        | Statement::Expr(expr)
        | Statement::TryResult(expr)
        | Statement::Return(expr) => collect_expr_function_refs(expr, functions, refs),
        Statement::Destructure { expr, .. } => collect_expr_function_refs(expr, functions, refs),
        Statement::Block { body } => collect_program_function_refs(body, functions, refs),
        Statement::Defer(statement) => collect_statement_function_refs(statement, functions, refs),
        Statement::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_expr_function_refs(condition, functions, refs);
            collect_program_function_refs(then_branch, functions, refs);
            if let Some(else_branch) = else_branch {
                collect_program_function_refs(else_branch, functions, refs);
            }
        }
        Statement::While { condition, body } => {
            collect_expr_function_refs(condition, functions, refs);
            collect_program_function_refs(body, functions, refs);
        }
        Statement::For { iterable, body, .. } => {
            collect_expr_function_refs(iterable, functions, refs);
            collect_program_function_refs(body, functions, refs);
        }
        Statement::Trait { .. }
        | Statement::TypeAlias { .. }
        | Statement::SumType { .. }
        | Statement::Newtype { .. }
        | Statement::ExternalFunction { .. }
        | Statement::Use { .. }
        | Statement::TryCommand(_)
        | Statement::TryCommandResult(_)
        | Statement::TryPipeline { .. }
        | Statement::TryPipelineResult { .. }
        | Statement::Command(_)
        | Statement::Redirect { .. }
        | Statement::Require { .. }
        | Statement::RequireOneOf { .. }
        | Statement::Break
        | Statement::Continue
        | Statement::Raw(_) => {}
    }
}

pub(super) fn collect_expr_function_refs(
    expr: &Expr,
    functions: &HashSet<String>,
    refs: &mut HashSet<String>,
) {
    match expr {
        Expr::Call { name, args } => {
            if functions.contains(name) {
                refs.insert(name.clone());
            }
            collect_exprs_function_refs(args, functions, refs);
        }
        Expr::NamedArg { value, .. } => collect_expr_function_refs(value, functions, refs),
        Expr::ArrayPattern { patterns, .. } => {
            collect_exprs_function_refs(patterns, functions, refs);
        }
        Expr::AliasPattern { pattern, .. } => {
            collect_expr_function_refs(pattern, functions, refs);
        }
        Expr::Ident(name) | Expr::Closure { name, .. } if functions.contains(name) => {
            refs.insert(name.clone());
        }
        Expr::Field { name, field } => {
            let qualified = format!("{name}.{field}");
            if functions.contains(&qualified) {
                refs.insert(qualified);
            }
        }
        Expr::Some(value)
        | Expr::Ok(value)
        | Expr::Err(value)
        | Expr::Async(value)
        | Expr::ResultOption(value)
        | Expr::TryResult(value)
        | Expr::PathExists(value)
        | Expr::IndexValue { value, .. }
        | Expr::ArraySliceValue { value, .. }
        | Expr::TupleFieldValue { value, .. }
        | Expr::FieldValue { value, .. }
        | Expr::Cast { expr: value, .. }
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
        | Expr::NewtypeCtor { value, .. } => collect_expr_function_refs(value, functions, refs),
        Expr::Default { value, fallback }
        | Expr::DefaultTry { value, fallback }
        | Expr::OptionOrElseValue { value, fallback }
        | Expr::OptionOrElseTry { value, fallback }
        | Expr::Binary {
            left: value,
            right: fallback,
            ..
        } => {
            collect_expr_function_refs(value, functions, refs);
            collect_expr_function_refs(fallback, functions, refs);
        }
        Expr::LetIn { value, body, .. } => {
            collect_expr_function_refs(value, functions, refs);
            collect_expr_function_refs(body, functions, refs);
        }
        Expr::OptionOrElse { fallback, .. } => {
            collect_expr_function_refs(fallback, functions, refs);
        }
        Expr::AllowedCommand { args, .. }
        | Expr::Array(args)
        | Expr::Tuple(args)
        | Expr::Variant { args, .. } => collect_exprs_function_refs(args, functions, refs),
        Expr::Range { start, end, .. } => {
            collect_expr_function_refs(start, functions, refs);
            collect_expr_function_refs(end, functions, refs);
        }
        Expr::Map(entries) => {
            for (key, value) in entries {
                collect_expr_function_refs(key, functions, refs);
                collect_expr_function_refs(value, functions, refs);
            }
        }
        Expr::Record(fields) => {
            for (_, value) in fields {
                collect_expr_function_refs(value, functions, refs);
            }
        }
        Expr::RecordPattern(fields) => {
            for (_, value) in fields {
                if let Some(value) = value {
                    collect_expr_function_refs(value, functions, refs);
                }
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
        } => collect_expr_function_refs(index, functions, refs),
        Expr::Slice { start, end, .. } | Expr::StringSlice { start, end, .. } => {
            collect_expr_function_refs(start, functions, refs);
            collect_expr_function_refs(end, functions, refs);
        }
        Expr::StringSliceValue { value, start, end } => {
            collect_expr_function_refs(value, functions, refs);
            collect_expr_function_refs(start, functions, refs);
            collect_expr_function_refs(end, functions, refs);
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
        | Expr::ResultFlatMapValue { value, mapper } => {
            collect_expr_function_refs(value, functions, refs);
            collect_expr_function_refs(mapper, functions, refs);
        }
        Expr::OptionApValue { function, value } | Expr::ResultApValue { function, value } => {
            collect_expr_function_refs(function, functions, refs);
            collect_expr_function_refs(value, functions, refs);
        }
        Expr::ArrayFold {
            initial, reducer, ..
        } => {
            collect_expr_function_refs(initial, functions, refs);
            collect_expr_function_refs(reducer, functions, refs);
        }
        Expr::ArrayFoldValue {
            value,
            initial,
            reducer,
        } => {
            collect_expr_function_refs(value, functions, refs);
            collect_expr_function_refs(initial, functions, refs);
            collect_expr_function_refs(reducer, functions, refs);
        }
        Expr::ArrayTakeValue { value, count }
        | Expr::ArrayDropValue { value, count }
        | Expr::JoinValue {
            value,
            separator: count,
        }
        | Expr::ArrayContainsValue { value, item: count }
        | Expr::ArrayIndexOfValue { value, item: count }
        | Expr::MapHasValue { value, key: count }
        | Expr::StringContainsValue {
            value,
            needle: count,
        }
        | Expr::StringIndexOfValue {
            value,
            needle: count,
        }
        | Expr::StringStartsWithValue {
            value,
            prefix: count,
        }
        | Expr::StringEndsWithValue {
            value,
            suffix: count,
        }
        | Expr::StringRepeatValue { value, count }
        | Expr::StringSplitValue {
            value,
            separator: count,
        } => {
            collect_expr_function_refs(value, functions, refs);
            collect_expr_function_refs(count, functions, refs);
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
            collect_expr_function_refs(key, functions, refs);
            collect_expr_function_refs(value, functions, refs);
        }
        Expr::StringReplace { from, to, .. } => {
            collect_expr_function_refs(from, functions, refs);
            collect_expr_function_refs(to, functions, refs);
        }
        Expr::StringReplaceValue { value, from, to } => {
            collect_expr_function_refs(value, functions, refs);
            collect_expr_function_refs(from, functions, refs);
            collect_expr_function_refs(to, functions, refs);
        }
        Expr::IfElse {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_expr_function_refs(condition, functions, refs);
            collect_expr_function_refs(then_expr, functions, refs);
            collect_expr_function_refs(else_expr, functions, refs);
        }
        Expr::Match { value, arms } => {
            collect_expr_function_refs(value, functions, refs);
            for arm in arms {
                if let Some(pattern) = &arm.pattern {
                    collect_expr_function_refs(pattern, functions, refs);
                }
                if let Some(guard) = &arm.guard {
                    collect_expr_function_refs(guard, functions, refs);
                }
                collect_expr_function_refs(&arm.expr, functions, refs);
            }
        }
        Expr::Lambda { body, .. } => collect_expr_function_refs(body, functions, refs),
        Expr::Do { steps, result } => {
            for step in steps {
                match step {
                    DoStep::Bind { expr, .. } | DoStep::Let { expr, .. } => {
                        collect_expr_function_refs(expr, functions, refs);
                    }
                }
            }
            collect_expr_function_refs(result, functions, refs);
        }
        Expr::Pipeline { input, .. }
        | Expr::TryPipeline { input, .. }
        | Expr::PipelineResult { input, .. } => {
            if let Some(input) = input {
                collect_expr_function_refs(input, functions, refs);
            }
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
        | Expr::HasCommand(_)
        | Expr::TupleField { .. }
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
        | Expr::Closure { .. } => {}
    }
}

pub(super) fn collect_exprs_function_refs(
    values: &[Expr],
    functions: &HashSet<String>,
    refs: &mut HashSet<String>,
) {
    for value in values {
        collect_expr_function_refs(value, functions, refs);
    }
}
