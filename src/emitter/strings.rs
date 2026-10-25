use super::collections::{emit_arithmetic_expansion, emit_slice_length};
use super::expressions::{emit_call_arg, emit_call_command, emit_expr};
use super::quoting::emit_shell_word;
use super::shell::{emit_pipeline, emit_shell_command};
use crate::Expr;

pub(super) fn emit_string_contains(out: &mut String, name: &str, needle: &Expr) {
    out.push_str("$(if [[ \"$");
    out.push_str(name);
    out.push_str("\" == *");
    emit_expr(out, needle);
    out.push_str("* ]]; then printf true; else printf false; fi)");
}

#[derive(Clone, Copy)]
pub(super) enum StringPredicate {
    Contains,
    IndexOf,
    StartsWith,
    EndsWith,
}

pub(super) fn emit_string_predicate_value_binding(
    out: &mut String,
    binding: &str,
    value: &Expr,
    arg: &Expr,
    readonly: bool,
    local: bool,
    predicate: StringPredicate,
) {
    if emit_checked_string_predicate_value(out, binding, value, local) {
        out.push_str(binding);
        out.push('=');
        emit_string_predicate_name(out, "__nacre_string_value", arg, predicate);
        out.push('\n');
        if readonly && !local {
            out.push_str("readonly ");
            out.push_str(binding);
            out.push('\n');
        }
        return;
    }
    if local {
        out.push_str("local ");
    } else if readonly {
        out.push_str("readonly ");
    }
    out.push_str(binding);
    out.push('=');
    emit_string_predicate_expr(out, value, arg, predicate);
    out.push('\n');
}

pub(super) fn emit_checked_string_predicate_value(
    out: &mut String,
    binding: &str,
    value: &Expr,
    local: bool,
) -> bool {
    match value {
        Expr::Command {
            command,
            checked: true,
        } => {
            if local {
                out.push_str("local ");
                out.push_str(binding);
                out.push_str(" __nacre_string_value\n");
            }
            out.push_str("__nacre_string_value=\"$(");
            emit_shell_command(out, command);
            out.push_str(")\" || exit $?\n");
            true
        }
        Expr::TryPipeline { input, commands } => {
            if local {
                out.push_str("local ");
                out.push_str(binding);
                out.push_str(" __nacre_string_value\n");
            }
            out.push_str("__nacre_string_value=\"$(");
            emit_pipeline(out, input.as_deref(), commands);
            out.push_str(")\" || exit $?\n");
            true
        }
        _ => false,
    }
}

pub(super) fn emit_string_predicate_name(
    out: &mut String,
    name: &str,
    arg: &Expr,
    predicate: StringPredicate,
) {
    match predicate {
        StringPredicate::Contains => emit_string_contains(out, name, arg),
        StringPredicate::IndexOf => emit_string_index_of(out, name, arg),
        StringPredicate::StartsWith => emit_string_starts_with(out, name, arg),
        StringPredicate::EndsWith => emit_string_ends_with(out, name, arg),
    }
}

pub(super) fn emit_string_predicate_expr(
    out: &mut String,
    value: &Expr,
    arg: &Expr,
    predicate: StringPredicate,
) {
    match predicate {
        StringPredicate::Contains => emit_string_contains_expr(out, value, arg),
        StringPredicate::IndexOf => emit_string_index_of_expr(out, value, arg),
        StringPredicate::StartsWith => emit_string_starts_with_expr(out, value, arg),
        StringPredicate::EndsWith => emit_string_ends_with_expr(out, value, arg),
    }
}

pub(super) fn emit_string_contains_expr(out: &mut String, value: &Expr, needle: &Expr) {
    out.push_str("$(if [[ ");
    emit_call_arg(out, value);
    out.push_str(" == *");
    emit_expr(out, needle);
    out.push_str("* ]]; then printf true; else printf false; fi)");
}

pub(super) fn emit_string_index_of(out: &mut String, name: &str, needle: &Expr) {
    out.push_str("$(awk -v __nacre_haystack=\"$");
    out.push_str(name);
    out.push_str("\" -v __nacre_needle=");
    emit_call_arg(out, needle);
    out.push_str(" 'BEGIN { __nacre_index = index(__nacre_haystack, __nacre_needle); printf \"%s\", (__nacre_index ? __nacre_index - 1 : -1) }')");
}

