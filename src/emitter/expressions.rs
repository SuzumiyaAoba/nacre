use super::arithmetic::{
    emit_awk_bool, emit_awk_condition, emit_awk_numeric, emit_bash_arithmetic, emit_numeric_call,
    emit_numeric_call_command,
};
use super::async_values::emit_discard_await;
use super::collections::{
    emit_array_bool_value, emit_array_bool_value_expr, emit_array_contains,
    emit_array_contains_value, emit_array_drop_value, emit_array_drop_value_expr,
    emit_array_filter_value, emit_array_filter_value_expr, emit_array_find_value,
    emit_array_find_value_expr, emit_array_first, emit_array_first_value,
    emit_array_flat_map_value, emit_array_flat_map_value_expr, emit_array_fold_value,
    emit_array_fold_value_expr, emit_array_index_of, emit_array_index_of_value,
    emit_array_is_empty_value, emit_array_last, emit_array_last_value, emit_array_len_value,
    emit_array_map_value, emit_array_map_value_expr, emit_array_pop, emit_array_push,
    emit_array_reverse_value, emit_array_reverse_value_expr, emit_array_slice_value,
    emit_array_slice_value_expr, emit_array_sort_value, emit_array_sort_value_expr,
    emit_array_take_value, emit_array_take_value_expr, emit_array_unique_value,
    emit_array_unique_value_expr, emit_is_empty, emit_join, emit_join_value, emit_map_has,
    emit_map_has_value, emit_map_is_empty_value, emit_map_keys_value, emit_map_keys_value_expr,
    emit_map_len_value, emit_map_remove, emit_map_set, emit_map_values_value,
    emit_map_values_value_expr, emit_std_str_join,
};
use super::commands::{
    emit_allowed_command, emit_allowed_command_result_discard, emit_process_env,
};
use super::control_flow::{emit_range_value, emit_try_result_value};
use super::filesystem::{
    emit_fs_append_lines_expr, emit_fs_list_value, emit_fs_read_lines_value, emit_fs_size,
    emit_fs_test, emit_fs_write_lines_expr, emit_path_basename, emit_path_dirname,
    emit_path_exists, emit_path_extname, emit_path_is_absolute, emit_path_method_expr,
    emit_path_stem, PathMethod,
};
use super::json::{emit_json_stringify, emit_json_stringify_value};
use super::matching::{
    collect_concat_parts, emit_command_result_value, emit_concat, emit_match_expr,
};
use super::option_result::{
    emit_default, emit_default_try, emit_option_ap, emit_option_ap_value, emit_option_flat_map,
    emit_option_flat_map_value, emit_option_map, emit_option_map_value, emit_option_or_else,
    emit_option_or_else_try, emit_option_or_else_value, emit_option_some, emit_result_ap,
    emit_result_ap_value, emit_result_err, emit_result_flat_map, emit_result_flat_map_value,
    emit_result_map, emit_result_map_value, emit_result_option,
};
use super::quoting::{emit_bash_string, emit_shell_word, emit_string};
use super::shell::{emit_pipeline, emit_pipeline_capture, emit_shell_command};
use super::strings::{
    emit_string_case_transform, emit_string_contains, emit_string_ends_with, emit_string_index_of,
    emit_string_is_empty, emit_string_len, emit_string_predicate_expr, emit_string_repeat,
    emit_string_repeat_expr, emit_string_replace, emit_string_replace_expr, emit_string_slice,
    emit_string_slice_expr, emit_string_split_expr_value, emit_string_split_value,
    emit_string_starts_with, emit_string_transform_expr, emit_string_trim, emit_string_trim_end,
    emit_string_trim_expr, emit_string_trim_start, emit_string_unary_expr, StringPredicate,
    StringTransform, StringUnary,
};
use super::values::{
    emit_array, emit_closure, emit_field, emit_field_value, emit_ident_value, emit_index,
    emit_index_value, emit_let_in, emit_map, emit_record_value, emit_tuple_field,
    emit_tuple_field_value, emit_tuple_value, emit_variable_ref, emit_variant,
};
use super::AllowedCommandParts;
use crate::symbols::is_identifier;
use crate::Expr;

