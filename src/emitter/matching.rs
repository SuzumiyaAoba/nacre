use crate::{Expr, MatchArm, Type};

use super::match_analysis::{
    constructor_record_match_fields, constructor_tuple_match_width, has_constructor_match_pattern,
    record_match_fields, tuple_match_width, variant_match_width,
};
use super::quoting::emit_shell_word;
use super::shell::{emit_pipeline, emit_shell_command};
use super::value_layout::{is_scalar_backed_type, value_suffixes};
use super::{
    constructor_record_fields, constructor_tuple_values, emit_array_element, emit_call_arg,
    emit_condition, emit_expr,
};

pub(super) fn emit_match_expr(out: &mut String, value: &Expr, arms: &[MatchArm]) {
    let propagating_guard = arms
        .iter()
        .any(|arm| matches!(arm.guard, Some(Expr::MatchGuardResult(_))));
    out.push_str("\"$(");
    emit_match_value(out, value, arms);
    if propagating_guard {
        out.push_str("; __nacre_match_guard_error=''; ");
    } else {
        out.push_str("; ");
    }
    for (index, arm) in arms.iter().enumerate() {
        if index == 0 {
            out.push_str("if ");
        } else {
            out.push_str("elif ");
        }
        if propagating_guard {
            out.push_str("[ -z \"$__nacre_match_guard_error\" ] && ");
        }
        emit_match_arm_test(out, arm.pattern.as_ref(), arm.guard.as_ref());
        out.push_str("; then ");
        out.push_str("printf '%s\\n' ");
        emit_expr(out, &arm.expr);
        out.push_str("; ");
    }
    if propagating_guard {
        out.push_str(
            "else if [ -n \"$__nacre_match_guard_error\" ]; then printf '%s\\n' \"$__nacre_match_guard_error\"; fi; fi)\"",
        );
    } else {
        out.push_str("fi)\"");
    }
}

fn emit_match_value(out: &mut String, value: &Expr, arms: &[MatchArm]) {
    match value {
        Expr::Command { command, .. } => {
            emit_command_match_value(out, command);
            return;
        }
        Expr::CommandResult { command } => {
            emit_command_match_value(out, command);
            return;
        }
        Expr::Pipeline { input, commands } => {
            let mut command = String::new();
            emit_pipeline(&mut command, input.as_deref(), commands);
            emit_command_match_value(out, &command);
            return;
        }
        Expr::PipelineResult { input, commands } => {
            let mut command = String::new();
            emit_pipeline(&mut command, input.as_deref(), commands);
            emit_command_match_value(out, &command);
            return;
        }
        _ => {}
    }
    if let Some((tag, fields)) = constructor_record_fields(value) {
        emit_constructor_record_match_value(out, tag, fields);
        return;
    }
    if let Some((tag, values)) = constructor_tuple_values(value) {
        emit_constructor_tuple_match_value(out, tag, values);
        return;
    }
    match value {
        Expr::Tuple(values) => {
            out.push_str("__nacre_match=(");
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    out.push(' ');
                }
                emit_array_element(out, value);
            }
            out.push(')');
        }
        Expr::Record(fields) => {
            for (index, (field, value)) in fields.iter().enumerate() {
                if index > 0 {
                    out.push_str("; ");
                }
                out.push_str("__nacre_match_");
                out.push_str(field);
                out.push('=');
                emit_expr(out, value);
            }
        }
        Expr::Ident(name) => {
            if let Some(width) = variant_match_width(arms) {
                out.push_str("__nacre_match=\"$");
                out.push_str(name);
                out.push('"');
                emit_decode_variant_match(out, width);
            } else if let Some(fields) = constructor_record_match_fields(arms) {
                out.push_str("__nacre_match=\"$");
                out.push_str(name);
                out.push('"');
                emit_constructor_match_error_fields(out, name);
                for field in fields {
                    out.push_str("; __nacre_match_");
                    out.push_str(&field);
                    out.push_str("=\"${");
                    out.push_str(name);
                    out.push('_');
                    out.push_str(&field);
                    out.push_str("-}\"");
                }
            } else if let Some(width) = constructor_tuple_match_width(arms) {
                out.push_str("__nacre_match=\"$");
                out.push_str(name);
                out.push('"');
                emit_constructor_match_error_fields(out, name);
                for index in 0..width {
                    out.push_str("; __nacre_match_");
                    out.push_str(&(index + 1).to_string());
                    out.push_str("=\"${");
                    out.push_str(name);
                    out.push('_');
                    out.push_str(&(index + 1).to_string());
                    out.push_str("-}\"");
                }
            } else if let Some(fields) = record_match_fields(arms) {
                for (index, field) in fields.iter().enumerate() {
                    if index > 0 {
                        out.push_str("; ");
                    }
                    out.push_str("__nacre_match_");
                    out.push_str(field);
                    out.push_str("=\"$");
                    out.push_str(name);
                    out.push('_');
                    out.push_str(field);
                    out.push('"');
                }
            } else if let Some(width) = tuple_match_width(arms) {
                out.push_str("__nacre_match=(");
                for index in 0..width {
                    if index > 0 {
                        out.push(' ');
                    }
                    out.push_str("\"$");
                    out.push_str(name);
                    out.push('_');
                    out.push_str(&(index + 1).to_string());
                    out.push('"');
                }
                out.push(')');
            } else if has_constructor_match_pattern(arms) {
                out.push_str("__nacre_match=\"$");
                out.push_str(name);
                out.push('"');
                emit_constructor_match_error_fields(out, name);
            } else {
                out.push_str("__nacre_match=");
                emit_expr(out, value);
            }
        }
        _ => {
            out.push_str("__nacre_match=");
            emit_expr(out, value);
            if has_constructor_match_pattern(arms) {
                emit_decode_embedded_match_error(out);
            }
            if let Some(width) = variant_match_width(arms) {
                emit_decode_variant_match(out, width);
            }
        }
    }
}