pub(super) fn emit_string_index_of_expr(out: &mut String, value: &Expr, needle: &Expr) {
    out.push_str("$(awk -v __nacre_haystack=");
    emit_call_arg(out, value);
    out.push_str(" -v __nacre_needle=");
    emit_call_arg(out, needle);
    out.push_str(" 'BEGIN { __nacre_index = index(__nacre_haystack, __nacre_needle); printf \"%s\", (__nacre_index ? __nacre_index - 1 : -1) }')");
}

pub(super) fn emit_string_starts_with(out: &mut String, name: &str, prefix: &Expr) {
    out.push_str("$(if [[ \"$");
    out.push_str(name);
    out.push_str("\" == ");
    emit_expr(out, prefix);
    out.push_str("* ]]; then printf true; else printf false; fi)");
}

pub(super) fn emit_string_starts_with_expr(out: &mut String, value: &Expr, prefix: &Expr) {
    out.push_str("$(if [[ ");
    emit_call_arg(out, value);
    out.push_str(" == ");
    emit_expr(out, prefix);
    out.push_str("* ]]; then printf true; else printf false; fi)");
}

pub(super) fn emit_string_ends_with(out: &mut String, name: &str, suffix: &Expr) {
    out.push_str("$(if [[ \"$");
    out.push_str(name);
    out.push_str("\" == *");
    emit_expr(out, suffix);
    out.push_str(" ]]; then printf true; else printf false; fi)");
}

pub(super) fn emit_string_ends_with_expr(out: &mut String, value: &Expr, suffix: &Expr) {
    out.push_str("$(if [[ ");
    emit_call_arg(out, value);
    out.push_str(" == *");
    emit_expr(out, suffix);
    out.push_str(" ]]; then printf true; else printf false; fi)");
}

pub(super) fn emit_string_len(out: &mut String, name: &str) {
    out.push_str("\"${#");
    out.push_str(name);
    out.push_str("}\"");
}

#[derive(Clone, Copy)]
pub(super) enum StringUnary {
    Len,
    IsEmpty,
}

pub(super) fn emit_string_unary_value_binding(
    out: &mut String,
    binding: &str,
    value: &Expr,
    readonly: bool,
    local: bool,
    op: StringUnary,
) {
    if emit_checked_string_predicate_value(out, binding, value, local) {
        out.push_str(binding);
        out.push('=');
        emit_string_unary_name(out, "__nacre_string_value", op);
        out.push('\n');
        if readonly && !local {
            out.push_str("readonly ");
            out.push_str(binding);
            out.push('\n');
        }
        return;
    }
    if local {
        out.push_str("local ");
    } else if readonly {
        out.push_str("readonly ");
    }
    out.push_str(binding);
    out.push('=');
    emit_string_unary_expr(out, value, op);
    out.push('\n');
}

pub(super) fn emit_string_unary_name(out: &mut String, name: &str, op: StringUnary) {
    match op {
        StringUnary::Len => emit_string_len(out, name),
        StringUnary::IsEmpty => emit_string_is_empty(out, name),
    }
}

pub(super) fn emit_string_unary_expr(out: &mut String, value: &Expr, op: StringUnary) {
    match op {
        StringUnary::Len => emit_string_len_expr(out, value),
        StringUnary::IsEmpty => emit_string_is_empty_expr(out, value),
    }
}

pub(super) fn emit_string_len_expr(out: &mut String, value: &Expr) {
    out.push_str("$(__nacre_string_value=");
    emit_call_arg(out, value);
    out.push_str("; printf '%s' \"${#__nacre_string_value}\")");
}

pub(super) fn emit_string_is_empty(out: &mut String, name: &str) {
    out.push_str("$(if [ -z \"$");
    out.push_str(name);
    out.push_str("\" ]; then printf true; else printf false; fi)");
}

pub(super) fn emit_string_is_empty_expr(out: &mut String, value: &Expr) {
    out.push_str("$(if [ -z ");
    emit_call_arg(out, value);
    out.push_str(" ]; then printf true; else printf false; fi)");
}

pub(super) fn emit_string_slice(out: &mut String, name: &str, start: &Expr, end: &Expr) {
    out.push_str("\"${");
    out.push_str(name);
    out.push(':');
    emit_arithmetic_expansion(out, start);
    out.push(':');
    emit_slice_length(out, start, end);
    out.push_str("}\"");
}