pub(super) fn emit_discard_expr(out: &mut String, expr: &Expr) {
    match expr {
        Expr::MatchGuardResult(value) => emit_discard_expr(out, value),
        Expr::AllowedCommand {
            program,
            args,
            result,
            read_args,
            write_args,
            ..
        } => {
            if *result {
                emit_allowed_command_result_discard(
                    out,
                    AllowedCommandParts::new(program.as_deref(), args, read_args, write_args),
                );
            } else {
                emit_allowed_command(out, program.as_deref(), args, read_args, write_args, false);
            }
            out.push('\n');
        }
        Expr::Command { command, checked } => {
            emit_shell_command(out, command);
            out.push_str(" >/dev/null");
            if *checked {
                out.push_str(" || exit $?");
            }
            out.push('\n');
        }
        Expr::CommandResult { command } => {
            out.push_str("if ");
            emit_shell_command(out, command);
            out.push_str(" >/dev/null 2>/dev/null; then :; else :; fi\n");
        }
        Expr::Pipeline { input, commands } => {
            emit_pipeline(out, input.as_deref(), commands);
            out.push_str(" >/dev/null\n");
        }
        Expr::TryPipeline { input, commands } => {
            emit_pipeline(out, input.as_deref(), commands);
            out.push_str(" >/dev/null || exit $?\n");
        }
        Expr::PipelineResult { input, commands } => {
            out.push_str("if ");
            emit_pipeline(out, input.as_deref(), commands);
            out.push_str(" >/dev/null 2>/dev/null; then :; else :; fi\n");
        }
        Expr::AsyncCommand(command) => {
            emit_shell_command(out, command);
            out.push_str(" >/dev/null 2>&1 &\n");
        }
        Expr::Async(value) => emit_discard_expr(out, value),
        Expr::Await(future) => emit_discard_await(out, future),
        Expr::Cast { expr, .. } => emit_discard_expr(out, expr),
        Expr::Call { name, args } => {
            emit_call_head(out, name);
            for arg in args {
                out.push(' ');
                emit_call_arg(out, arg);
            }
            out.push_str(" >/dev/null\n");
        }
        Expr::Binary {
            op: crate::BinaryOp::Concat,
            ..
        } => {
            let mut parts = Vec::new();
            collect_concat_parts(expr, &mut parts);
            for part in parts {
                emit_discard_expr(out, part);
            }
        }
        Expr::Binary { op, .. } if op.is_bitwise() => {
            emit_bash_arithmetic(out, expr);
            out.push_str(" >/dev/null\n");
        }
        Expr::BitNot(_) => {
            emit_bash_arithmetic(out, expr);
            out.push_str(" >/dev/null\n");
        }
        Expr::Binary { op, .. } if op.is_arithmetic() => {
            emit_awk_numeric(out, expr);
            out.push_str(" >/dev/null\n");
        }
        Expr::Binary { .. } => {
            emit_awk_bool(out, expr);
            out.push_str(" >/dev/null\n");
        }
        Expr::Not(_) => {
            emit_awk_bool(out, expr);
            out.push_str(" >/dev/null\n");
        }
        Expr::IfElse {
            condition,
            then_expr,
            else_expr,
        } => {
            out.push_str("if ");
            emit_condition(out, condition);
            out.push_str("; then\n");
            emit_discard_expr(out, then_expr);
            out.push_str("else\n");
            emit_discard_expr(out, else_expr);
            out.push_str("fi\n");
        }
        Expr::NewtypeCtor { value, .. }
        | Expr::Some(value)
        | Expr::Ok(value)
        | Expr::Err(value)
        | Expr::ResultOption(value)
        | Expr::TryResult(value)
        | Expr::PathExists(value)
        | Expr::FsIsFile { path: value }
        | Expr::FsIsDir { path: value }
        | Expr::FsSize { path: value }
        | Expr::FsReadLines { path: value }
        | Expr::FsList { path: value } => emit_discard_expr(out, value),
        Expr::Variant { .. } => {
            emit_expr(out, expr);
            out.push_str(" >/dev/null\n");
        }
        Expr::Default { value, fallback } | Expr::DefaultTry { value, fallback } => {
            emit_discard_expr(out, value);
            emit_discard_expr(out, fallback);
        }
        Expr::OptionOrElseTry { value, fallback } => {
            emit_discard_expr(out, value);
            emit_discard_expr(out, fallback);
        }
        Expr::FsWriteLines { path, lines } => {
            emit_discard_expr(out, path);
            emit_discard_expr(out, lines);
        }
        Expr::FsAppendLines { path, lines } => {
            emit_discard_expr(out, path);
            emit_discard_expr(out, lines);
        }
        Expr::Array(values) | Expr::Tuple(values) => {
            for value in values {
                emit_discard_expr(out, value);
            }
        }
        Expr::Range { start, end, .. } => {
            emit_discard_expr(out, start);
            emit_discard_expr(out, end);
        }
        Expr::Map(entries) => {
            for (key, value) in entries {
                emit_discard_expr(out, key);
                emit_discard_expr(out, value);
            }
        }
        Expr::Record(fields) => {
            for (_, value) in fields {
                emit_discard_expr(out, value);
            }
        }
        Expr::RecordPattern(fields) => {
            for (_, value) in fields {
                if let Some(value) = value {
                    emit_discard_expr(out, value);
                }
            }
        }
        Expr::Match { .. } => {
            out.push_str(": ");
            emit_expr(out, expr);
            out.push('\n');
        }
        Expr::Int(_)
        | Expr::Float(_)
        | Expr::Bool(_)
        | Expr::Unit
        | Expr::None
        | Expr::String(_)
        | Expr::RawString(_)
        | Expr::Ident(_)
        | Expr::ProcessArgs
        | Expr::ProcessEnv { .. }
        | Expr::CliParse
        | Expr::JsonParse { .. }
        | Expr::JsonStringify { .. }
        | Expr::JsonStringifyValue { .. }
        | Expr::HasCommand(_)
        | Expr::Index { .. }
        | Expr::IndexValue { .. }
        | Expr::Slice { .. }
        | Expr::ArraySliceValue { .. }
        | Expr::ArrayTake { .. }
        | Expr::ArrayTakeValue { .. }
        | Expr::ArrayDrop { .. }
        | Expr::ArrayDropValue { .. }
        | Expr::TupleField { .. }
        | Expr::TupleFieldValue { .. }
        | Expr::Field { .. }
        | Expr::FieldValue { .. }
        | Expr::Value(_)
        | Expr::Len(_)
        | Expr::ArrayLenValue(_)
        | Expr::MapLenValue(_)
        | Expr::IsEmpty(_)
        | Expr::ArrayIsEmptyValue(_)
        | Expr::MapIsEmptyValue(_)
        | Expr::ArrayFirst(_)
        | Expr::ArrayFirstValue(_)
        | Expr::ArrayLast(_)
        | Expr::ArrayLastValue(_)
        | Expr::ArrayReverse(_)
        | Expr::ArrayReverseValue(_)
        | Expr::ArraySort(_)
        | Expr::ArraySortValue(_)
        | Expr::ArrayUnique(_)
        | Expr::ArrayUniqueValue(_)
        | Expr::ArrayMap { .. }
        | Expr::ArrayMapValue { .. }
        | Expr::ArrayFilter { .. }
        | Expr::ArrayFilterValue { .. }
        | Expr::ArrayFlatMap { .. }
        | Expr::ArrayFlatMapValue { .. }
        | Expr::ArrayFind { .. }
        | Expr::ArrayFindValue { .. }
        | Expr::ArrayAny { .. }
        | Expr::ArrayAnyValue { .. }
        | Expr::ArrayAll { .. }
        | Expr::ArrayAllValue { .. }
        | Expr::ArrayFold { .. }
        | Expr::ArrayFoldValue { .. }
        | Expr::OptionMap { .. }
        | Expr::OptionMapValue { .. }
        | Expr::OptionFlatMap { .. }
        | Expr::OptionFlatMapValue { .. }
        | Expr::ResultMap { .. }
        | Expr::ResultMapValue { .. }
        | Expr::ResultFlatMap { .. }
        | Expr::ResultFlatMapValue { .. }
        | Expr::OptionAp { .. }
        | Expr::OptionApValue { .. }
        | Expr::ResultAp { .. }
        | Expr::ResultApValue { .. }
        | Expr::OptionOrElse { .. }
        | Expr::OptionOrElseValue { .. }
        | Expr::Join { .. }
        | Expr::JoinValue { .. }
        | Expr::ArrayPush { .. }
        | Expr::ArrayPop { .. }
        | Expr::MapSet { .. }
        | Expr::MapRemove { .. }
        | Expr::ArrayContains { .. }
        | Expr::ArrayContainsValue { .. }
        | Expr::ArrayIndexOf { .. }
        | Expr::ArrayIndexOfValue { .. }
        | Expr::MapKeys(_)
        | Expr::MapKeysValue(_)
        | Expr::MapValues(_)
        | Expr::MapValuesValue(_)
        | Expr::MapHas { .. }
        | Expr::MapHasValue { .. }
        | Expr::StringContains { .. }
        | Expr::StringContainsValue { .. }
        | Expr::StringIndexOf { .. }
        | Expr::StringIndexOfValue { .. }
        | Expr::StringStartsWith { .. }
        | Expr::StringStartsWithValue { .. }
        | Expr::StringEndsWith { .. }
        | Expr::StringEndsWithValue { .. }
        | Expr::StringLen(_)
        | Expr::StringLenValue(_)
        | Expr::StringIsEmpty(_)
        | Expr::StringIsEmptyValue(_)
        | Expr::StringSlice { .. }
        | Expr::StringSliceValue { .. }
        | Expr::StringTrim(_)
        | Expr::StringTrimValue(_)
        | Expr::StringTrimStart(_)
        | Expr::StringTrimStartValue(_)
        | Expr::StringTrimEnd(_)
        | Expr::StringTrimEndValue(_)
        | Expr::StringToUpper(_)
        | Expr::StringToUpperValue(_)
        | Expr::StringToLower(_)
        | Expr::StringToLowerValue(_)
        | Expr::StringRepeat { .. }
        | Expr::StringRepeatValue { .. }
        | Expr::StringSplit { .. }
        | Expr::StringSplitValue { .. }
        | Expr::StringReplace { .. }
        | Expr::StringReplaceValue { .. }
        | Expr::PathBasename(_)
        | Expr::PathBasenameValue(_)
        | Expr::PathDirname(_)
        | Expr::PathDirnameValue(_)
        | Expr::PathStem(_)
        | Expr::PathStemValue(_)
        | Expr::PathExtname(_)
        | Expr::PathExtnameValue(_)
        | Expr::PathIsAbsolute(_)
        | Expr::PathIsAbsoluteValue(_)
        | Expr::Env(_)
        | Expr::EnvDefault { .. } => {}
        Expr::LetIn {
            name,
            annotation,
            value,
            body,
        } => {
            emit_let_in(out, name, annotation.as_ref(), value, body);
        }
        Expr::Do { .. } => unreachable!("do expressions are lowered before emission"),
        Expr::NamedArg { .. } => unreachable!("named arguments are lowered before emission"),
        Expr::Closure { name, captures } => emit_closure(out, name, captures),
        Expr::ArrayPattern { .. } | Expr::AliasPattern { .. } => {
            unreachable!("match patterns are only emitted by the match emitter")
        }
        Expr::Lambda { .. } => unreachable!("lambdas are lowered before emission"),
    }
}