fn emit_decode_variant_match(out: &mut String, width: usize) {
    out.push_str("; if [[ \"$__nacre_match\" == __nacre_variant:* ]]; then eval \"$(__nacre_variant_unpack \"$__nacre_match\")\"; else __nacre_match_tag=\"${__nacre_match%%$'\\037'*}\"");
    if width == 0 {
        out.push_str("; fi");
        return;
    }
    out.push_str("; __nacre_match_rest=\"${__nacre_match#*$'\\037'}\"");
    for index in 1..=width {
        out.push_str("; __nacre_match_");
        out.push_str(&index.to_string());
        out.push_str("=\"${__nacre_match_rest%%$'\\037'*}\"");
        if index < width {
            out.push_str("; __nacre_match_rest=\"${__nacre_match_rest#*$'\\037'}\"");
        }
    }
    out.push_str("; fi");
}

fn emit_decode_embedded_match_error(out: &mut String) {
    out.push_str("; case \"$__nacre_match\" in 0*) ");
    out.push_str("if [ -z \"${__nacre_match_code+x}\" ] || [ -z \"$__nacre_match_code\" ]; then ");
    out.push_str("__nacre_match_payload=\"${__nacre_match#?}\"; ");
    out.push_str("case \"$__nacre_match_payload\" in *$'\\037'*) ");
    out.push_str("__nacre_match_code=\"${__nacre_match_payload%%$'\\037'*}\"; ");
    out.push_str("__nacre_match_stderr=\"${__nacre_match_payload#*$'\\037'}\" ;; esac; fi ;; esac");
}

fn emit_constructor_match_error_fields(out: &mut String, name: &str) {
    out.push_str("; __nacre_match_code=\"${");
    out.push_str(name);
    out.push_str("_code-}\"; __nacre_match_stderr=\"${");
    out.push_str(name);
    out.push_str("_stderr-}\"");
    emit_decode_embedded_match_error(out);
}

fn emit_command_match_value(out: &mut String, command: &str) {
    out.push_str("__nacre_match_stderr_file=\"$(mktemp)\"; ");
    out.push_str("if __nacre_match_output=\"$({ ");
    emit_shell_command(out, command);
    out.push_str("; } 2>\"$__nacre_match_stderr_file\")\"; then ");
    out.push_str("__nacre_match=$(printf '1%s' \"$__nacre_match_output\"); ");
    out.push_str("else __nacre_match_code=$?; ");
    out.push_str("__nacre_match_stderr=\"$(cat \"$__nacre_match_stderr_file\")\"; ");
    out.push_str("__nacre_match=$(printf '0%s\\037%s' \"$__nacre_match_code\" \"$__nacre_match_stderr\"); fi; ");
    out.push_str("rm -f \"$__nacre_match_stderr_file\"");
}

pub(super) fn emit_command_result_value(out: &mut String, command: &str) {
    out.push_str("\"$(");
    emit_command_match_value(out, command);
    out.push_str("; printf '%s' \"$__nacre_match\")\"");
}

