use super::async_values::{
    emit_async_binding, emit_async_expr_binding, emit_await_binding, emit_await_dependencies,
};
use super::collections::{
    emit_array_bool_binding, emit_array_bool_value_binding, emit_array_drop_binding,
    emit_array_drop_value_binding, emit_array_expansion_binding, emit_array_filter_binding,
    emit_array_filter_value_binding, emit_array_find_binding, emit_array_find_value_binding,
    emit_array_flat_map_binding, emit_array_flat_map_value_binding, emit_array_fold_binding,
    emit_array_fold_value_binding, emit_array_map_binding, emit_array_map_value_binding,
    emit_array_reverse_binding, emit_array_reverse_value_binding, emit_array_slice_binding,
    emit_array_slice_value_binding, emit_array_sort_binding, emit_array_sort_value_binding,
    emit_array_take_binding, emit_array_take_value_binding, emit_array_unique_binding,
    emit_array_unique_value_binding, emit_map_keys_value_binding, emit_map_values_value_binding,
};
use super::commands::{
    emit_allowed_command, emit_allowed_command_result_binding, emit_cli_parse_binding,
    emit_process_args_binding,
};
use super::control_flow::{emit_range_binding, emit_try_result_binding};
use super::expressions::{emit_bound_expr, emit_call_arg, emit_discard_expr, emit_expr};
use super::filesystem::{
    emit_fs_list_binding, emit_fs_read_lines_binding, emit_path_method_value_binding, PathMethod,
};
use super::json::emit_json_parse_binding;
use super::matching::emit_command_result_binding;
use super::quoting::emit_shell_word;
use super::shell::{emit_pipeline, emit_shell_command};
use super::strings::{
    emit_call_output_array_binding, emit_string_predicate_value_binding,
    emit_string_repeat_value_binding, emit_string_replace_value_binding,
    emit_string_slice_value_binding, emit_string_split_binding, emit_string_split_value_binding,
    emit_string_transform_value_binding, emit_string_trim_value_binding,
    emit_string_unary_value_binding, StringPredicate, StringTransform, StringUnary,
};
use super::values::{
    constructor_record_fields, constructor_tuple_values, emit_array, emit_array_element,
    emit_constructor_record_binding, emit_constructor_tuple_binding, emit_index_expr, emit_map,
    emit_record_binding, emit_tuple_binding,
};
use super::AllowedCommandParts;
use crate::{AssignTarget, BindingPattern, Expr};