pub(super) fn emit_string_slice_value_binding(
    out: &mut String,
    binding: &str,
    value: &Expr,
    start: &Expr,
    end: &Expr,
    readonly: bool,
    local: bool,
) {
    if emit_checked_string_predicate_value(out, binding, value, local) {
        out.push_str(binding);
        out.push('=');
        emit_string_slice(out, "__nacre_string_value", start, end);
        out.push('\n');
        if readonly && !local {
            out.push_str("readonly ");
            out.push_str(binding);
            out.push('\n');
        }
        return;
    }
    if local {
        out.push_str("local ");
    } else if readonly {
        out.push_str("readonly ");
    }
    out.push_str(binding);
    out.push('=');
    emit_string_slice_expr(out, value, start, end);
    out.push('\n');
}

pub(super) fn emit_string_slice_expr(out: &mut String, value: &Expr, start: &Expr, end: &Expr) {
    out.push_str("\"$(__nacre_slice_value=");
    emit_call_arg(out, value);
    out.push_str("; printf '%s' \"${__nacre_slice_value:");
    emit_arithmetic_expansion(out, start);
    out.push(':');
    emit_slice_length(out, start, end);
    out.push_str("}\")\"");
}

#[derive(Clone, Copy)]
pub(super) enum StringTransform {
    Trim,
    TrimStart,
    TrimEnd,
    ToUpper,
    ToLower,
}

pub(super) fn string_transform_temp_name(transform: StringTransform) -> &'static str {
    match transform {
        StringTransform::Trim => "__nacre_trim_value",
        StringTransform::TrimStart => "__nacre_trim_start_value",
        StringTransform::TrimEnd => "__nacre_trim_end_value",
        StringTransform::ToUpper => "__nacre_upper_value",
        StringTransform::ToLower => "__nacre_lower_value",
    }
}

pub(super) fn emit_string_transform_name(out: &mut String, name: &str, transform: StringTransform) {
    match transform {
        StringTransform::Trim => emit_string_trim(out, name),
        StringTransform::TrimStart => emit_string_trim_start(out, name),
        StringTransform::TrimEnd => emit_string_trim_end(out, name),
        StringTransform::ToUpper => emit_string_case_transform(out, name, "[:lower:]", "[:upper:]"),
        StringTransform::ToLower => emit_string_case_transform(out, name, "[:upper:]", "[:lower:]"),
    }
}

pub(super) fn emit_string_transform_expr(
    out: &mut String,
    value: &Expr,
    transform: StringTransform,
) {
    match transform {
        StringTransform::Trim => emit_string_trim_expr(out, value),
        StringTransform::TrimStart => emit_string_trim_start_expr(out, value),
        StringTransform::TrimEnd => emit_string_trim_end_expr(out, value),
        StringTransform::ToUpper => {
            emit_string_case_transform_expr(out, value, "[:lower:]", "[:upper:]")
        }
        StringTransform::ToLower => {
            emit_string_case_transform_expr(out, value, "[:upper:]", "[:lower:]")
        }
    }
}

pub(super) fn emit_string_trim(out: &mut String, name: &str) {
    out.push_str("\"$(printf '%s' \"$");
    out.push_str(name);
    out.push_str("\" | awk ");
    emit_shell_word(
        out,
        r#"{gsub(/^[[:space:]]+|[[:space:]]+$/, ""); printf "%s", $0}"#,
    );
    out.push_str(")\"");
}

pub(super) fn emit_string_trim_expr(out: &mut String, value: &Expr) {
    out.push_str("\"$(printf '%s' ");
    emit_call_arg(out, value);
    out.push_str(" | awk ");
    emit_shell_word(
        out,
        r#"{gsub(/^[[:space:]]+|[[:space:]]+$/, ""); printf "%s", $0}"#,
    );
    out.push_str(")\"");
}