pub(super) fn emit_command_result_binding(
    out: &mut String,
    name: &str,
    command: &str,
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local ");
        out.push_str(name);
        out.push(' ');
        out.push_str(name);
        out.push_str("_code ");
        out.push_str(name);
        out.push_str("_stderr\n");
    }
    out.push_str("__nacre_result_stderr_file=\"$(mktemp)\"\n");
    out.push_str("if __nacre_result_output=\"$({ ");
    emit_shell_command(out, command);
    out.push_str("; } 2>\"$__nacre_result_stderr_file\")\"; then\n");
    out.push_str(name);
    out.push_str("=$(printf '1%s' \"$__nacre_result_output\")\n");
    out.push_str(name);
    out.push_str("_code=0\n");
    out.push_str(name);
    out.push_str("_stderr=''\n");
    out.push_str("else\n__nacre_result_code=$?\n");
    out.push_str(name);
    out.push_str("_code=\"$__nacre_result_code\"\n");
    out.push_str(name);
    out.push_str("_stderr=\"$(cat \"$__nacre_result_stderr_file\")\"\n");
    out.push_str(name);
    out.push_str("=$(printf '0%s\\037%s' \"$");
    out.push_str(name);
    out.push_str("_code\" \"$");
    out.push_str(name);
    out.push_str("_stderr\")\nfi\n");
    out.push_str("rm -f \"$__nacre_result_stderr_file\"\n");
    if readonly && !local {
        out.push_str("readonly ");
        out.push_str(name);
        out.push(' ');
        out.push_str(name);
        out.push_str("_code ");
        out.push_str(name);
        out.push_str("_stderr\n");
    }
}

fn emit_constructor_record_match_value(out: &mut String, tag: char, fields: &[(String, Expr)]) {
    out.push_str("__nacre_match=");
    out.push(tag);
    for (field, value) in fields {
        out.push_str("; __nacre_match_");
        out.push_str(field);
        out.push('=');
        emit_expr(out, value);
    }
}

fn emit_constructor_tuple_match_value(out: &mut String, tag: char, values: &[Expr]) {
    out.push_str("__nacre_match=");
    out.push(tag);
    for (index, value) in values.iter().enumerate() {
        out.push_str("; __nacre_match_");
        out.push_str(&(index + 1).to_string());
        out.push('=');
        emit_expr(out, value);
    }
}

pub(super) fn emit_concat(out: &mut String, expr: &Expr) {
    let mut parts = Vec::new();
    collect_concat_parts(expr, &mut parts);
    out.push_str("\"$(printf '%s'");
    for part in parts {
        out.push(' ');
        emit_call_arg(out, part);
    }
    out.push_str(")\"");
}

pub(super) fn collect_concat_parts<'a>(expr: &'a Expr, parts: &mut Vec<&'a Expr>) {
    match expr {
        Expr::Binary {
            left,
            op: crate::BinaryOp::Concat,
            right,
        } => {
            collect_concat_parts(left, parts);
            collect_concat_parts(right, parts);
        }
        _ => parts.push(expr),
    }
}

pub(super) fn emit_match_pattern(out: &mut String, pattern: &Expr) {
    match pattern {
        Expr::Int(value) => out.push_str(&value.to_string()),
        Expr::Float(value) => out.push_str(value),
        Expr::Bool(true) => emit_shell_word(out, "true"),
        Expr::Bool(false) => emit_shell_word(out, "false"),
        Expr::String(value) | Expr::RawString(value) => emit_shell_word(out, value),
        Expr::NewtypeCtor { value, .. } => emit_match_pattern(out, value),
        Expr::Cast { expr, .. } => emit_match_pattern(out, expr),
        _ => emit_expr(out, pattern),
    }
}

fn emit_match_arm_pattern<'a>(out: &mut String, pattern: &'a Expr) -> Option<&'a str> {
    match pattern {
        Expr::Some(payload) | Expr::Ok(payload) => {
            emit_constructor_match_pattern(out, '1', payload)
        }
        Expr::Err(payload) => emit_constructor_match_pattern(out, '0', payload),
        Expr::None => {
            out.push('0');
            None
        }
        _ => {
            emit_match_pattern(out, pattern);
            None
        }
    }
}