pub(super) fn emit_binding(out: &mut String, name: &str, expr: &Expr, readonly: bool, local: bool) {
    if name == "_" {
        emit_discard_expr(out, expr);
        return;
    }

    if let Some((tag, fields)) = constructor_record_fields(expr) {
        emit_constructor_record_binding(out, name, tag, fields, readonly, local);
        return;
    }
    if let Some((tag, values)) = constructor_tuple_values(expr) {
        emit_constructor_tuple_binding(out, name, tag, values, readonly, local);
        return;
    }

    match expr {
        Expr::Map(entries) => {
            if local {
                out.push_str("local ");
                out.push_str("-A ");
            } else if readonly {
                out.push_str("declare -Ar ");
            } else {
                out.push_str("declare -A ");
            }
            out.push_str(name);
            out.push('=');
            emit_map(out, entries);
            out.push('\n');
        }
        Expr::Record(fields) => emit_record_binding(out, name, fields, readonly, local),
        Expr::Tuple(values) => emit_tuple_binding(out, name, values, readonly, local),
        Expr::Array(values) => {
            if local {
                out.push_str("local ");
                out.push_str("-a ");
            } else if readonly {
                out.push_str("readonly -a ");
            }
            out.push_str(name);
            out.push('=');
            emit_array(out, values);
            out.push('\n');
        }
        Expr::Range {
            start,
            end,
            inclusive,
        } => emit_range_binding(out, name, start, end, *inclusive, readonly, local),
        Expr::Slice {
            name: source,
            start,
            end,
        } => emit_array_slice_binding(out, name, source, start, end, readonly, local),
        Expr::ArraySliceValue { value, start, end } => {
            emit_array_slice_value_binding(out, name, value, start, end, readonly, local)
        }
        Expr::ArrayTake {
            name: source,
            count,
        } => emit_array_take_binding(out, name, source, count, readonly, local),
        Expr::ArrayTakeValue { value, count } => {
            emit_array_take_value_binding(out, name, value, count, readonly, local)
        }
        Expr::ArrayDrop {
            name: source,
            count,
        } => emit_array_drop_binding(out, name, source, count, readonly, local),
        Expr::ArrayDropValue { value, count } => {
            emit_array_drop_value_binding(out, name, value, count, readonly, local)
        }
        Expr::ArrayReverse(source) => {
            emit_array_reverse_binding(out, name, source, readonly, local)
        }
        Expr::ArrayReverseValue(value) => {
            emit_array_reverse_value_binding(out, name, value, readonly, local)
        }
        Expr::ArraySort(source) => emit_array_sort_binding(out, name, source, readonly, local),
        Expr::ArraySortValue(value) => {
            emit_array_sort_value_binding(out, name, value, readonly, local)
        }
        Expr::ArrayUnique(source) => emit_array_unique_binding(out, name, source, readonly, local),
        Expr::ArrayUniqueValue(value) => {
            emit_array_unique_value_binding(out, name, value, readonly, local)
        }
        Expr::ArrayMap {
            name: source,
            mapper,
        } => emit_array_map_binding(out, name, source, mapper, readonly, local),
        Expr::ArrayMapValue { value, mapper } => {
            emit_array_map_value_binding(out, name, value, mapper, readonly, local)
        }
        Expr::ArrayFilter {
            name: source,
            predicate,
        } => emit_array_filter_binding(out, name, source, predicate, readonly, local),
        Expr::ArrayFilterValue { value, predicate } => {
            emit_array_filter_value_binding(out, name, value, predicate, readonly, local)
        }
        Expr::ArrayFlatMap {
            name: source,
            mapper,
        } => emit_array_flat_map_binding(out, name, source, mapper, readonly, local),
        Expr::ArrayFlatMapValue { value, mapper } => {
            emit_array_flat_map_value_binding(out, name, value, mapper, readonly, local)
        }
        Expr::ArrayFind {
            name: source,
            predicate,
        } => emit_array_find_binding(out, name, source, predicate, readonly, local),
        Expr::ArrayFindValue { value, predicate } => {
            emit_array_find_value_binding(out, name, value, predicate, readonly, local)
        }
        Expr::ArrayAny {
            name: source,
            predicate,
        } => emit_array_bool_binding(out, name, source, predicate, true, readonly, local),
        Expr::ArrayAnyValue { value, predicate } => {
            emit_array_bool_value_binding(out, name, value, predicate, true, readonly, local)
        }
        Expr::ArrayAll {
            name: source,
            predicate,
        } => emit_array_bool_binding(out, name, source, predicate, false, readonly, local),
        Expr::ArrayAllValue { value, predicate } => {
            emit_array_bool_value_binding(out, name, value, predicate, false, readonly, local)
        }
        Expr::ArrayFold {
            name: source,
            initial,
            reducer,
        } => emit_array_fold_binding(out, name, source, initial, reducer, readonly, local),
        Expr::ArrayFoldValue {
            value,
            initial,
            reducer,
        } => emit_array_fold_value_binding(out, name, value, initial, reducer, readonly, local),
        Expr::MapKeys(source) => {
            emit_array_expansion_binding(out, name, &format!("${{!{source}[@]}}"), readonly, local)
        }
        Expr::MapKeysValue(value) => emit_map_keys_value_binding(out, name, value, readonly, local),
        Expr::MapValues(source) => {
            emit_array_expansion_binding(out, name, &format!("${{{source}[@]}}"), readonly, local)
        }
        Expr::MapValuesValue(value) => {
            emit_map_values_value_binding(out, name, value, readonly, local)
        }
        Expr::FsReadLines { path } => emit_fs_read_lines_binding(out, name, path, readonly, local),
        Expr::FsList { path } => emit_fs_list_binding(out, name, path, readonly, local),
        Expr::JsonParse { value } => emit_json_parse_binding(out, name, value, readonly, local),
        Expr::StringContainsValue { value, needle } => emit_string_predicate_value_binding(
            out,
            name,
            value,
            needle,
            readonly,
            local,
            StringPredicate::Contains,
        ),
        Expr::StringIndexOfValue { value, needle } => emit_string_predicate_value_binding(
            out,
            name,
            value,
            needle,
            readonly,
            local,
            StringPredicate::IndexOf,
        ),
        Expr::StringStartsWithValue { value, prefix } => emit_string_predicate_value_binding(
            out,
            name,
            value,
            prefix,
            readonly,
            local,
            StringPredicate::StartsWith,
        ),
        Expr::StringEndsWithValue { value, suffix } => emit_string_predicate_value_binding(
            out,
            name,
            value,
            suffix,
            readonly,
            local,
            StringPredicate::EndsWith,
        ),
        Expr::StringLenValue(value) => {
            emit_string_unary_value_binding(out, name, value, readonly, local, StringUnary::Len)
        }
        Expr::StringIsEmptyValue(value) => {
            emit_string_unary_value_binding(out, name, value, readonly, local, StringUnary::IsEmpty)
        }
        Expr::StringSliceValue { value, start, end } => {
            emit_string_slice_value_binding(out, name, value, start, end, readonly, local)
        }
        Expr::StringRepeatValue { value, count } => {
            emit_string_repeat_value_binding(out, name, value, count, readonly, local)
        }
        Expr::StringTrimValue(value) => {
            emit_string_trim_value_binding(out, name, value, readonly, local)
        }
        Expr::StringTrimStartValue(value) => emit_string_transform_value_binding(
            out,
            name,
            value,
            readonly,
            local,
            StringTransform::TrimStart,
        ),
        Expr::StringTrimEndValue(value) => emit_string_transform_value_binding(
            out,
            name,
            value,
            readonly,
            local,
            StringTransform::TrimEnd,
        ),
        Expr::StringToUpperValue(value) => emit_string_transform_value_binding(
            out,
            name,
            value,
            readonly,
            local,
            StringTransform::ToUpper,
        ),
        Expr::StringToLowerValue(value) => emit_string_transform_value_binding(
            out,
            name,
            value,
            readonly,
            local,
            StringTransform::ToLower,
        ),
        Expr::StringSplit {
            name: source,
            separator,
        } => emit_string_split_binding(out, name, source, separator, readonly, local),
        Expr::StringSplitValue { value, separator } => {
            emit_string_split_value_binding(out, name, value, separator, readonly, local)
        }
        Expr::StringReplaceValue { value, from, to } => {
            emit_string_replace_value_binding(out, name, value, from, to, readonly, local)
        }
        Expr::PathBasenameValue(value) => {
            emit_path_method_value_binding(out, name, value, readonly, local, PathMethod::Basename)
        }
        Expr::PathDirnameValue(value) => {
            emit_path_method_value_binding(out, name, value, readonly, local, PathMethod::Dirname)
        }
        Expr::PathStemValue(value) => {
            emit_path_method_value_binding(out, name, value, readonly, local, PathMethod::Stem)
        }
        Expr::PathExtnameValue(value) => {
            emit_path_method_value_binding(out, name, value, readonly, local, PathMethod::Extname)
        }
        Expr::PathIsAbsoluteValue(value) => emit_path_method_value_binding(
            out,
            name,
            value,
            readonly,
            local,
            PathMethod::IsAbsolute,
        ),
        Expr::ProcessArgs => emit_process_args_binding(out, name, readonly, local),
        Expr::CliParse => emit_cli_parse_binding(out, name, readonly, local),
        Expr::Call { name: call, args } if call == "str.split" => {
            emit_call_output_array_binding(out, name, call, args, readonly, local)
        }
        Expr::AllowedCommand {
            program,
            args,
            result,
            read_args,
            write_args,
            ..
        } => {
            if *result {
                emit_allowed_command_result_binding(
                    out,
                    name,
                    AllowedCommandParts::new(program.as_deref(), args, read_args, write_args),
                    readonly,
                    local,
                );
            } else {
                if local {
                    out.push_str("local ");
                    out.push_str(name);
                    out.push('\n');
                }
                out.push_str(name);
                out.push('=');
                emit_allowed_command(out, program.as_deref(), args, read_args, write_args, true);
                out.push_str(" || exit $?\n");
                if readonly && !local {
                    out.push_str("readonly ");
                    out.push_str(name);
                    out.push('\n');
                }
            }
        }
        Expr::Command { command, checked } if *checked => {
            if local {
                out.push_str("local ");
                out.push_str(name);
                out.push('\n');
            }
            out.push_str(name);
            out.push_str("=\"$(");
            emit_shell_command(out, command);
            out.push_str(")\" || exit $?\n");
            if readonly && !local {
                out.push_str("readonly ");
                out.push_str(name);
                out.push('\n');
            }
        }
        Expr::TryPipeline { input, commands } => {
            if local {
                out.push_str("local ");
                out.push_str(name);
                out.push('\n');
            }
            out.push_str(name);
            out.push_str("=\"$(");
            emit_pipeline(out, input.as_deref(), commands);
            out.push_str(")\" || exit $?\n");
            if readonly && !local {
                out.push_str("readonly ");
                out.push_str(name);
                out.push('\n');
            }
        }
        Expr::TryResult(value) => emit_try_result_binding(out, name, value, readonly, local),
        Expr::CommandResult { command } => {
            emit_command_result_binding(out, name, command, readonly, local)
        }
        Expr::PipelineResult { input, commands } => {
            let mut command = String::new();
            emit_pipeline(&mut command, input.as_deref(), commands);
            emit_command_result_binding(out, name, &command, readonly, local);
        }
        Expr::AsyncCommand(command) => emit_async_binding(out, name, command, readonly, local),
        Expr::Async(value) => emit_async_expr_binding(out, name, value, readonly, local),
        Expr::Await(future) => emit_await_binding(out, name, future, readonly, local),
        _ => {
            emit_await_dependencies(out, expr);
            if local {
                out.push_str("local ");
            } else if readonly {
                out.push_str("readonly ");
            }
            out.push_str(name);
            out.push('=');
            emit_bound_expr(out, expr);
        }
    }
}

