use super::commands::emit_allowed_command;
use super::expressions::{emit_call_arg, emit_call_head, emit_expr};
use super::shell::emit_shell_command;
use crate::{DoStep, Expr};

pub(super) fn emit_async_binding(
    out: &mut String,
    name: &str,
    command: &str,
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local ");
    }
    out.push_str(name);
    out.push_str("_out=\"$(mktemp)\"\n");
    emit_shell_command(out, command);
    out.push_str(" > \"$");
    out.push_str(name);
    out.push_str("_out\" 2>&1 &\n");
    if local {
        out.push_str("local ");
    }
    out.push_str(name);
    out.push_str("_pid=$!\n");
    if readonly && !local {
        out.push_str("readonly ");
        out.push_str(name);
        out.push_str("_out ");
        out.push_str(name);
        out.push_str("_pid\n");
    }
}

pub(super) fn emit_async_expr_binding(
    out: &mut String,
    name: &str,
    value: &Expr,
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local ");
    }
    out.push_str(name);
    out.push_str("_out=\"$(mktemp)\"\n");
    out.push_str("{ ");
    match value {
        Expr::AllowedCommand {
            program,
            args,
            read_args,
            write_args,
            ..
        } => emit_allowed_command(out, program.as_deref(), args, read_args, write_args, false),
        Expr::Call { name, args } => {
            emit_call_head(out, name);
            for arg in args {
                out.push(' ');
                emit_call_arg(out, arg);
            }
        }
        _ => {
            out.push_str("printf '%s\\n' ");
            emit_expr(out, value);
        }
    }
    out.push_str("; } > \"$");
    out.push_str(name);
    out.push_str("_out\" 2>&1 &\n");
    if local {
        out.push_str("local ");
    }
    out.push_str(name);
    out.push_str("_pid=$!\n");
    if readonly && !local {
        out.push_str("readonly ");
        out.push_str(name);
        out.push_str("_out ");
        out.push_str(name);
        out.push_str("_pid\n");
    }
}

pub(super) fn emit_await_binding(
    out: &mut String,
    name: &str,
    future: &str,
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local ");
        out.push_str(name);
        out.push('\n');
    }
    out.push_str("if wait \"$");
    out.push_str(future);
    out.push_str("_pid\"; then\n");
    out.push_str(name);
    out.push_str("=\"$(cat \"$");
    out.push_str(future);
    out.push_str("_out\")\"\n");
    out.push_str("else\n__nacre_status=$?\nexit $__nacre_status\nfi\n");
    if readonly && !local {
        out.push_str("readonly ");
        out.push_str(name);
        out.push('\n');
    }
}

pub(super) fn emit_await_dependencies(out: &mut String, expr: &Expr) {
    let mut futures = Vec::new();
    collect_awaits(expr, &mut futures);
    for future in futures {
        out.push_str("if ! wait \"$");
        out.push_str(&future);
        out.push_str("_pid\"; then\n__nacre_status=$?\nexit $__nacre_status\nfi\n");
    }
}