fn emit_match_arm_test(out: &mut String, pattern: Option<&Expr>, guard: Option<&Expr>) {
    match pattern {
        None => {
            if let Some(guard) = guard {
                emit_match_guard(out, guard);
            } else {
                out.push_str("true");
            }
        }
        Some(Expr::Tuple(patterns)) => emit_tuple_match_arm_test(out, patterns, guard),
        Some(Expr::RecordPattern(patterns)) => emit_record_match_arm_test(out, patterns, guard),
        Some(Expr::Variant {
            name,
            args,
            field_types,
        }) => emit_variant_match_arm_test(out, name, args, field_types, guard),
        Some(Expr::Some(payload)) | Some(Expr::Ok(payload))
            if matches!(payload.as_ref(), Expr::Tuple(_)) =>
        {
            if let Expr::Tuple(patterns) = payload.as_ref() {
                emit_constructor_tuple_match_arm_test(out, '1', patterns, guard);
            }
        }
        Some(Expr::Err(payload)) if matches!(payload.as_ref(), Expr::Tuple(_)) => {
            if let Expr::Tuple(patterns) = payload.as_ref() {
                emit_constructor_tuple_match_arm_test(out, '0', patterns, guard);
            }
        }
        Some(Expr::Some(payload)) | Some(Expr::Ok(payload))
            if matches!(payload.as_ref(), Expr::RecordPattern(_)) =>
        {
            if let Expr::RecordPattern(patterns) = payload.as_ref() {
                emit_constructor_record_match_arm_test(out, '1', patterns, guard);
            }
        }
        Some(Expr::Err(payload)) if matches!(payload.as_ref(), Expr::RecordPattern(_)) => {
            if let Expr::RecordPattern(patterns) = payload.as_ref() {
                emit_constructor_record_match_arm_test(out, '0', patterns, guard);
            }
        }
        Some(pattern) => {
            out.push_str("case \"$__nacre_match\" in ");
            let binding = emit_match_arm_pattern(out, pattern);
            out.push_str(") ");
            if let Some(binding) = binding {
                out.push_str(binding);
                out.push_str("=\"${__nacre_match#?}\"; ");
                out.push_str(binding);
                out.push_str("_code=\"${__nacre_match_code-}\"; ");
                out.push_str(binding);
                out.push_str("_stderr=\"${__nacre_match_stderr-}\"; ");
            }
            if let Some(guard) = guard {
                emit_match_guard(out, guard);
            } else {
                out.push_str("true");
            }
            out.push_str(" ;; *) false ;; esac");
        }
    }
}

fn emit_variant_match_arm_test(
    out: &mut String,
    name: &str,
    patterns: &[Expr],
    field_types: &[Type],
    guard: Option<&Expr>,
) {
    out.push_str("[ \"$__nacre_match_tag\" = ");
    emit_shell_word(out, name);
    out.push_str(" ]");
    emit_variant_pattern_conditions(out, patterns, field_types);
    if let Some(guard) = guard {
        out.push_str(" && ");
        emit_match_guard(out, guard);
    }
}

fn emit_variant_pattern_conditions(out: &mut String, patterns: &[Expr], field_types: &[Type]) {
    for (index, (pattern, ty)) in patterns.iter().zip(field_types).enumerate() {
        if matches!(pattern, Expr::Ident(name) if name == "_") {
            continue;
        }
        let field = index + 1;
        if let Expr::Ident(name) = pattern {
            out.push_str(" && ");
            if is_scalar_backed_type(ty) {
                out.push_str(name);
                out.push_str("=\"$__nacre_match_");
                out.push_str(&field.to_string());
                out.push('"');
            } else {
                for (suffix_index, suffix) in value_suffixes(ty).iter().enumerate() {
                    if suffix_index > 0 {
                        out.push_str(" && ");
                    }
                    out.push_str("eval \"$(__nacre_capture ");
                    emit_shell_word(out, &format!("__nacre_match_{field}{suffix}"));
                    out.push(' ');
                    emit_shell_word(out, &format!("{name}{suffix}"));
                    out.push_str(")\"");
                }
            }
            continue;
        }
        out.push_str(" && [ \"$__nacre_match_");
        out.push_str(&field.to_string());
        out.push_str("\" = ");
        emit_match_pattern(out, pattern);
        out.push_str(" ]");
    }
}

fn emit_record_match_arm_test(
    out: &mut String,
    patterns: &[(String, Option<Expr>)],
    guard: Option<&Expr>,
) {
    out.push_str("true");
    emit_record_pattern_conditions(out, patterns);
    if let Some(guard) = guard {
        out.push_str(" && ");
        emit_match_guard(out, guard);
    }
}