pub(super) fn emit_assignment_target(out: &mut String, target: &AssignTarget, expr: &Expr) {
    match target {
        AssignTarget::Name(name) => emit_assignment(out, name, expr),
        AssignTarget::Index { name, index } => emit_index_assignment(out, name, index, expr),
        AssignTarget::FieldIndex { name, field, index } => {
            emit_index_assignment(out, &format!("{name}_{field}"), index, expr)
        }
        AssignTarget::Field { name, field } => emit_field_assignment(out, name, field, expr),
        AssignTarget::TupleField { .. } => unreachable!("tuple field assignment is rejected"),
    }
}

pub(super) fn emit_index_assignment(out: &mut String, name: &str, index: &Expr, expr: &Expr) {
    out.push_str("if [[ \"$(declare -p ");
    out.push_str(name);
    out.push_str(" 2>/dev/null)\" == declare\\ -A* ]]; then ");
    out.push_str(name);
    out.push('[');
    emit_call_arg(out, index);
    out.push_str("]=");
    emit_expr(out, expr);
    out.push_str("; else ");
    out.push_str(name);
    out.push('[');
    emit_index_expr(out, index);
    out.push_str("]=");
    emit_array_element(out, expr);
    out.push_str("; fi\n");
}

pub(super) fn emit_field_assignment(out: &mut String, name: &str, field: &str, expr: &Expr) {
    emit_assignment(out, &format!("{name}_{field}"), expr);
}