pub(super) fn emit_bound_expr(out: &mut String, expr: &Expr) {
    match expr {
        Expr::Command { command, checked } => {
            out.push_str("\"$(");
            emit_shell_command(out, command);
            out.push_str(")\"");
            if *checked {
                out.push_str(" || exit $?");
            }
            out.push('\n');
        }
        Expr::Binary { op, .. } if *op == crate::BinaryOp::Concat => {
            emit_expr(out, expr);
            out.push('\n');
        }
        Expr::Binary { op, .. } if op.is_bitwise() => {
            emit_bash_arithmetic(out, expr);
            out.push('\n');
        }
        Expr::BitNot(_) => {
            emit_bash_arithmetic(out, expr);
            out.push('\n');
        }
        Expr::Binary { op, .. } if op.is_arithmetic() => {
            out.push_str("$(");
            emit_awk_numeric(out, expr);
            out.push_str(")\n");
        }
        Expr::Binary { .. } => {
            out.push_str("$(");
            emit_awk_bool(out, expr);
            out.push_str(")\n");
        }
        Expr::Not(_) => {
            out.push_str("$(");
            emit_awk_bool(out, expr);
            out.push_str(")\n");
        }
        _ => {
            emit_expr(out, expr);
            out.push('\n');
        }
    }
}

pub(super) fn emit_condition(out: &mut String, expr: &Expr) {
    match expr {
        Expr::Binary { .. } | Expr::Not(_) => emit_awk_condition(out, expr),
        _ => emit_expr(out, expr),
    }
}