fn emit_constructor_record_match_arm_test(
    out: &mut String,
    tag: char,
    patterns: &[(String, Option<Expr>)],
    guard: Option<&Expr>,
) {
    emit_constructor_tag_test(out, tag);
    emit_record_pattern_conditions(out, patterns);
    if let Some(guard) = guard {
        out.push_str(" && ");
        emit_match_guard(out, guard);
    }
}

fn emit_constructor_tuple_match_arm_test(
    out: &mut String,
    tag: char,
    patterns: &[Expr],
    guard: Option<&Expr>,
) {
    emit_constructor_tag_test(out, tag);
    emit_tuple_var_pattern_conditions(out, patterns, "__nacre_match");
    if let Some(guard) = guard {
        out.push_str(" && ");
        emit_match_guard(out, guard);
    }
}

fn emit_match_guard(out: &mut String, guard: &Expr) {
    match guard {
        Expr::MatchGuardResult(value) => {
            out.push_str("{ __nacre_match_guard_result=");
            emit_expr(out, value);
            out.push_str(
                "; case \"$__nacre_match_guard_result\" in 1true) true ;; 1*) false ;; *) __nacre_match_guard_error=\"$__nacre_match_guard_result\"; false ;; esac; }",
            );
        }
        _ => emit_condition(out, guard),
    }
}

fn emit_constructor_tag_test(out: &mut String, tag: char) {
    out.push_str("case \"$__nacre_match\" in ");
    out.push(tag);
    out.push_str("*) true ;; *) false ;; esac");
}

fn emit_record_pattern_conditions(out: &mut String, patterns: &[(String, Option<Expr>)]) {
    for (field, pattern) in patterns {
        match pattern {
            None => {
                out.push_str(" && ");
                out.push_str(field);
                out.push_str("=\"$__nacre_match_");
                out.push_str(field);
                out.push('"');
            }
            Some(Expr::Ident(name)) if name == "_" => {}
            Some(Expr::Ident(name)) => {
                out.push_str(" && ");
                out.push_str(name);
                out.push_str("=\"$__nacre_match_");
                out.push_str(field);
                out.push('"');
            }
            Some(pattern) => {
                out.push_str(" && [ \"$__nacre_match_");
                out.push_str(field);
                out.push_str("\" = ");
                emit_match_pattern(out, pattern);
                out.push_str(" ]");
            }
        }
    }
}

fn emit_tuple_match_arm_test(out: &mut String, patterns: &[Expr], guard: Option<&Expr>) {
    out.push_str("[ \"${#__nacre_match[@]}\" -eq ");
    out.push_str(&patterns.len().to_string());
    out.push_str(" ]");
    for (index, pattern) in patterns.iter().enumerate() {
        if matches!(pattern, Expr::Ident(name) if name == "_") {
            continue;
        }
        if let Expr::Ident(name) = pattern {
            out.push_str(" && ");
            out.push_str(name);
            out.push_str("=\"${__nacre_match[");
            out.push_str(&index.to_string());
            out.push_str("]}\"");
            continue;
        }
        out.push_str(" && [ \"${__nacre_match[");
        out.push_str(&index.to_string());
        out.push_str("]}\" = ");
        emit_match_pattern(out, pattern);
        out.push_str(" ]");
    }
    if let Some(guard) = guard {
        out.push_str(" && ");
        emit_match_guard(out, guard);
    }
}

fn emit_tuple_var_pattern_conditions(out: &mut String, patterns: &[Expr], prefix: &str) {
    for (index, pattern) in patterns.iter().enumerate() {
        if matches!(pattern, Expr::Ident(name) if name == "_") {
            continue;
        }
        let field = index + 1;
        if let Expr::Ident(name) = pattern {
            out.push_str(" && ");
            out.push_str(name);
            out.push_str("=\"$");
            out.push_str(prefix);
            out.push('_');
            out.push_str(&field.to_string());
            out.push('"');
            continue;
        }
        out.push_str(" && [ \"$");
        out.push_str(prefix);
        out.push('_');
        out.push_str(&field.to_string());
        out.push_str("\" = ");
        emit_match_pattern(out, pattern);
        out.push_str(" ]");
    }
}

fn emit_constructor_match_pattern<'a>(
    out: &mut String,
    tag: char,
    payload: &'a Expr,
) -> Option<&'a str> {
    out.push(tag);
    match payload {
        Expr::Ident(name) if name == "_" => {
            out.push('*');
            None
        }
        Expr::Ident(name) => {
            out.push('*');
            Some(name)
        }
        _ => {
            emit_match_pattern(out, payload);
            None
        }
    }
}