pub(super) fn emit_assignment(out: &mut String, name: &str, expr: &Expr) {
    if name == "_" {
        emit_discard_expr(out, expr);
        return;
    }

    if let Some((tag, fields)) = constructor_record_fields(expr) {
        emit_constructor_record_binding(out, name, tag, fields, false, false);
        return;
    }
    if let Some((tag, values)) = constructor_tuple_values(expr) {
        emit_constructor_tuple_binding(out, name, tag, values, false, false);
        return;
    }

    match expr {
        Expr::Map(entries) => {
            out.push_str("declare -A ");
            out.push_str(name);
            out.push('=');
            emit_map(out, entries);
            out.push('\n');
        }
        Expr::Record(fields) => emit_record_binding(out, name, fields, false, false),
        Expr::Tuple(values) => emit_tuple_binding(out, name, values, false, false),
        Expr::Array(values) => {
            out.push_str(name);
            out.push('=');
            emit_array(out, values);
            out.push('\n');
        }
        Expr::Range {
            start,
            end,
            inclusive,
        } => emit_range_binding(out, name, start, end, *inclusive, false, false),
        Expr::Slice {
            name: source,
            start,
            end,
        } => emit_array_slice_binding(out, name, source, start, end, false, false),
        Expr::ArraySliceValue { value, start, end } => {
            emit_array_slice_value_binding(out, name, value, start, end, false, false)
        }
        Expr::ArrayTake {
            name: source,
            count,
        } => emit_array_take_binding(out, name, source, count, false, false),
        Expr::ArrayTakeValue { value, count } => {
            emit_array_take_value_binding(out, name, value, count, false, false)
        }
        Expr::ArrayDrop {
            name: source,
            count,
        } => emit_array_drop_binding(out, name, source, count, false, false),
        Expr::ArrayDropValue { value, count } => {
            emit_array_drop_value_binding(out, name, value, count, false, false)
        }
        Expr::ArrayReverse(source) => emit_array_reverse_binding(out, name, source, false, false),
        Expr::ArrayReverseValue(value) => {
            emit_array_reverse_value_binding(out, name, value, false, false)
        }
        Expr::ArraySort(source) => emit_array_sort_binding(out, name, source, false, false),
        Expr::ArraySortValue(value) => {
            emit_array_sort_value_binding(out, name, value, false, false)
        }
        Expr::ArrayUnique(source) => emit_array_unique_binding(out, name, source, false, false),
        Expr::ArrayUniqueValue(value) => {
            emit_array_unique_value_binding(out, name, value, false, false)
        }
        Expr::ArrayMap {
            name: source,
            mapper,
        } => emit_array_map_binding(out, name, source, mapper, false, false),
        Expr::ArrayMapValue { value, mapper } => {
            emit_array_map_value_binding(out, name, value, mapper, false, false)
        }
        Expr::MapKeys(source) => {
            emit_array_expansion_binding(out, name, &format!("${{!{source}[@]}}"), false, false)
        }
        Expr::MapKeysValue(value) => emit_map_keys_value_binding(out, name, value, false, false),
        Expr::MapValues(source) => {
            emit_array_expansion_binding(out, name, &format!("${{{source}[@]}}"), false, false)
        }
        Expr::MapValuesValue(value) => {
            emit_map_values_value_binding(out, name, value, false, false)
        }
        Expr::FsReadLines { path } => emit_fs_read_lines_binding(out, name, path, false, false),
        Expr::FsList { path } => emit_fs_list_binding(out, name, path, false, false),
        Expr::JsonParse { value } => emit_json_parse_binding(out, name, value, false, false),
        Expr::StringContainsValue { value, needle } => emit_string_predicate_value_binding(
            out,
            name,
            value,
            needle,
            false,
            false,
            StringPredicate::Contains,
        ),
        Expr::StringIndexOfValue { value, needle } => emit_string_predicate_value_binding(
            out,
            name,
            value,
            needle,
            false,
            false,
            StringPredicate::IndexOf,
        ),
        Expr::StringStartsWithValue { value, prefix } => emit_string_predicate_value_binding(
            out,
            name,
            value,
            prefix,
            false,
            false,
            StringPredicate::StartsWith,
        ),
        Expr::StringEndsWithValue { value, suffix } => emit_string_predicate_value_binding(
            out,
            name,
            value,
            suffix,
            false,
            false,
            StringPredicate::EndsWith,
        ),
        Expr::StringLenValue(value) => {
            emit_string_unary_value_binding(out, name, value, false, false, StringUnary::Len)
        }
        Expr::StringIsEmptyValue(value) => {
            emit_string_unary_value_binding(out, name, value, false, false, StringUnary::IsEmpty)
        }
        Expr::StringSliceValue { value, start, end } => {
            emit_string_slice_value_binding(out, name, value, start, end, false, false)
        }
        Expr::StringRepeatValue { value, count } => {
            emit_string_repeat_value_binding(out, name, value, count, false, false)
        }
        Expr::StringTrimValue(value) => {
            emit_string_trim_value_binding(out, name, value, false, false)
        }
        Expr::StringTrimStartValue(value) => emit_string_transform_value_binding(
            out,
            name,
            value,
            false,
            false,
            StringTransform::TrimStart,
        ),
        Expr::StringTrimEndValue(value) => emit_string_transform_value_binding(
            out,
            name,
            value,
            false,
            false,
            StringTransform::TrimEnd,
        ),
        Expr::StringToUpperValue(value) => emit_string_transform_value_binding(
            out,
            name,
            value,
            false,
            false,
            StringTransform::ToUpper,
        ),
        Expr::StringToLowerValue(value) => emit_string_transform_value_binding(
            out,
            name,
            value,
            false,
            false,
            StringTransform::ToLower,
        ),
        Expr::StringSplit {
            name: source,
            separator,
        } => emit_string_split_binding(out, name, source, separator, false, false),
        Expr::StringSplitValue { value, separator } => {
            emit_string_split_value_binding(out, name, value, separator, false, false)
        }
        Expr::StringReplaceValue { value, from, to } => {
            emit_string_replace_value_binding(out, name, value, from, to, false, false)
        }
        Expr::PathBasenameValue(value) => {
            emit_path_method_value_binding(out, name, value, false, false, PathMethod::Basename)
        }
        Expr::PathDirnameValue(value) => {
            emit_path_method_value_binding(out, name, value, false, false, PathMethod::Dirname)
        }
        Expr::PathStemValue(value) => {
            emit_path_method_value_binding(out, name, value, false, false, PathMethod::Stem)
        }
        Expr::PathExtnameValue(value) => {
            emit_path_method_value_binding(out, name, value, false, false, PathMethod::Extname)
        }
        Expr::PathIsAbsoluteValue(value) => {
            emit_path_method_value_binding(out, name, value, false, false, PathMethod::IsAbsolute)
        }
        Expr::ProcessArgs => emit_process_args_binding(out, name, false, false),
        Expr::CliParse => emit_cli_parse_binding(out, name, false, false),
        Expr::Call { name: call, args } if call == "str.split" => {
            emit_call_output_array_binding(out, name, call, args, false, false)
        }
        Expr::AllowedCommand {
            program,
            args,
            result,
            read_args,
            write_args,
            ..
        } if *result => emit_allowed_command_result_binding(
            out,
            name,
            AllowedCommandParts::new(program.as_deref(), args, read_args, write_args),
            false,
            false,
        ),
        Expr::AsyncCommand(command) => emit_async_binding(out, name, command, false, false),
        Expr::Await(future) => emit_await_binding(out, name, future, false, false),
        Expr::CommandResult { command } => {
            emit_command_result_binding(out, name, command, false, false)
        }
        Expr::PipelineResult { input, commands } => {
            let mut command = String::new();
            emit_pipeline(&mut command, input.as_deref(), commands);
            emit_command_result_binding(out, name, &command, false, false);
        }
        _ => {
            emit_await_dependencies(out, expr);
            out.push_str(name);
            out.push('=');
            emit_bound_expr(out, expr);
        }
    }
}

