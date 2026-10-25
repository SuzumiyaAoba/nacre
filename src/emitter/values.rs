use super::bindings::emit_binding;
use super::expressions::{emit_call_arg, emit_expr};
use super::json::emit_map_expr_binding;
use super::quoting::{emit_bash_string, emit_shell_word};
use crate::symbols::{is_identifier, shell_function_name};
use crate::value_layout::{is_scalar_backed_type, value_suffixes};
use crate::{ClosureCapture, Expr, Type};

pub(super) fn emit_tuple_binding(
    out: &mut String,
    name: &str,
    values: &[Expr],
    readonly: bool,
    local: bool,
) {
    for (index, value) in values.iter().enumerate() {
        if local {
            out.push_str("local ");
        } else if readonly {
            out.push_str("readonly ");
        }
        out.push_str(name);
        out.push('_');
        out.push_str(&(index + 1).to_string());
        out.push('=');
        emit_expr(out, value);
        out.push('\n');
    }
}

pub(super) fn emit_record_binding(
    out: &mut String,
    name: &str,
    fields: &[(String, Expr)],
    readonly: bool,
    local: bool,
) {
    for (field, value) in fields {
        if local {
            out.push_str("local ");
        } else if readonly {
            out.push_str("readonly ");
        }
        out.push_str(name);
        out.push('_');
        out.push_str(field);
        out.push('=');
        emit_expr(out, value);
        out.push('\n');
    }
}

pub(super) fn constructor_record_fields(expr: &Expr) -> Option<(char, &[(String, Expr)])> {
    match expr {
        Expr::Some(value) | Expr::Ok(value) => match value.as_ref() {
            Expr::Record(fields) => Some(('1', fields.as_slice())),
            _ => None,
        },
        Expr::Err(value) => match value.as_ref() {
            Expr::Record(fields) => Some(('0', fields.as_slice())),
            _ => None,
        },
        _ => None,
    }
}

pub(super) fn constructor_tuple_values(expr: &Expr) -> Option<(char, &[Expr])> {
    match expr {
        Expr::Some(value) | Expr::Ok(value) => match value.as_ref() {
            Expr::Tuple(values) => Some(('1', values.as_slice())),
            _ => None,
        },
        Expr::Err(value) => match value.as_ref() {
            Expr::Tuple(values) => Some(('0', values.as_slice())),
            _ => None,
        },
        _ => None,
    }
}

pub(super) fn emit_constructor_record_binding(
    out: &mut String,
    name: &str,
    tag: char,
    fields: &[(String, Expr)],
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local ");
    } else if readonly {
        out.push_str("readonly ");
    }
    out.push_str(name);
    out.push('=');
    out.push(tag);
    out.push('\n');
    emit_record_binding(out, name, fields, readonly, local);
}

pub(super) fn emit_constructor_tuple_binding(
    out: &mut String,
    name: &str,
    tag: char,
    values: &[Expr],
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local ");
    } else if readonly {
        out.push_str("readonly ");
    }
    out.push_str(name);
    out.push('=');
    out.push(tag);
    out.push('\n');
    emit_tuple_binding(out, name, values, readonly, local);
}

pub(super) fn emit_record_value(out: &mut String, fields: &[(String, Expr)]) {
    out.push('(');
    for (index, (_field, value)) in fields.iter().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        emit_array_element(out, value);
    }
    out.push(')');
}

pub(super) fn emit_tuple_value(out: &mut String, values: &[Expr]) {
    out.push('(');
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        emit_array_element(out, value);
    }
    out.push(')');
}

pub(super) fn emit_tuple_element(out: &mut String, values: &[Expr]) {
    out.push_str("\"$(__nacre_tuple_pack");
    for value in values {
        out.push(' ');
        emit_call_arg(out, value);
    }
    out.push_str(")\"");
}

pub(super) fn emit_array_element_value(out: &mut String, values: &[Expr]) {
    out.push_str("\"$(__nacre_array_pack");
    for value in values {
        out.push(' ');
        emit_call_arg(out, value);
    }
    out.push_str(")\"");
}