pub(super) fn emit_string_trim_start_expr(out: &mut String, value: &Expr) {
    out.push_str("\"$(printf '%s' ");
    emit_call_arg(out, value);
    out.push_str(" | awk ");
    emit_shell_word(out, r#"{gsub(/^[[:space:]]+/, ""); printf "%s", $0}"#);
    out.push_str(")\"");
}

pub(super) fn emit_string_trim_start(out: &mut String, name: &str) {
    out.push_str("\"$(printf '%s' \"$");
    out.push_str(name);
    out.push_str("\" | awk ");
    emit_shell_word(out, r#"{gsub(/^[[:space:]]+/, ""); printf "%s", $0}"#);
    out.push_str(")\"");
}

pub(super) fn emit_string_trim_end_expr(out: &mut String, value: &Expr) {
    out.push_str("\"$(printf '%s' ");
    emit_call_arg(out, value);
    out.push_str(" | awk ");
    emit_shell_word(out, r#"{gsub(/[[:space:]]+$/, ""); printf "%s", $0}"#);
    out.push_str(")\"");
}

pub(super) fn emit_string_trim_end(out: &mut String, name: &str) {
    out.push_str("\"$(printf '%s' \"$");
    out.push_str(name);
    out.push_str("\" | awk ");
    emit_shell_word(out, r#"{gsub(/[[:space:]]+$/, ""); printf "%s", $0}"#);
    out.push_str(")\"");
}

pub(super) fn emit_string_case_transform(out: &mut String, name: &str, from: &str, to: &str) {
    out.push_str("\"$(printf '%s' \"$");
    out.push_str(name);
    out.push_str("\" | tr ");
    emit_shell_word(out, from);
    out.push(' ');
    emit_shell_word(out, to);
    out.push_str(")\"");
}

pub(super) fn emit_string_case_transform_expr(
    out: &mut String,
    value: &Expr,
    from: &str,
    to: &str,
) {
    out.push_str("\"$(printf '%s' ");
    emit_call_arg(out, value);
    out.push_str(" | tr ");
    emit_shell_word(out, from);
    out.push(' ');
    emit_shell_word(out, to);
    out.push_str(")\"");
}

pub(super) fn emit_string_repeat(out: &mut String, name: &str, count: &Expr) {
    out.push_str("\"$(for ((__nacre_i=0; __nacre_i<");
    emit_arithmetic_expansion(out, count);
    out.push_str("; __nacre_i++)); do printf '%s' \"$");
    out.push_str(name);
    out.push_str("\"; done)\"");
}

pub(super) fn emit_string_repeat_value_binding(
    out: &mut String,
    binding: &str,
    value: &Expr,
    count: &Expr,
    readonly: bool,
    local: bool,
) {
    if emit_checked_string_predicate_value(out, binding, value, local) {
        out.push_str(binding);
        out.push('=');
        emit_string_repeat(out, "__nacre_string_value", count);
        out.push('\n');
        if readonly && !local {
            out.push_str("readonly ");
            out.push_str(binding);
            out.push('\n');
        }
        return;
    }
    if local {
        out.push_str("local ");
    } else if readonly {
        out.push_str("readonly ");
    }
    out.push_str(binding);
    out.push('=');
    emit_string_repeat_expr(out, value, count);
    out.push('\n');
}

pub(super) fn emit_string_repeat_expr(out: &mut String, value: &Expr, count: &Expr) {
    out.push_str("\"$(__nacre_repeat_value=");
    emit_call_arg(out, value);
    out.push_str("; for ((__nacre_i=0; __nacre_i<");
    emit_arithmetic_expansion(out, count);
    out.push_str("; __nacre_i++)); do printf '%s' \"$__nacre_repeat_value\"; done)\"");
}

pub(super) fn emit_string_split_binding(
    out: &mut String,
    binding: &str,
    source: &str,
    separator: &Expr,
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local -a ");
        out.push_str(binding);
        out.push('\n');
    }
    out.push_str("mapfile -t ");
    out.push_str(binding);
    out.push_str(" < <(");
    emit_string_split_command(out, source, separator);
    out.push_str(")\n");
    if readonly && !local {
        out.push_str("readonly -a ");
        out.push_str(binding);
        out.push('\n');
    }
}

pub(super) fn emit_string_split_value_binding(
    out: &mut String,
    binding: &str,
    value: &Expr,
    separator: &Expr,
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local -a ");
        out.push_str(binding);
        out.push('\n');
    }
    if emit_checked_string_split_value(out, value, separator, local) {
        out.push_str("mapfile -t ");
        out.push_str(binding);
        out.push_str(" < <(");
        emit_string_split_command(out, "__nacre_split_value", separator);
        out.push_str(")\n");
        if readonly && !local {
            out.push_str("readonly -a ");
            out.push_str(binding);
            out.push('\n');
        }
        return;
    }
    out.push_str("mapfile -t ");
    out.push_str(binding);
    out.push_str(" < <(");
    emit_string_split_expr_command(out, value, separator);
    out.push_str(")\n");
    if readonly && !local {
        out.push_str("readonly -a ");
        out.push_str(binding);
        out.push('\n');
    }
}

pub(super) fn emit_string_trim_value_binding(
    out: &mut String,
    binding: &str,
    value: &Expr,
    readonly: bool,
    local: bool,
) {
    emit_string_transform_value_binding(
        out,
        binding,
        value,
        readonly,
        local,
        StringTransform::Trim,
    );
}

pub(super) fn emit_string_transform_value_binding(
    out: &mut String,
    binding: &str,
    value: &Expr,
    readonly: bool,
    local: bool,
    transform: StringTransform,
) {
    if emit_checked_string_transform_value(out, binding, value, local, transform) {
        out.push_str(binding);
        out.push('=');
        emit_string_transform_name(out, string_transform_temp_name(transform), transform);
        out.push('\n');
        if readonly && !local {
            out.push_str("readonly ");
            out.push_str(binding);
            out.push('\n');
        }
        return;
    }
    if local {
        out.push_str("local ");
    } else if readonly {
        out.push_str("readonly ");
    }
    out.push_str(binding);
    out.push('=');
    emit_string_transform_expr(out, value, transform);
    out.push('\n');
}

pub(super) fn emit_checked_string_transform_value(
    out: &mut String,
    binding: &str,
    value: &Expr,
    local: bool,
    transform: StringTransform,
) -> bool {
    let temp_name = string_transform_temp_name(transform);
    match value {
        Expr::Command {
            command,
            checked: true,
        } => {
            if local {
                out.push_str("local ");
                out.push_str(binding);
                out.push(' ');
                out.push_str(temp_name);
                out.push('\n');
            }
            out.push_str(temp_name);
            out.push_str("=\"$(");
            emit_shell_command(out, command);
            out.push_str(")\" || exit $?\n");
            true
        }
        Expr::TryPipeline { input, commands } => {
            if local {
                out.push_str("local ");
                out.push_str(binding);
                out.push(' ');
                out.push_str(temp_name);
                out.push('\n');
            }
            out.push_str(temp_name);
            out.push_str("=\"$(");
            emit_pipeline(out, input.as_deref(), commands);
            out.push_str(")\" || exit $?\n");
            true
        }
        _ => false,
    }
}

pub(super) fn emit_checked_string_split_value(
    out: &mut String,
    value: &Expr,
    _separator: &Expr,
    local: bool,
) -> bool {
    match value {
        Expr::Command {
            command,
            checked: true,
        } => {
            if local {
                out.push_str("local __nacre_split_value\n");
            }
            out.push_str("__nacre_split_value=\"$(");
            emit_shell_command(out, command);
            out.push_str(")\" || exit $?\n");
            true
        }
        Expr::TryPipeline { input, commands } => {
            if local {
                out.push_str("local __nacre_split_value\n");
            }
            out.push_str("__nacre_split_value=\"$(");
            emit_pipeline(out, input.as_deref(), commands);
            out.push_str(")\" || exit $?\n");
            true
        }
        _ => false,
    }
}

pub(super) fn emit_call_output_array_binding(
    out: &mut String,
    binding: &str,
    name: &str,
    args: &[Expr],
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local -a ");
        out.push_str(binding);
        out.push('\n');
    }
    out.push_str("mapfile -t ");
    out.push_str(binding);
    out.push_str(" < <(");
    emit_call_command(out, name, args);
    out.push_str(")\n");
    if readonly && !local {
        out.push_str("readonly -a ");
        out.push_str(binding);
        out.push('\n');
    }
}

pub(super) fn emit_string_split_value(out: &mut String, name: &str, separator: &Expr) {
    out.push_str("\"$(");
    emit_string_split_command(out, name, separator);
    out.push_str(")\"");
}

pub(super) fn emit_string_split_expr_value(out: &mut String, value: &Expr, separator: &Expr) {
    out.push_str("\"$(");
    emit_string_split_expr_command(out, value, separator);
    out.push_str(")\"");
}

pub(super) fn emit_string_split_command(out: &mut String, name: &str, separator: &Expr) {
    if is_newline_separator(separator) {
        out.push_str("printf '%s\\n' \"$");
        out.push_str(name);
        out.push('"');
        return;
    }
    out.push_str("awk -v __nacre_value=\"$");
    out.push_str(name);
    out.push_str("\" -v __nacre_sep=");
    emit_expr(out, separator);
    out.push(' ');
    emit_shell_word(out, STRING_SPLIT_AWK);
}

pub(super) fn emit_string_split_expr_command(out: &mut String, value: &Expr, separator: &Expr) {
    if is_newline_separator(separator) {
        out.push_str("printf '%s\\n' ");
        emit_call_arg(out, value);
        return;
    }
    out.push_str("awk -v __nacre_value=");
    emit_call_arg(out, value);
    out.push_str(" -v __nacre_sep=");
    emit_expr(out, separator);
    out.push(' ');
    emit_shell_word(out, STRING_SPLIT_AWK);
}

pub(super) fn is_newline_separator(separator: &Expr) -> bool {
    matches!(separator, Expr::String(value) | Expr::RawString(value) if value == "\n")
}

const STRING_SPLIT_AWK: &str = r#"BEGIN {
  if (__nacre_sep == "") {
    print __nacre_value
    exit
  }
  while ((idx = index(__nacre_value, __nacre_sep)) > 0) {
    print substr(__nacre_value, 1, idx - 1)
    __nacre_value = substr(__nacre_value, idx + length(__nacre_sep))
  }
  print __nacre_value
}"#;