pub(super) fn emit_destructure(
    out: &mut String,
    pattern: &BindingPattern,
    expr: &Expr,
    readonly: bool,
    local: bool,
) {
    match pattern {
        BindingPattern::Name(name) => emit_binding(out, name, expr, readonly, local),
        BindingPattern::Array { patterns, rest } => {
            for (index, pattern) in patterns.iter().enumerate() {
                let value = destructured_array_value(expr, index);
                emit_destructure(out, pattern, &value, readonly, local);
            }
            if let Some(rest) = rest {
                emit_array_rest_binding(out, rest, expr, patterns.len(), readonly, local);
            }
        }
        BindingPattern::Tuple(patterns) => {
            for (index, pattern) in patterns.iter().enumerate() {
                let value = destructured_tuple_value(expr, index + 1);
                emit_destructure(out, pattern, &value, readonly, local);
            }
        }
        BindingPattern::Record(bindings) => {
            for (field, pattern) in bindings {
                let value = destructured_record_value(expr, field);
                emit_destructure(out, pattern, &value, readonly, local);
            }
        }
    }
}

pub(super) fn destructured_array_value(expr: &Expr, index: usize) -> Expr {
    match expr {
        Expr::Array(values) => values.get(index).cloned().unwrap_or(Expr::Unit),
        Expr::Ident(name) if is_packed_destructure_source(name) => Expr::IndexValue {
            value: Box::new(Expr::Ident(name.clone())),
            index: Box::new(Expr::Int(index as i64)),
        },
        Expr::Ident(name) => Expr::Index {
            name: name.clone(),
            index: Box::new(Expr::Int(index as i64)),
        },
        Expr::Field { name, field } => Expr::Index {
            name: format!("{name}_{field}"),
            index: Box::new(Expr::Int(index as i64)),
        },
        Expr::TupleField { name, field } => Expr::Index {
            name: format!("{name}_{field}"),
            index: Box::new(Expr::Int(index as i64)),
        },
        Expr::Slice { name, start, .. } => Expr::Index {
            name: name.clone(),
            index: Box::new(Expr::Binary {
                left: start.clone(),
                op: crate::BinaryOp::Add,
                right: Box::new(Expr::Int(index as i64)),
            }),
        },
        _ => Expr::IndexValue {
            value: Box::new(expr.clone()),
            index: Box::new(Expr::Int(index as i64)),
        },
    }
}