pub(super) fn emit_expr(out: &mut String, expr: &Expr) {
    match expr {
        Expr::Int(value) => out.push_str(&value.to_string()),
        Expr::Float(value) => out.push_str(value),
        Expr::Bool(true) => out.push_str("true"),
        Expr::Bool(false) => out.push_str("false"),
        Expr::Unit => emit_bash_string(out, ""),
        Expr::Some(value) => emit_option_some(out, value),
        Expr::Ok(value) => emit_option_some(out, value),
        Expr::Err(value) => emit_result_err(out, value),
        Expr::ResultOption(value) => emit_result_option(out, value),
        Expr::TryResult(value) => emit_try_result_value(out, value),
        Expr::MatchGuardResult(value) => emit_expr(out, value),
        Expr::None => emit_shell_word(out, "0"),
        Expr::Default { value, fallback } => emit_default(out, value, fallback),
        Expr::DefaultTry { value, fallback } => emit_default_try(out, value, fallback),
        Expr::String(value) => emit_string(out, value),
        Expr::RawString(value) => emit_bash_string(out, value),
        Expr::AllowedCommand {
            program,
            args,
            read_args,
            write_args,
            ..
        } => emit_allowed_command(out, program.as_deref(), args, read_args, write_args, true),
        Expr::Command { command, checked } => {
            out.push_str("\"$(");
            emit_shell_command(out, command);
            out.push(')');
            out.push('"');
            if *checked {
                out.push_str(" || exit $?");
            }
        }
        Expr::CommandResult { command } => emit_command_result_value(out, command),
        Expr::AsyncCommand(command) => emit_shell_word(out, command),
        Expr::Async(_) => unreachable!("async expressions must be bound before emission"),
        Expr::Await(future) => {
            out.push_str("\"$(cat \"$");
            out.push_str(future);
            out.push_str("_out\")\"");
        }
        Expr::Pipeline { input, commands } => {
            emit_pipeline_capture(out, input.as_deref(), commands)
        }
        Expr::TryPipeline { input, commands } => {
            emit_pipeline_capture(out, input.as_deref(), commands);
            out.push_str(" || exit $?");
        }
        Expr::PipelineResult { input, commands } => {
            let mut command = String::new();
            emit_pipeline(&mut command, input.as_deref(), commands);
            emit_command_result_value(out, &command);
        }
        Expr::ProcessArgs => out.push_str("\"${args[@]}\""),
        Expr::ProcessEnv { name } => emit_process_env(out, name),
        Expr::FsIsFile { path } => emit_fs_test(out, "-f", path),
        Expr::FsIsDir { path } => emit_fs_test(out, "-d", path),
        Expr::FsSize { path } => emit_fs_size(out, path),
        Expr::FsReadLines { path } => emit_fs_read_lines_value(out, path),
        Expr::FsList { path } => emit_fs_list_value(out, path),
        Expr::FsWriteLines { path, lines } => emit_fs_write_lines_expr(out, path, lines),
        Expr::FsAppendLines { path, lines } => emit_fs_append_lines_expr(out, path, lines),
        Expr::CliParse => emit_map(out, &[]),
        Expr::JsonParse { .. } => emit_map(out, &[]),
        Expr::JsonStringify { name } => emit_json_stringify(out, name),
        Expr::JsonStringifyValue { value } => emit_json_stringify_value(out, value),
        Expr::HasCommand(command) => {
            out.push_str("$(command -v ");
            emit_shell_word(out, command);
            out.push_str(" >/dev/null 2>&1 && printf true || printf false)");
        }
        Expr::PathExists(path) => emit_path_exists(out, path),
        Expr::Array(values) => emit_array(out, values),
        Expr::Range {
            start,
            end,
            inclusive,
        } => emit_range_value(out, start, end, *inclusive),
        Expr::Map(entries) => emit_map(out, entries),
        Expr::Record(fields) => emit_record_value(out, fields),
        Expr::RecordPattern(_) => emit_bash_string(out, ""),
        Expr::Tuple(values) => emit_tuple_value(out, values),
        Expr::Index { name, index } => emit_index(out, name, index),
        Expr::IndexValue { value, index } => emit_index_value(out, value, index),
        Expr::Slice { name, start, end } => emit_array_slice_value(out, name, start, end),
        Expr::ArraySliceValue { value, start, end } => {
            emit_array_slice_value_expr(out, value, start, end)
        }
        Expr::ArrayTake { name, count } => emit_array_take_value(out, name, count),
        Expr::ArrayTakeValue { value, count } => emit_array_take_value_expr(out, value, count),
        Expr::ArrayDrop { name, count } => emit_array_drop_value(out, name, count),
        Expr::ArrayDropValue { value, count } => emit_array_drop_value_expr(out, value, count),
        Expr::TupleField { name, field } => emit_tuple_field(out, name, *field),
        Expr::TupleFieldValue { value, field } => emit_tuple_field_value(out, value, *field),
        Expr::Field { name, field } => emit_field(out, name, field),
        Expr::FieldValue { value, field } => emit_field_value(out, value, field),
        Expr::NewtypeCtor { value, .. } => emit_expr(out, value),
        Expr::Variant {
            name,
            args,
            field_types,
        } => emit_variant(out, name, args, field_types),
        Expr::Cast { expr, .. } => emit_expr(out, expr),
        Expr::Call { name, args } if name == "str.join" => emit_std_str_join(out, args),
        Expr::Call { name, args } => emit_call(out, name, args),
        Expr::Value(name) => emit_variable_ref(out, name),
        Expr::Len(name) => {
            out.push_str("\"${#");
            out.push_str(name);
            out.push_str("[@]}\"");
        }
        Expr::ArrayLenValue(value) => emit_array_len_value(out, value),
        Expr::MapLenValue(value) => emit_map_len_value(out, value),
        Expr::IsEmpty(name) => emit_is_empty(out, name),
        Expr::ArrayIsEmptyValue(value) => emit_array_is_empty_value(out, value),
        Expr::MapIsEmptyValue(value) => emit_map_is_empty_value(out, value),
        Expr::ArrayFirst(name) => emit_array_first(out, name),
        Expr::ArrayFirstValue(value) => emit_array_first_value(out, value),
        Expr::ArrayLast(name) => emit_array_last(out, name),
        Expr::ArrayLastValue(value) => emit_array_last_value(out, value),
        Expr::ArrayReverse(name) => emit_array_reverse_value(out, name),
        Expr::ArrayReverseValue(value) => emit_array_reverse_value_expr(out, value),
        Expr::ArraySort(name) => emit_array_sort_value(out, name),
        Expr::ArraySortValue(value) => emit_array_sort_value_expr(out, value),
        Expr::ArrayUnique(name) => emit_array_unique_value(out, name),
        Expr::ArrayUniqueValue(value) => emit_array_unique_value_expr(out, value),
        Expr::ArrayMap { name, mapper } => emit_array_map_value(out, name, mapper),
        Expr::ArrayMapValue { value, mapper } => emit_array_map_value_expr(out, value, mapper),
        Expr::ArrayFilter { name, predicate } => emit_array_filter_value(out, name, predicate),
        Expr::ArrayFilterValue { value, predicate } => {
            emit_array_filter_value_expr(out, value, predicate)
        }
        Expr::ArrayFlatMap { name, mapper } => emit_array_flat_map_value(out, name, mapper),
        Expr::ArrayFlatMapValue { value, mapper } => {
            emit_array_flat_map_value_expr(out, value, mapper)
        }
        Expr::ArrayFind { name, predicate } => emit_array_find_value(out, name, predicate),
        Expr::ArrayFindValue { value, predicate } => {
            emit_array_find_value_expr(out, value, predicate)
        }
        Expr::ArrayAny { name, predicate } => emit_array_bool_value(out, name, predicate, true),
        Expr::ArrayAnyValue { value, predicate } => {
            emit_array_bool_value_expr(out, value, predicate, true)
        }
        Expr::ArrayAll { name, predicate } => emit_array_bool_value(out, name, predicate, false),
        Expr::ArrayAllValue { value, predicate } => {
            emit_array_bool_value_expr(out, value, predicate, false)
        }
        Expr::ArrayFold {
            name,
            initial,
            reducer,
        } => emit_array_fold_value(out, name, initial, reducer),
        Expr::ArrayFoldValue {
            value,
            initial,
            reducer,
        } => emit_array_fold_value_expr(out, value, initial, reducer),
        Expr::OptionMap { name, mapper } => emit_option_map(out, name, mapper),
        Expr::OptionMapValue { value, mapper } => emit_option_map_value(out, value, mapper),
        Expr::OptionFlatMap { name, mapper } => emit_option_flat_map(out, name, mapper),
        Expr::OptionFlatMapValue { value, mapper } => {
            emit_option_flat_map_value(out, value, mapper)
        }
        Expr::ResultMap { name, mapper } => emit_result_map(out, name, mapper),
        Expr::ResultMapValue { value, mapper } => emit_result_map_value(out, value, mapper),
        Expr::ResultFlatMap { name, mapper } => emit_result_flat_map(out, name, mapper),
        Expr::ResultFlatMapValue { value, mapper } => {
            emit_result_flat_map_value(out, value, mapper)
        }
        Expr::OptionAp { name, value } => emit_option_ap(out, name, value),
        Expr::OptionApValue { function, value } => emit_option_ap_value(out, function, value),
        Expr::ResultAp { name, value } => emit_result_ap(out, name, value),
        Expr::ResultApValue { function, value } => emit_result_ap_value(out, function, value),
        Expr::OptionOrElse { name, fallback } => emit_option_or_else(out, name, fallback),
        Expr::OptionOrElseValue { value, fallback } => {
            emit_option_or_else_value(out, value, fallback)
        }
        Expr::OptionOrElseTry { value, fallback } => emit_option_or_else_try(out, value, fallback),
        Expr::Join { name, separator } => emit_join(out, name, separator),
        Expr::JoinValue { value, separator } => emit_join_value(out, value, separator),
        Expr::ArrayPush { name, value } => {
            emit_array_push(out, name, value);
            emit_bash_string(out, "");
        }
        Expr::ArrayPop { name } => {
            emit_array_pop(out, name);
            emit_bash_string(out, "");
        }
        Expr::MapSet { name, key, value } => {
            emit_map_set(out, name, key, value);
            emit_bash_string(out, "");
        }
        Expr::MapRemove { name, key } => {
            emit_map_remove(out, name, key);
            emit_bash_string(out, "");
        }
        Expr::ArrayContains { name, value } => emit_array_contains(out, name, value),
        Expr::ArrayContainsValue { value, item } => emit_array_contains_value(out, value, item),
        Expr::ArrayIndexOf { name, value } => emit_array_index_of(out, name, value),
        Expr::ArrayIndexOfValue { value, item } => emit_array_index_of_value(out, value, item),
        Expr::MapKeys(name) => emit_map_keys_value(out, name),
        Expr::MapKeysValue(value) => emit_map_keys_value_expr(out, value),
        Expr::MapValues(name) => emit_map_values_value(out, name),
        Expr::MapValuesValue(value) => emit_map_values_value_expr(out, value),
        Expr::MapHas { name, key } => emit_map_has(out, name, key),
        Expr::MapHasValue { value, key } => emit_map_has_value(out, value, key),
        Expr::StringContains { name, needle } => emit_string_contains(out, name, needle),
        Expr::StringContainsValue { value, needle } => {
            emit_string_predicate_expr(out, value, needle, StringPredicate::Contains)
        }
        Expr::StringIndexOf { name, needle } => emit_string_index_of(out, name, needle),
        Expr::StringIndexOfValue { value, needle } => {
            emit_string_predicate_expr(out, value, needle, StringPredicate::IndexOf)
        }
        Expr::StringStartsWith { name, prefix } => emit_string_starts_with(out, name, prefix),
        Expr::StringStartsWithValue { value, prefix } => {
            emit_string_predicate_expr(out, value, prefix, StringPredicate::StartsWith)
        }
        Expr::StringEndsWith { name, suffix } => emit_string_ends_with(out, name, suffix),
        Expr::StringEndsWithValue { value, suffix } => {
            emit_string_predicate_expr(out, value, suffix, StringPredicate::EndsWith)
        }
        Expr::StringLen(name) => emit_string_len(out, name),
        Expr::StringLenValue(value) => emit_string_unary_expr(out, value, StringUnary::Len),
        Expr::StringIsEmpty(name) => emit_string_is_empty(out, name),
        Expr::StringIsEmptyValue(value) => emit_string_unary_expr(out, value, StringUnary::IsEmpty),
        Expr::StringSlice { name, start, end } => emit_string_slice(out, name, start, end),
        Expr::StringSliceValue { value, start, end } => {
            emit_string_slice_expr(out, value, start, end)
        }
        Expr::StringTrim(name) => emit_string_trim(out, name),
        Expr::StringTrimValue(value) => emit_string_trim_expr(out, value),
        Expr::StringTrimStart(name) => emit_string_trim_start(out, name),
        Expr::StringTrimStartValue(value) => {
            emit_string_transform_expr(out, value, StringTransform::TrimStart)
        }
        Expr::StringTrimEnd(name) => emit_string_trim_end(out, name),
        Expr::StringTrimEndValue(value) => {
            emit_string_transform_expr(out, value, StringTransform::TrimEnd)
        }
        Expr::StringToUpper(name) => {
            emit_string_case_transform(out, name, "[:lower:]", "[:upper:]")
        }
        Expr::StringToUpperValue(value) => {
            emit_string_transform_expr(out, value, StringTransform::ToUpper)
        }
        Expr::StringToLower(name) => {
            emit_string_case_transform(out, name, "[:upper:]", "[:lower:]")
        }
        Expr::StringToLowerValue(value) => {
            emit_string_transform_expr(out, value, StringTransform::ToLower)
        }
        Expr::StringRepeat { name, count } => emit_string_repeat(out, name, count),
        Expr::StringRepeatValue { value, count } => emit_string_repeat_expr(out, value, count),
        Expr::StringSplit { name, separator } => emit_string_split_value(out, name, separator),
        Expr::StringSplitValue { value, separator } => {
            emit_string_split_expr_value(out, value, separator)
        }
        Expr::StringReplace { name, from, to } => emit_string_replace(out, name, from, to),
        Expr::StringReplaceValue { value, from, to } => {
            emit_string_replace_expr(out, value, from, to)
        }
        Expr::PathBasename(name) => emit_path_basename(out, name),
        Expr::PathBasenameValue(value) => emit_path_method_expr(out, value, PathMethod::Basename),
        Expr::PathDirname(name) => emit_path_dirname(out, name),
        Expr::PathDirnameValue(value) => emit_path_method_expr(out, value, PathMethod::Dirname),
        Expr::PathStem(name) => emit_path_stem(out, name),
        Expr::PathStemValue(value) => emit_path_method_expr(out, value, PathMethod::Stem),
        Expr::PathExtname(name) => emit_path_extname(out, name),
        Expr::PathExtnameValue(value) => emit_path_method_expr(out, value, PathMethod::Extname),
        Expr::PathIsAbsolute(name) => emit_path_is_absolute(out, name),
        Expr::PathIsAbsoluteValue(value) => {
            emit_path_method_expr(out, value, PathMethod::IsAbsolute)
        }
        Expr::EnvDefault { name, default } => {
            out.push('"');
            out.push_str("${");
            out.push_str(name);
            out.push_str(":-");
            out.push_str(default);
            out.push_str("}\"");
        }
        Expr::Env(name) => {
            out.push('"');
            out.push_str("${");
            out.push_str(name);
            out.push_str("}\"");
        }
        Expr::IfElse {
            condition,
            then_expr,
            else_expr,
        } => emit_if_expr(out, condition, then_expr, else_expr),
        Expr::Match { value, arms } => emit_match_expr(out, value, arms),
        Expr::Not(_) => {
            out.push_str("$(");
            emit_awk_bool(out, expr);
            out.push(')');
        }
        Expr::BitNot(_) => emit_bash_arithmetic(out, expr),
        Expr::Ident(name) => emit_ident_value(out, name),
        Expr::Binary { op, .. } if *op == crate::BinaryOp::Concat => emit_concat(out, expr),
        Expr::Binary { op, .. } if op.is_bitwise() => emit_bash_arithmetic(out, expr),
        Expr::Binary { op, .. } if op.is_arithmetic() => {
            out.push_str("$(");
            emit_awk_numeric(out, expr);
            out.push(')');
        }
        Expr::Binary { .. } => {
            out.push_str("$(");
            emit_awk_bool(out, expr);
            out.push(')');
        }
        Expr::LetIn {
            name,
            annotation,
            value,
            body,
        } => emit_let_in(out, name, annotation.as_ref(), value, body),
        Expr::Do { .. } => unreachable!("do expressions are lowered before emission"),
        Expr::NamedArg { .. } => unreachable!("named arguments are lowered before emission"),
        Expr::Closure { name, captures } => emit_closure(out, name, captures),
        Expr::ArrayPattern { .. } | Expr::AliasPattern { .. } => {
            unreachable!("match patterns are only emitted by the match emitter")
        }
        Expr::Lambda { .. } => unreachable!("lambdas are lowered before emission"),
    }
}