pub(super) fn emit_string_replace_value_binding(
    out: &mut String,
    binding: &str,
    value: &Expr,
    from: &Expr,
    to: &Expr,
    readonly: bool,
    local: bool,
) {
    if emit_checked_string_replace_value(out, binding, value, local) {
        out.push_str(binding);
        out.push('=');
        emit_string_replace(out, "__nacre_replace_value", from, to);
        out.push('\n');
        if readonly && !local {
            out.push_str("readonly ");
            out.push_str(binding);
            out.push('\n');
        }
        return;
    }
    if local {
        out.push_str("local ");
    } else if readonly {
        out.push_str("readonly ");
    }
    out.push_str(binding);
    out.push('=');
    emit_string_replace_expr(out, value, from, to);
    out.push('\n');
}

pub(super) fn emit_checked_string_replace_value(
    out: &mut String,
    binding: &str,
    value: &Expr,
    local: bool,
) -> bool {
    match value {
        Expr::Command {
            command,
            checked: true,
        } => {
            if local {
                out.push_str("local ");
                out.push_str(binding);
                out.push_str(" __nacre_replace_value\n");
            }
            out.push_str("__nacre_replace_value=\"$(");
            emit_shell_command(out, command);
            out.push_str(")\" || exit $?\n");
            true
        }
        Expr::TryPipeline { input, commands } => {
            if local {
                out.push_str("local ");
                out.push_str(binding);
                out.push_str(" __nacre_replace_value\n");
            }
            out.push_str("__nacre_replace_value=\"$(");
            emit_pipeline(out, input.as_deref(), commands);
            out.push_str(")\" || exit $?\n");
            true
        }
        _ => false,
    }
}