pub(super) fn emit_array_rest_binding(
    out: &mut String,
    name: &str,
    expr: &Expr,
    start: usize,
    readonly: bool,
    local: bool,
) {
    if name == "_" {
        return;
    }
    if let Expr::Array(values) = expr {
        let rest = values.iter().skip(start).cloned().collect::<Vec<_>>();
        emit_binding(out, name, &Expr::Array(rest), readonly, local);
        return;
    }
    if let Expr::Slice {
        name: source,
        start: slice_start,
        end,
    } = expr
    {
        emit_array_slice_binding(
            out,
            name,
            source,
            &Expr::Binary {
                left: slice_start.clone(),
                op: crate::BinaryOp::Add,
                right: Box::new(Expr::Int(start as i64)),
            },
            end,
            readonly,
            local,
        );
        return;
    }
    if let Expr::Field {
        name: source,
        field: source_field,
    } = expr
    {
        let source = format!("{source}_{source_field}");
        let end = Expr::Len(source.clone());
        emit_array_slice_binding(
            out,
            name,
            &source,
            &Expr::Int(start as i64),
            &end,
            readonly,
            local,
        );
        return;
    }
    if let Expr::TupleField {
        name: source,
        field: source_field,
    } = expr
    {
        let source = format!("{source}_{source_field}");
        let end = Expr::Len(source.clone());
        emit_array_slice_binding(
            out,
            name,
            &source,
            &Expr::Int(start as i64),
            &end,
            readonly,
            local,
        );
        return;
    }

    if !matches!(expr, Expr::Ident(_) | Expr::Slice { .. })
        || matches!(expr, Expr::Ident(name) if is_packed_destructure_source(name))
    {
        emit_array_rest_value_binding(out, name, expr, start, readonly, local);
        return;
    }

    if local {
        out.push_str("local -a ");
    } else if readonly {
        out.push_str("readonly -a ");
    }
    out.push_str(name);
    out.push_str("=(\"${");
    out.push_str(&destructure_source_name(expr));
    out.push_str("[@]:");
    out.push_str(&start.to_string());
    out.push_str("}\")\n");
}