pub(super) fn emit_record_element(out: &mut String, fields: &[(String, Expr)]) {
    out.push_str("\"$(__nacre_record_pack");
    for (field, value) in fields {
        out.push(' ');
        emit_shell_word(out, field);
        out.push(' ');
        emit_call_arg(out, value);
    }
    out.push_str(")\"");
}

pub(super) fn emit_tuple_field(out: &mut String, name: &str, field: usize) {
    out.push('"');
    out.push('$');
    out.push_str(name);
    out.push('_');
    out.push_str(&field.to_string());
    out.push('"');
}

pub(super) fn emit_tuple_field_value(out: &mut String, value: &Expr, field: usize) {
    if let Expr::Tuple(values) = value {
        if let Some(value) = values.get(field - 1) {
            emit_expr(out, value);
            return;
        }
    }
    out.push_str("\"$(__nacre_tuple_field ");
    emit_expr(out, value);
    out.push(' ');
    out.push_str(&field.to_string());
    out.push_str(")\"");
}

pub(super) fn emit_variable_ref(out: &mut String, name: &str) {
    out.push('"');
    out.push('$');
    out.push_str(name);
    out.push('"');
}

pub(super) fn emit_ident_value(out: &mut String, name: &str) {
    if is_identifier(name) {
        emit_variable_ref(out, name);
    } else {
        emit_shell_word(out, name);
    }
}

pub(super) fn emit_closure(out: &mut String, name: &str, captures: &[ClosureCapture]) {
    out.push_str("\"$(__nacre_closure_pack ");
    emit_shell_word(out, &shell_function_name(name));
    for capture in captures {
        for suffix in &capture.suffixes {
            out.push_str(" \"$(__nacre_capture ");
            emit_shell_word(out, &format!("{}{}", capture.source, suffix));
            out.push(' ');
            emit_shell_word(out, &format!("{}{}", capture.target, suffix));
            out.push_str(")\"");
        }
    }
    out.push_str(")\"");
}

pub(super) fn emit_let_in(
    out: &mut String,
    name: &str,
    binding_type: Option<&Type>,
    value: &Expr,
    body: &Expr,
) {
    out.push_str("\"$(");
    if binding_type.is_none_or(is_scalar_backed_type) {
        out.push_str(name);
        out.push('=');
        emit_expr(out, value);
        out.push_str("; ");
    } else {
        let source = if let Expr::Ident(source) = value {
            source.as_str()
        } else {
            let source = "__nacre_do_value";
            emit_binding(out, source, value, false, false);
            source
        };
        emit_inline_declaration_copy(
            out,
            source,
            name,
            binding_type.expect("structured let-in has a binding type"),
        );
    }
    out.push_str("__nacre_do_result=");
    emit_expr(out, body);
    out.push_str("; printf '%s' \"$__nacre_do_result\"");
    out.push_str(")\"");
}

pub(super) fn emit_inline_declaration_copy(
    out: &mut String,
    source: &str,
    target: &str,
    ty: &Type,
) {
    for suffix in value_suffixes(ty) {
        let source = format!("{source}{suffix}");
        let target = format!("{target}{suffix}");
        out.push_str("__nacre_do_declaration=\"$(declare -p ");
        emit_shell_word(out, &source);
        out.push_str(")\"; __nacre_do_declaration=\"${__nacre_do_declaration/ ");
        out.push_str(&source);
        out.push_str("=/ ");
        out.push_str(&target);
        out.push_str("=}\"; eval \"$__nacre_do_declaration\"; ");
    }
}

pub(super) fn emit_field(out: &mut String, name: &str, field: &str) {
    out.push('"');
    out.push('$');
    out.push_str(name);
    out.push('_');
    out.push_str(field);
    out.push('"');
}

pub(super) fn emit_field_value(out: &mut String, value: &Expr, field: &str) {
    if let Expr::Record(fields) = value {
        if let Some((_, value)) = fields.iter().find(|(candidate, _)| candidate == field) {
            emit_expr(out, value);
            return;
        }
    }
    out.push_str("\"$(__nacre_record_field ");
    emit_expr(out, value);
    out.push(' ');
    emit_shell_word(out, field);
    out.push_str(")\"");
}

pub(super) fn emit_index(out: &mut String, name: &str, index: &Expr) {
    out.push_str("\"${");
    out.push_str(name);
    out.push('[');
    emit_index_expr(out, index);
    out.push_str("]}\"");
}