pub(super) fn collect_awaits(expr: &Expr, futures: &mut Vec<String>) {
    match expr {
        Expr::Await(future) => {
            if !futures.iter().any(|seen| seen == future) {
                futures.push(future.clone());
            }
        }
        Expr::Some(value)
        | Expr::Ok(value)
        | Expr::Err(value)
        | Expr::ResultOption(value)
        | Expr::TryResult(value)
        | Expr::Async(value)
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
        | Expr::JsonStringifyValue { value }
        | Expr::MatchGuardResult(value)
        | Expr::Not(value)
        | Expr::BitNot(value)
        | Expr::Cast { expr: value, .. }
        | Expr::TupleFieldValue { value, .. }
        | Expr::FieldValue { value, .. }
        | Expr::NewtypeCtor { value, .. } => collect_awaits(value, futures),
        Expr::Default { value, fallback }
        | Expr::DefaultTry { value, fallback }
        | Expr::IndexValue {
            value,
            index: fallback,
        }
        | Expr::ArraySliceValue {
            value,
            start: fallback,
            ..
        }
        | Expr::OptionOrElseValue { value, fallback }
        | Expr::OptionOrElseTry { value, fallback }
        | Expr::OptionApValue {
            function: value,
            value: fallback,
        }
        | Expr::ResultApValue {
            function: value,
            value: fallback,
        }
        | Expr::JoinValue {
            value,
            separator: fallback,
        }
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
        }
        | Expr::StringContainsValue {
            value,
            needle: fallback,
        }
        | Expr::StringIndexOfValue {
            value,
            needle: fallback,
        }
        | Expr::StringStartsWithValue {
            value,
            prefix: fallback,
        }
        | Expr::StringEndsWithValue {
            value,
            suffix: fallback,
        }
        | Expr::StringRepeatValue {
            value,
            count: fallback,
        }
        | Expr::StringSplitValue {
            value,
            separator: fallback,
        }
        | Expr::Binary {
            left: value,
            right: fallback,
            ..
        } => {
            collect_awaits(value, futures);
            collect_awaits(fallback, futures);
        }
        Expr::StringRepeat {
            count: fallback, ..
        } => collect_awaits(fallback, futures),
        Expr::ProcessEnv { name: value }
        | Expr::FsIsFile { path: value }
        | Expr::FsIsDir { path: value }
        | Expr::FsSize { path: value }
        | Expr::FsReadLines { path: value }
        | Expr::FsList { path: value }
        | Expr::JsonParse { value } => collect_awaits(value, futures),
        Expr::Range { start, end, .. }
        | Expr::Slice { start, end, .. }
        | Expr::ArrayTakeValue {
            value: start,
            count: end,
        }
        | Expr::ArrayDropValue {
            value: start,
            count: end,
        }
        | Expr::StringSlice { start, end, .. }
        | Expr::StringSliceValue { start, end, .. } => {
            collect_awaits(start, futures);
            collect_awaits(end, futures);
        }
        Expr::FsWriteLines { path, lines } | Expr::FsAppendLines { path, lines } => {
            collect_awaits(path, futures);
            collect_awaits(lines, futures);
        }
        Expr::StringReplace { from, to, .. } => {
            collect_awaits(from, futures);
            collect_awaits(to, futures);
        }
        Expr::StringReplaceValue { value, from, to } => {
            collect_awaits(value, futures);
            collect_awaits(from, futures);
            collect_awaits(to, futures);
        }
        Expr::IfElse {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_awaits(condition, futures);
            collect_awaits(then_expr, futures);
            collect_awaits(else_expr, futures);
        }
        Expr::Array(values) | Expr::Tuple(values) | Expr::Call { args: values, .. } => {
            for value in values {
                collect_awaits(value, futures);
            }
        }
        Expr::AllowedCommand { args, .. } | Expr::Variant { args, .. } => {
            for arg in args {
                collect_awaits(arg, futures);
            }
        }
        Expr::Map(entries) => {
            for (key, value) in entries {
                collect_awaits(key, futures);
                collect_awaits(value, futures);
            }
        }
        Expr::Record(fields) => {
            for (_, value) in fields {
                collect_awaits(value, futures);
            }
        }
        Expr::RecordPattern(fields) => {
            for (_, value) in fields {
                if let Some(value) = value {
                    collect_awaits(value, futures);
                }
            }
        }
        Expr::ArrayPattern { patterns, .. } => {
            for pattern in patterns {
                collect_awaits(pattern, futures);
            }
        }
        Expr::AliasPattern { pattern, .. }
        | Expr::Lambda { body: pattern, .. }
        | Expr::ArrayMap {
            mapper: pattern, ..
        }
        | Expr::ArrayFilter {
            predicate: pattern, ..
        }
        | Expr::ArrayFlatMap {
            mapper: pattern, ..
        }
        | Expr::ArrayFind {
            predicate: pattern, ..
        }
        | Expr::ArrayAny {
            predicate: pattern, ..
        }
        | Expr::ArrayAll {
            predicate: pattern, ..
        }
        | Expr::OptionMap {
            mapper: pattern, ..
        }
        | Expr::OptionFlatMap {
            mapper: pattern, ..
        }
        | Expr::ResultMap {
            mapper: pattern, ..
        }
        | Expr::ResultFlatMap {
            mapper: pattern, ..
        } => collect_awaits(pattern, futures),
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
            collect_awaits(value, futures);
            collect_awaits(mapper, futures);
        }
        Expr::ArrayFold {
            initial, reducer, ..
        } => {
            collect_awaits(initial, futures);
            collect_awaits(reducer, futures);
        }
        Expr::ArrayFoldValue {
            value,
            initial,
            reducer,
        } => {
            collect_awaits(value, futures);
            collect_awaits(initial, futures);
            collect_awaits(reducer, futures);
        }
        Expr::Match { value, arms } => {
            collect_awaits(value, futures);
            for arm in arms {
                if let Some(pattern) = &arm.pattern {
                    collect_awaits(pattern, futures);
                }
                if let Some(guard) = &arm.guard {
                    collect_awaits(guard, futures);
                }
                collect_awaits(&arm.expr, futures);
            }
        }
        Expr::LetIn { value, body, .. } => {
            collect_awaits(value, futures);
            collect_awaits(body, futures);
        }
        Expr::Do { steps, result } => {
            for step in steps {
                match step {
                    DoStep::Bind { expr, .. } | DoStep::Let { expr, .. } => {
                        collect_awaits(expr, futures);
                    }
                }
            }
            collect_awaits(result, futures);
        }
        _ => {}
    }
}

pub(super) fn emit_discard_await(out: &mut String, future: &str) {
    out.push_str("if wait \"$");
    out.push_str(future);
    out.push_str("_pid\"; then\nrm -f \"$");
    out.push_str(future);
    out.push_str("_out\"\nelse\n__nacre_status=$?\nrm -f \"$");
    out.push_str(future);
    out.push_str("_out\"\nexit $__nacre_status\nfi\n");
}