pub(super) fn emit_array_rest_value_binding(
    out: &mut String,
    name: &str,
    expr: &Expr,
    start: usize,
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local -a ");
        out.push_str(name);
        out.push('\n');
    }
    out.push_str("eval \"$(__nacre_array_rest_decl ");
    emit_expr(out, expr);
    out.push(' ');
    emit_shell_word(out, name);
    out.push(' ');
    out.push_str(&start.to_string());
    out.push_str(")\"\n");
    if readonly && !local {
        out.push_str("readonly -a ");
        out.push_str(name);
        out.push('\n');
    }
}

pub(super) fn destructured_tuple_value(expr: &Expr, field: usize) -> Expr {
    match expr {
        Expr::Tuple(values) => values[field - 1].clone(),
        Expr::Ident(name) if is_packed_destructure_source(name) => Expr::TupleFieldValue {
            value: Box::new(Expr::Ident(name.clone())),
            field,
        },
        Expr::Ident(name) => Expr::TupleField {
            name: name.clone(),
            field,
        },
        Expr::Field {
            name,
            field: source_field,
        } => Expr::TupleField {
            name: format!("{name}_{source_field}"),
            field,
        },
        Expr::TupleField {
            name,
            field: source_field,
        } => Expr::TupleField {
            name: format!("{name}_{source_field}"),
            field,
        },
        _ => Expr::TupleFieldValue {
            value: Box::new(expr.clone()),
            field,
        },
    }
}