pub(super) fn emit_index_value(out: &mut String, value: &Expr, index: &Expr) {
    out.push_str("$(");
    match value {
        Expr::Array(values) => {
            out.push_str("__nacre_index_value=");
            emit_array(out, values);
            out.push_str("; printf '%s' \"${__nacre_index_value[");
            emit_index_expr(out, index);
            out.push_str("]}\"");
        }
        Expr::Map(_) => {
            emit_map_expr_binding(out, "__nacre_index_value", value);
            out.push_str("printf '%s' \"${__nacre_index_value[");
            emit_index_expr(out, index);
            out.push_str("]}\"");
        }
        _ => {
            out.push_str("__nacre_index_value=");
            emit_expr(out, value);
            out.push_str("; __nacre_array_field \"$__nacre_index_value\" ");
            emit_index_expr(out, index);
        }
    }
    out.push(')');
}

pub(super) fn emit_index_expr(out: &mut String, expr: &Expr) {
    match expr {
        Expr::Int(value) => out.push_str(&value.to_string()),
        Expr::Ident(name) => out.push_str(name),
        _ => emit_expr(out, expr),
    }
}

pub(super) fn emit_array(out: &mut String, values: &[Expr]) {
    out.push('(');
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        emit_array_element(out, value);
    }
    out.push(')');
}

pub(super) fn emit_array_element(out: &mut String, expr: &Expr) {
    // Packed composite elements and literal quoting differ from call arguments.
    // All other values use the same argument emission rules.
    match expr {
        Expr::Bool(true) => out.push_str("true"),
        Expr::Bool(false) => out.push_str("false"),
        Expr::Unit => emit_bash_string(out, ""),
        Expr::Array(values) => emit_array_element_value(out, values),
        Expr::Tuple(values) => emit_tuple_element(out, values),
        Expr::Record(fields) => emit_record_element(out, fields),
        Expr::MatchGuardResult(value) | Expr::NewtypeCtor { value, .. } => {
            emit_array_element(out, value);
        }
        Expr::Cast { expr, .. } => emit_array_element(out, expr),
        _ => emit_call_arg(out, expr),
    }
}

pub(super) fn emit_variant(out: &mut String, name: &str, args: &[Expr], field_types: &[Type]) {
    if args.is_empty() {
        emit_shell_word(out, name);
        return;
    }
    out.push_str("\"$(__nacre_variant_pack ");
    emit_shell_word(out, name);
    for (index, (arg, ty)) in args.iter().zip(field_types).enumerate() {
        let target = format!("__nacre_variant_field_{}", index + 1);
        let match_target = format!("__nacre_match_{}", index + 1);
        out.push_str(" \"$(");
        if let Expr::Ident(source) = arg {
            emit_snapshot_declarations(out, source, &match_target, ty);
        } else {
            emit_binding(out, &target, arg, false, false);
            emit_snapshot_declarations(out, &target, &match_target, ty);
        }
        out.push_str(")\"");
    }
    out.push_str(")\"");
}

pub(super) fn emit_snapshot_declarations(out: &mut String, source: &str, target: &str, ty: &Type) {
    for suffix in value_suffixes(ty) {
        out.push_str("__nacre_capture ");
        emit_shell_word(out, &format!("{source}{suffix}"));
        out.push(' ');
        emit_shell_word(out, &format!("{target}{suffix}"));
        out.push_str("; printf '\\n'; ");
    }
}

pub(super) fn emit_map(out: &mut String, entries: &[(Expr, Expr)]) {
    out.push('(');
    for (index, (key, value)) in entries.iter().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        out.push('[');
        emit_map_key(out, key);
        out.push_str("]=");
        emit_array_element(out, value);
    }
    out.push(')');
}

pub(super) fn emit_map_key(out: &mut String, expr: &Expr) {
    match expr {
        Expr::String(value) | Expr::RawString(value) => emit_shell_word(out, value),
        Expr::Int(value) => out.push_str(&value.to_string()),
        Expr::Ident(name) => emit_ident_value(out, name),
        _ => emit_expr(out, expr),
    }
}