pub(super) fn emit_call(out: &mut String, name: &str, args: &[Expr]) {
    if emit_numeric_call(out, name, args) {
        return;
    }
    out.push_str("\"$(");
    emit_call_command(out, name, args);
    out.push_str(")\"");
}

pub(super) fn emit_call_command(out: &mut String, name: &str, args: &[Expr]) {
    if emit_numeric_call_command(out, name, args) {
        return;
    }
    emit_call_head(out, name);
    for arg in args {
        out.push(' ');
        emit_call_arg(out, arg);
    }
}

pub(super) fn emit_call_head(out: &mut String, name: &str) {
    if is_identifier(name) {
        out.push_str("__nacre_call \"$");
        out.push_str(name);
        out.push('"');
    } else {
        out.push_str(name);
    }
}

pub(super) fn emit_if_expr(out: &mut String, condition: &Expr, then_expr: &Expr, else_expr: &Expr) {
    out.push_str("$(if ");
    emit_condition(out, condition);
    out.push_str("; then printf '%s\\n' ");
    emit_expr(out, then_expr);
    out.push_str("; else printf '%s\\n' ");
    emit_expr(out, else_expr);
    out.push_str("; fi)");
}

pub(super) fn emit_call_arg(out: &mut String, arg: &Expr) {
    match arg {
        Expr::Int(value) => out.push_str(&value.to_string()),
        Expr::Float(value) => out.push_str(value),
        Expr::Bool(true) => emit_shell_word(out, "true"),
        Expr::Bool(false) => emit_shell_word(out, "false"),
        Expr::Unit => emit_shell_word(out, ""),
        Expr::Some(value) => emit_option_some(out, value),
        Expr::Ok(value) => emit_option_some(out, value),
        Expr::Err(value) => emit_result_err(out, value),
        Expr::ResultOption(value) => emit_result_option(out, value),
        Expr::MatchGuardResult(value) => emit_call_arg(out, value),
        Expr::None => emit_shell_word(out, "0"),
        Expr::Default { value, fallback } => emit_default(out, value, fallback),
        Expr::DefaultTry { value, fallback } => emit_default_try(out, value, fallback),
        Expr::String(value) => emit_string(out, value),
        Expr::RawString(value) => emit_bash_string(out, value),
        Expr::Ident(name) => emit_ident_value(out, name),
        Expr::ProcessArgs => out.push_str("\"${args[@]}\""),
        Expr::Index { name, index } => emit_index(out, name, index),
        Expr::IndexValue { value, index } => emit_index_value(out, value, index),
        Expr::Slice { name, start, end } => emit_array_slice_value(out, name, start, end),
        Expr::ArraySliceValue { value, start, end } => {
            emit_array_slice_value_expr(out, value, start, end)
        }
        Expr::ArrayTake { name, count } => emit_array_take_value(out, name, count),
        Expr::ArrayTakeValue { value, count } => emit_array_take_value_expr(out, value, count),
        Expr::ArrayDrop { name, count } => emit_array_drop_value(out, name, count),
        Expr::ArrayDropValue { value, count } => emit_array_drop_value_expr(out, value, count),
        Expr::TupleField { name, field } => emit_tuple_field(out, name, *field),
        Expr::TupleFieldValue { value, field } => emit_tuple_field_value(out, value, *field),
        Expr::Field { name, field } => emit_field(out, name, field),
        Expr::FieldValue { value, field } => emit_field_value(out, value, field),
        Expr::NewtypeCtor { value, .. } => emit_call_arg(out, value),
        Expr::Variant { .. } => emit_expr(out, arg),
        Expr::Cast { expr, .. } => emit_call_arg(out, expr),
        Expr::Call { name, args } if name == "str.join" => emit_std_str_join(out, args),
        Expr::Call { name, args } => emit_call(out, name, args),
        Expr::Value(name) => emit_variable_ref(out, name),
        Expr::Len(name) => {
            out.push_str("\"${#");
            out.push_str(name);
            out.push_str("[@]}\"");
        }
        Expr::ArrayLenValue(value) => emit_array_len_value(out, value),
        Expr::MapLenValue(value) => emit_map_len_value(out, value),
        Expr::IsEmpty(name) => emit_is_empty(out, name),
        Expr::ArrayIsEmptyValue(value) => emit_array_is_empty_value(out, value),
        Expr::MapIsEmptyValue(value) => emit_map_is_empty_value(out, value),
        Expr::ArrayFirst(name) => emit_array_first(out, name),
        Expr::ArrayFirstValue(value) => emit_array_first_value(out, value),
        Expr::ArrayLast(name) => emit_array_last(out, name),
        Expr::ArrayLastValue(value) => emit_array_last_value(out, value),
        Expr::ArrayReverse(name) => emit_array_reverse_value(out, name),
        Expr::ArrayReverseValue(value) => emit_array_reverse_value_expr(out, value),
        Expr::ArraySort(name) => emit_array_sort_value(out, name),
        Expr::ArraySortValue(value) => emit_array_sort_value_expr(out, value),
        Expr::ArrayUnique(name) => emit_array_unique_value(out, name),
        Expr::ArrayUniqueValue(value) => emit_array_unique_value_expr(out, value),
        Expr::ArrayMap { name, mapper } => emit_array_map_value(out, name, mapper),
        Expr::ArrayMapValue { value, mapper } => emit_array_map_value_expr(out, value, mapper),
        Expr::ArrayFilter { name, predicate } => emit_array_filter_value(out, name, predicate),
        Expr::ArrayFilterValue { value, predicate } => {
            emit_array_filter_value_expr(out, value, predicate)
        }
        Expr::ArrayFlatMap { name, mapper } => emit_array_flat_map_value(out, name, mapper),
        Expr::ArrayFlatMapValue { value, mapper } => {
            emit_array_flat_map_value_expr(out, value, mapper)
        }
        Expr::ArrayFind { name, predicate } => emit_array_find_value(out, name, predicate),
        Expr::ArrayFindValue { value, predicate } => {
            emit_array_find_value_expr(out, value, predicate)
        }
        Expr::ArrayAny { name, predicate } => emit_array_bool_value(out, name, predicate, true),
        Expr::ArrayAnyValue { value, predicate } => {
            emit_array_bool_value_expr(out, value, predicate, true)
        }
        Expr::ArrayAll { name, predicate } => emit_array_bool_value(out, name, predicate, false),
        Expr::ArrayAllValue { value, predicate } => {
            emit_array_bool_value_expr(out, value, predicate, false)
        }
        Expr::ArrayFold {
            name,
            initial,
            reducer,
        } => emit_array_fold_value(out, name, initial, reducer),
        Expr::ArrayFoldValue {
            value,
            initial,
            reducer,
        } => emit_array_fold_value_expr(out, value, initial, reducer),
        Expr::OptionMap { name, mapper } => emit_option_map(out, name, mapper),
        Expr::OptionMapValue { value, mapper } => emit_option_map_value(out, value, mapper),
        Expr::OptionFlatMap { name, mapper } => emit_option_flat_map(out, name, mapper),
        Expr::OptionFlatMapValue { value, mapper } => {
            emit_option_flat_map_value(out, value, mapper)
        }
        Expr::ResultMap { name, mapper } => emit_result_map(out, name, mapper),
        Expr::ResultMapValue { value, mapper } => emit_result_map_value(out, value, mapper),
        Expr::ResultFlatMap { name, mapper } => emit_result_flat_map(out, name, mapper),
        Expr::ResultFlatMapValue { value, mapper } => {
            emit_result_flat_map_value(out, value, mapper)
        }
        Expr::OptionAp { name, value } => emit_option_ap(out, name, value),
        Expr::OptionApValue { function, value } => emit_option_ap_value(out, function, value),
        Expr::ResultAp { name, value } => emit_result_ap(out, name, value),
        Expr::ResultApValue { function, value } => emit_result_ap_value(out, function, value),
        Expr::OptionOrElse { name, fallback } => emit_option_or_else(out, name, fallback),
        Expr::OptionOrElseValue { value, fallback } => {
            emit_option_or_else_value(out, value, fallback)
        }
        Expr::OptionOrElseTry { value, fallback } => emit_option_or_else_try(out, value, fallback),
        Expr::Join { name, separator } => emit_join(out, name, separator),
        Expr::JoinValue { value, separator } => emit_join_value(out, value, separator),
        Expr::ArrayPush { name, value } => {
            emit_array_push(out, name, value);
            emit_bash_string(out, "");
        }
        Expr::ArrayPop { name } => {
            emit_array_pop(out, name);
            emit_bash_string(out, "");
        }
        Expr::MapSet { name, key, value } => {
            emit_map_set(out, name, key, value);
            emit_bash_string(out, "");
        }
        Expr::MapRemove { name, key } => {
            emit_map_remove(out, name, key);
            emit_bash_string(out, "");
        }
        Expr::ArrayContains { name, value } => emit_array_contains(out, name, value),
        Expr::ArrayContainsValue { value, item } => emit_array_contains_value(out, value, item),
        Expr::ArrayIndexOf { name, value } => emit_array_index_of(out, name, value),
        Expr::ArrayIndexOfValue { value, item } => emit_array_index_of_value(out, value, item),
        Expr::MapKeys(name) => emit_map_keys_value(out, name),
        Expr::MapKeysValue(value) => emit_map_keys_value_expr(out, value),
        Expr::MapValues(name) => emit_map_values_value(out, name),
        Expr::MapValuesValue(value) => emit_map_values_value_expr(out, value),
        Expr::MapHas { name, key } => emit_map_has(out, name, key),
        Expr::MapHasValue { value, key } => emit_map_has_value(out, value, key),
        Expr::StringContains { name, needle } => emit_string_contains(out, name, needle),
        Expr::StringContainsValue { value, needle } => {
            emit_string_predicate_expr(out, value, needle, StringPredicate::Contains)
        }
        Expr::StringIndexOf { name, needle } => emit_string_index_of(out, name, needle),
        Expr::StringIndexOfValue { value, needle } => {
            emit_string_predicate_expr(out, value, needle, StringPredicate::IndexOf)
        }
        Expr::StringStartsWith { name, prefix } => emit_string_starts_with(out, name, prefix),
        Expr::StringStartsWithValue { value, prefix } => {
            emit_string_predicate_expr(out, value, prefix, StringPredicate::StartsWith)
        }
        Expr::StringEndsWith { name, suffix } => emit_string_ends_with(out, name, suffix),
        Expr::StringEndsWithValue { value, suffix } => {
            emit_string_predicate_expr(out, value, suffix, StringPredicate::EndsWith)
        }
        Expr::StringLen(name) => emit_string_len(out, name),
        Expr::StringLenValue(value) => emit_string_unary_expr(out, value, StringUnary::Len),
        Expr::StringIsEmpty(name) => emit_string_is_empty(out, name),
        Expr::StringIsEmptyValue(value) => emit_string_unary_expr(out, value, StringUnary::IsEmpty),
        Expr::StringSlice { name, start, end } => emit_string_slice(out, name, start, end),
        Expr::StringSliceValue { value, start, end } => {
            emit_string_slice_expr(out, value, start, end)
        }
        Expr::StringTrim(name) => emit_string_trim(out, name),
        Expr::StringTrimValue(value) => emit_string_trim_expr(out, value),
        Expr::StringTrimStart(name) => emit_string_trim_start(out, name),
        Expr::StringTrimStartValue(value) => {
            emit_string_transform_expr(out, value, StringTransform::TrimStart)
        }
        Expr::StringTrimEnd(name) => emit_string_trim_end(out, name),
        Expr::StringTrimEndValue(value) => {
            emit_string_transform_expr(out, value, StringTransform::TrimEnd)
        }
        Expr::StringToUpper(name) => {
            emit_string_case_transform(out, name, "[:lower:]", "[:upper:]")
        }
        Expr::StringToUpperValue(value) => {
            emit_string_transform_expr(out, value, StringTransform::ToUpper)
        }
        Expr::StringToLower(name) => {
            emit_string_case_transform(out, name, "[:upper:]", "[:lower:]")
        }
        Expr::StringToLowerValue(value) => {
            emit_string_transform_expr(out, value, StringTransform::ToLower)
        }
        Expr::StringRepeat { name, count } => emit_string_repeat(out, name, count),
        Expr::StringRepeatValue { value, count } => emit_string_repeat_expr(out, value, count),
        Expr::StringSplit { name, separator } => emit_string_split_value(out, name, separator),
        Expr::StringSplitValue { value, separator } => {
            emit_string_split_expr_value(out, value, separator)
        }
        Expr::StringReplace { name, from, to } => emit_string_replace(out, name, from, to),
        Expr::StringReplaceValue { value, from, to } => {
            emit_string_replace_expr(out, value, from, to)
        }
        Expr::PathBasename(name) => emit_path_basename(out, name),
        Expr::PathBasenameValue(value) => emit_path_method_expr(out, value, PathMethod::Basename),
        Expr::PathDirname(name) => emit_path_dirname(out, name),
        Expr::PathDirnameValue(value) => emit_path_method_expr(out, value, PathMethod::Dirname),
        Expr::PathStem(name) => emit_path_stem(out, name),
        Expr::PathStemValue(value) => emit_path_method_expr(out, value, PathMethod::Stem),
        Expr::PathExtname(name) => emit_path_extname(out, name),
        Expr::PathExtnameValue(value) => emit_path_method_expr(out, value, PathMethod::Extname),
        Expr::PathIsAbsolute(name) => emit_path_is_absolute(out, name),
        Expr::PathIsAbsoluteValue(value) => {
            emit_path_method_expr(out, value, PathMethod::IsAbsolute)
        }
        Expr::Env(_)
        | Expr::EnvDefault { .. }
        | Expr::ProcessEnv { .. }
        | Expr::FsIsFile { .. }
        | Expr::FsIsDir { .. }
        | Expr::FsSize { .. }
        | Expr::FsReadLines { .. }
        | Expr::FsList { .. }
        | Expr::FsWriteLines { .. }
        | Expr::FsAppendLines { .. }
        | Expr::CliParse
        | Expr::JsonParse { .. }
        | Expr::JsonStringify { .. }
        | Expr::JsonStringifyValue { .. }
        | Expr::IfElse { .. }
        | Expr::Match { .. }
        | Expr::Not(_)
        | Expr::BitNot(_)
        | Expr::AllowedCommand { .. }
        | Expr::Command { .. }
        | Expr::CommandResult { .. }
        | Expr::AsyncCommand(_)
        | Expr::Async(_)
        | Expr::Await(_)
        | Expr::Pipeline { .. }
        | Expr::TryPipeline { .. }
        | Expr::TryResult(_)
        | Expr::PipelineResult { .. }
        | Expr::HasCommand(_)
        | Expr::PathExists(_)
        | Expr::Array(_)
        | Expr::Range { .. }
        | Expr::Map(_)
        | Expr::Tuple(_)
        | Expr::Record(_)
        | Expr::RecordPattern(_)
        | Expr::ArrayPattern { .. }
        | Expr::AliasPattern { .. }
        | Expr::Binary { .. }
        | Expr::LetIn { .. } => emit_expr(out, arg),
        Expr::Do { .. } => unreachable!("do expressions are lowered before emission"),
        Expr::NamedArg { .. } => unreachable!("named arguments are lowered before emission"),
        Expr::Closure { name, captures } => emit_closure(out, name, captures),
        Expr::Lambda { .. } => unreachable!("lambdas are lowered before emission"),
    }
}