pub(super) fn destructured_record_value(expr: &Expr, field: &str) -> Expr {
    match expr {
        Expr::Record(fields) => fields
            .iter()
            .find(|(name, _)| name == field)
            .map(|(_, value)| value.clone())
            .unwrap_or_else(|| Expr::Unit),
        Expr::Ident(name) if is_packed_destructure_source(name) => Expr::FieldValue {
            value: Box::new(Expr::Ident(name.clone())),
            field: field.to_string(),
        },
        Expr::Ident(name) => Expr::Field {
            name: name.clone(),
            field: field.to_string(),
        },
        Expr::Field {
            name,
            field: source_field,
        } => Expr::Field {
            name: format!("{name}_{source_field}"),
            field: field.to_string(),
        },
        Expr::TupleField {
            name,
            field: source_field,
        } => Expr::Field {
            name: format!("{name}_{source_field}"),
            field: field.to_string(),
        },
        _ => Expr::FieldValue {
            value: Box::new(expr.clone()),
            field: field.to_string(),
        },
    }
}

pub(super) fn is_packed_destructure_source(name: &str) -> bool {
    name == "__nacre_for_item" || name.starts_with("__nacre_for_array_field_")
}

pub(super) fn destructure_source_name(expr: &Expr) -> String {
    match expr {
        Expr::ProcessArgs => "args".to_string(),
        Expr::Ident(name)
        | Expr::Value(name)
        | Expr::Len(name)
        | Expr::IsEmpty(name)
        | Expr::ArrayFirst(name)
        | Expr::ArrayLast(name)
        | Expr::ArrayReverse(name)
        | Expr::ArraySort(name)
        | Expr::ArrayUnique(name)
        | Expr::ArrayMap { name, .. }
        | Expr::ArrayFilter { name, .. }
        | Expr::ArrayFlatMap { name, .. }
        | Expr::ArrayFind { name, .. }
        | Expr::ArrayAny { name, .. }
        | Expr::ArrayAll { name, .. }
        | Expr::ArrayFold { name, .. }
        | Expr::ArrayTake { name, .. }
        | Expr::ArrayDrop { name, .. }
        | Expr::Join { name, .. }
        | Expr::ArrayPush { name, .. }
        | Expr::ArrayPop { name }
        | Expr::MapSet { name, .. }
        | Expr::MapRemove { name, .. }
        | Expr::ArrayContains { name, .. }
        | Expr::ArrayIndexOf { name, .. }
        | Expr::Slice { name, .. }
        | Expr::MapKeys(name)
        | Expr::MapValues(name)
        | Expr::MapHas { name, .. }
        | Expr::StringContains { name, .. }
        | Expr::StringIndexOf { name, .. }
        | Expr::StringStartsWith { name, .. }
        | Expr::StringEndsWith { name, .. }
        | Expr::StringLen(name)
        | Expr::StringIsEmpty(name)
        | Expr::StringSlice { name, .. }
        | Expr::StringTrim(name)
        | Expr::StringTrimStart(name)
        | Expr::StringTrimEnd(name)
        | Expr::StringToUpper(name)
        | Expr::StringToLower(name)
        | Expr::StringRepeat { name, .. }
        | Expr::StringSplit { name, .. }
        | Expr::StringReplace { name, .. }
        | Expr::PathBasename(name)
        | Expr::PathDirname(name)
        | Expr::PathStem(name)
        | Expr::PathExtname(name)
        | Expr::PathIsAbsolute(name)
        | Expr::Await(name) => name.clone(),
        _ => String::new(),
    }
}