pub(super) fn emit_string_replace(out: &mut String, name: &str, from: &Expr, to: &Expr) {
    out.push_str("\"$(");
    out.push_str("awk -v __nacre_value=\"$");
    out.push_str(name);
    out.push_str("\" -v __nacre_from=");
    emit_expr(out, from);
    out.push_str(" -v __nacre_to=");
    emit_expr(out, to);
    out.push(' ');
    emit_shell_word(out, STRING_REPLACE_AWK);
    out.push_str(")\"");
}

pub(super) fn emit_string_replace_expr(out: &mut String, value: &Expr, from: &Expr, to: &Expr) {
    out.push_str("\"$(");
    out.push_str("awk -v __nacre_value=");
    emit_call_arg(out, value);
    out.push_str(" -v __nacre_from=");
    emit_expr(out, from);
    out.push_str(" -v __nacre_to=");
    emit_expr(out, to);
    out.push(' ');
    emit_shell_word(out, STRING_REPLACE_AWK);
    out.push_str(")\"");
}

const STRING_REPLACE_AWK: &str = r#"BEGIN {
  if (__nacre_from == "") {
    printf "%s", __nacre_value
    exit
  }
  while ((idx = index(__nacre_value, __nacre_from)) > 0) {
    printf "%s%s", substr(__nacre_value, 1, idx - 1), __nacre_to
    __nacre_value = substr(__nacre_value, idx + length(__nacre_from))
  }
  printf "%s", __nacre_value
}"#;
