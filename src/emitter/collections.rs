use super::arithmetic::emit_bash_arith_expr;
use super::expressions::{emit_call, emit_call_arg, emit_call_head, emit_expr};
use super::json::emit_map_expr_binding;
use super::quoting::{emit_bash_string, emit_shell_word};
use super::values::{emit_array, emit_array_element, emit_closure, emit_map_key};
use crate::Expr;

pub(super) fn emit_array_words(out: &mut String, expr: &Expr) {
    match expr {
        Expr::Ident(name) => {
            out.push_str(" \"${");
            out.push_str(name);
            out.push_str("[@]}\"");
        }
        Expr::ProcessArgs => out.push_str(" \"${args[@]}\""),
        Expr::Array(values) => {
            for value in values {
                out.push(' ');
                emit_array_element(out, value);
            }
        }
        Expr::Slice { name, start, end } => {
            out.push(' ');
            emit_array_slice_elements(out, name, start, end);
        }
        Expr::ArraySliceValue { value, start, end } => {
            out.push(' ');
            emit_array_slice_value_expr(out, value, start, end);
        }
        Expr::ArrayTake { name, count } => {
            out.push(' ');
            emit_array_take_elements(out, name, count);
        }
        Expr::ArrayTakeValue { value, count } => {
            out.push(' ');
            emit_array_take_value_expr(out, value, count);
        }
        Expr::ArrayDrop { name, count } => {
            out.push(' ');
            emit_array_drop_elements(out, name, count);
        }
        Expr::ArrayDropValue { value, count } => {
            out.push(' ');
            emit_array_drop_value_expr(out, value, count);
        }
        Expr::ArrayReverse(name) => {
            out.push(' ');
            emit_array_reverse_value(out, name);
        }
        Expr::ArrayReverseValue(value) => {
            out.push(' ');
            emit_array_reverse_value_expr(out, value);
        }
        Expr::ArraySort(name) => {
            out.push(' ');
            emit_array_sort_value(out, name);
        }
        Expr::ArraySortValue(value) => {
            out.push(' ');
            emit_array_sort_value_expr(out, value);
        }
        Expr::ArrayUnique(name) => {
            out.push(' ');
            emit_array_unique_value(out, name);
        }
        Expr::ArrayUniqueValue(value) => {
            out.push(' ');
            emit_array_unique_value_expr(out, value);
        }
        Expr::MapKeys(name) => {
            out.push_str(" \"${!");
            out.push_str(name);
            out.push_str("[@]}\"");
        }
        Expr::MapValues(name) => {
            out.push_str(" \"${");
            out.push_str(name);
            out.push_str("[@]}\"");
        }
        _ => {
            out.push(' ');
            emit_call_arg(out, expr);
        }
    }
}

pub(super) fn emit_array_slice_binding(
    out: &mut String,
    binding: &str,
    source: &str,
    start: &Expr,
    end: &Expr,
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local -a ");
    } else if readonly {
        out.push_str("readonly -a ");
    }
    out.push_str(binding);
    out.push_str("=(");
    emit_array_slice_elements(out, source, start, end);
    out.push_str(")\n");
}

pub(super) fn emit_array_slice_value_binding(
    out: &mut String,
    binding: &str,
    source: &Expr,
    start: &Expr,
    end: &Expr,
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local -a ");
        out.push_str(binding);
        out.push('\n');
    }
    emit_array_temp_assignment(out, "__nacre_array_value", source, local);
    out.push_str(binding);
    out.push_str("=(");
    emit_array_slice_elements(out, "__nacre_array_value", start, end);
    out.push_str(")\n");
    out.push_str("unset __nacre_array_value\n");
    if readonly && !local {
        out.push_str("readonly -a ");
        out.push_str(binding);
        out.push('\n');
    }
}

pub(super) fn emit_array_take_binding(
    out: &mut String,
    binding: &str,
    source: &str,
    count: &Expr,
    readonly: bool,
    local: bool,
) {
    emit_array_slice_binding(out, binding, source, &Expr::Int(0), count, readonly, local);
}

pub(super) fn emit_array_take_value_binding(
    out: &mut String,
    binding: &str,
    source: &Expr,
    count: &Expr,
    readonly: bool,
    local: bool,
) {
    emit_array_slice_value_binding(out, binding, source, &Expr::Int(0), count, readonly, local);
}

pub(super) fn emit_array_drop_binding(
    out: &mut String,
    binding: &str,
    source: &str,
    count: &Expr,
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local -a ");
    } else if readonly {
        out.push_str("readonly -a ");
    }
    out.push_str(binding);
    out.push_str("=(");
    emit_array_drop_elements(out, source, count);
    out.push_str(")\n");
}

pub(super) fn emit_array_drop_value_binding(
    out: &mut String,
    binding: &str,
    source: &Expr,
    count: &Expr,
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local -a ");
        out.push_str(binding);
        out.push('\n');
    }
    emit_array_temp_assignment(out, "__nacre_array_value", source, local);
    out.push_str(binding);
    out.push_str("=(");
    emit_array_drop_elements(out, "__nacre_array_value", count);
    out.push_str(")\n");
    out.push_str("unset __nacre_array_value\n");
    if readonly && !local {
        out.push_str("readonly -a ");
        out.push_str(binding);
        out.push('\n');
    }
}

pub(super) fn emit_array_temp_assignment(out: &mut String, name: &str, source: &Expr, local: bool) {
    if local {
        out.push_str("local -a ");
    }
    out.push_str(name);
    out.push('=');
    emit_expr(out, source);
    out.push('\n');
}

pub(super) fn emit_array_expansion_binding(
    out: &mut String,
    binding: &str,
    expansion: &str,
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local -a ");
    } else if readonly {
        out.push_str("readonly -a ");
    }
    out.push_str(binding);
    out.push_str("=(\"");
    out.push_str(expansion);
    out.push_str("\")\n");
}

pub(super) fn emit_array_reverse_binding(
    out: &mut String,
    binding: &str,
    source: &str,
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local -a ");
        out.push_str(binding);
        out.push('\n');
    }
    out.push_str(binding);
    out.push_str("=()\n");
    out.push_str("for ((__nacre_i=${#");
    out.push_str(source);
    out.push_str("[@]} - 1; __nacre_i >= 0; __nacre_i--)); do\n");
    out.push_str(binding);
    out.push_str("+=(\"${");
    out.push_str(source);
    out.push_str("[$__nacre_i]}\")\n");
    out.push_str("done\n");
    if readonly && !local {
        out.push_str("readonly -a ");
        out.push_str(binding);
        out.push('\n');
    }
}

pub(super) fn emit_array_reverse_value_binding(
    out: &mut String,
    binding: &str,
    source: &Expr,
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local -a ");
    } else if readonly {
        out.push_str("readonly -a ");
    }
    out.push_str(binding);
    out.push_str("=(");
    if let Expr::Array(values) = source {
        for (index, value) in values.iter().rev().enumerate() {
            if index > 0 {
                out.push(' ');
            }
            emit_array_element(out, value);
        }
    }
    out.push_str(")\n");
}

pub(super) fn emit_array_sort_binding(
    out: &mut String,
    binding: &str,
    source: &str,
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local -a ");
        out.push_str(binding);
        out.push('\n');
    }
    out.push_str(binding);
    out.push_str("=()\n");
    out.push_str("if [ \"${#");
    out.push_str(source);
    out.push_str("[@]}\" -gt 0 ]; then\n");
    out.push_str("mapfile -t ");
    out.push_str(binding);
    out.push_str(" < <(printf '%s\\n' \"${");
    out.push_str(source);
    out.push_str("[@]}\" | sort)\n");
    out.push_str("fi\n");
    if readonly && !local {
        out.push_str("readonly -a ");
        out.push_str(binding);
        out.push('\n');
    }
}

pub(super) fn emit_array_sort_value_binding(
    out: &mut String,
    binding: &str,
    source: &Expr,
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local -a ");
        out.push_str(binding);
        out.push('\n');
    }
    out.push_str(binding);
    out.push_str("=()\n");
    if let Expr::Array(values) = source {
        if !values.is_empty() {
            out.push_str("mapfile -t ");
            out.push_str(binding);
            out.push_str(" < <(printf '%s\\n'");
            emit_array_value_args(out, source);
            out.push_str(" | sort)\n");
        }
    }
    if readonly && !local {
        out.push_str("readonly -a ");
        out.push_str(binding);
        out.push('\n');
    }
}

pub(super) fn emit_array_unique_binding(
    out: &mut String,
    binding: &str,
    source: &str,
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local -a ");
        out.push_str(binding);
        out.push('\n');
    }
    emit_array_unique_to(out, binding, source);
    if readonly && !local {
        out.push_str("readonly -a ");
        out.push_str(binding);
        out.push('\n');
    }
}

pub(super) fn emit_array_unique_value_binding(
    out: &mut String,
    binding: &str,
    source: &Expr,
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local -a ");
        out.push_str(binding);
        out.push('\n');
    }
    out.push_str(binding);
    out.push_str("=()\n");
    out.push_str("for __nacre_item in ");
    emit_array_value_words(out, source);
    out.push_str("; do __nacre_seen=false; for __nacre_existing in \"${");
    out.push_str(binding);
    out.push_str("[@]}\"; do if [ \"$__nacre_existing\" = \"$__nacre_item\" ]; then __nacre_seen=true; break; fi; done; if [ \"$__nacre_seen\" = false ]; then ");
    out.push_str(binding);
    out.push_str("+=(\"$__nacre_item\"); fi; done\n");
    if readonly && !local {
        out.push_str("readonly -a ");
        out.push_str(binding);
        out.push('\n');
    }
}

pub(super) fn emit_array_unique_to(out: &mut String, binding: &str, source: &str) {
    out.push_str(binding);
    out.push_str("=()\n");
    out.push_str("for __nacre_item in \"${");
    out.push_str(source);
    out.push_str("[@]}\"; do\n");
    out.push_str("__nacre_seen=false\n");
    out.push_str("for __nacre_existing in \"${");
    out.push_str(binding);
    out.push_str("[@]}\"; do\n");
    out.push_str(
        "if [ \"$__nacre_existing\" = \"$__nacre_item\" ]; then __nacre_seen=true; break; fi\n",
    );
    out.push_str("done\n");
    out.push_str("if [ \"$__nacre_seen\" = false ]; then ");
    out.push_str(binding);
    out.push_str("+=(\"$__nacre_item\"); fi\n");
    out.push_str("done\n");
}

pub(super) fn emit_array_map_binding(
    out: &mut String,
    binding: &str,
    source: &str,
    mapper: &Expr,
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local -a ");
        out.push_str(binding);
        out.push('\n');
    }
    emit_array_map_to(out, binding, source, mapper);
    if readonly && !local {
        out.push_str("readonly -a ");
        out.push_str(binding);
        out.push('\n');
    }
}

pub(super) fn emit_array_map_value_binding(
    out: &mut String,
    binding: &str,
    source: &Expr,
    mapper: &Expr,
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local -a ");
        out.push_str(binding);
        out.push('\n');
    }
    emit_array_temp_assignment(out, "__nacre_array_value", source, local);
    emit_array_map_to(out, binding, "__nacre_array_value", mapper);
    out.push_str("unset __nacre_array_value\n");
    if readonly && !local {
        out.push_str("readonly -a ");
        out.push_str(binding);
        out.push('\n');
    }
}

pub(super) fn emit_array_map_to(out: &mut String, binding: &str, source: &str, mapper: &Expr) {
    out.push_str(binding);
    out.push_str("=()\n");
    out.push_str("for __nacre_item in \"${");
    out.push_str(source);
    out.push_str("[@]}\"; do\n");
    out.push_str(binding);
    out.push_str("+=(\"$(");
    emit_mapper_command(out, mapper);
    out.push_str(" \"$__nacre_item\")\")\n");
    out.push_str("done\n");
}

pub(super) fn emit_array_filter_binding(
    out: &mut String,
    binding: &str,
    source: &str,
    predicate: &Expr,
    readonly: bool,
    local: bool,
) {
    emit_array_loop_binding(out, binding, readonly, local, |out| {
        emit_array_filter_to(out, binding, source, predicate)
    });
}

pub(super) fn emit_array_filter_value_binding(
    out: &mut String,
    binding: &str,
    source: &Expr,
    predicate: &Expr,
    readonly: bool,
    local: bool,
) {
    emit_array_temp_assignment(out, "__nacre_array_value", source, local);
    emit_array_filter_binding(
        out,
        binding,
        "__nacre_array_value",
        predicate,
        readonly,
        local,
    );
    out.push_str("unset __nacre_array_value\n");
}

pub(super) fn emit_array_flat_map_binding(
    out: &mut String,
    binding: &str,
    source: &str,
    mapper: &Expr,
    readonly: bool,
    local: bool,
) {
    emit_array_loop_binding(out, binding, readonly, local, |out| {
        emit_array_flat_map_to(out, binding, source, mapper)
    });
}

pub(super) fn emit_array_flat_map_value_binding(
    out: &mut String,
    binding: &str,
    source: &Expr,
    mapper: &Expr,
    readonly: bool,
    local: bool,
) {
    emit_array_temp_assignment(out, "__nacre_array_value", source, local);
    emit_array_flat_map_binding(out, binding, "__nacre_array_value", mapper, readonly, local);
    out.push_str("unset __nacre_array_value\n");
}

pub(super) fn emit_array_loop_binding(
    out: &mut String,
    binding: &str,
    readonly: bool,
    local: bool,
    emit_body: impl FnOnce(&mut String),
) {
    if local {
        out.push_str("local -a ");
        out.push_str(binding);
        out.push('\n');
    }
    emit_body(out);
    if readonly && !local {
        out.push_str("readonly -a ");
        out.push_str(binding);
        out.push('\n');
    }
}

pub(super) fn emit_array_filter_to(
    out: &mut String,
    binding: &str,
    source: &str,
    predicate: &Expr,
) {
    out.push_str(binding);
    out.push_str("=()\n");
    out.push_str("for __nacre_item in \"${");
    out.push_str(source);
    out.push_str("[@]}\"; do\n");
    out.push_str("if [[ \"$(");
    emit_mapper_command(out, predicate);
    out.push_str(" \"$__nacre_item\")\" == true ]]; then ");
    out.push_str(binding);
    out.push_str("+=(\"$__nacre_item\"); fi\n");
    out.push_str("done\n");
}

pub(super) fn emit_array_flat_map_to(out: &mut String, binding: &str, source: &str, mapper: &Expr) {
    out.push_str(binding);
    out.push_str("=()\n");
    out.push_str("for __nacre_item in \"${");
    out.push_str(source);
    out.push_str("[@]}\"; do\n");
    out.push_str("while IFS= read -r __nacre_mapped; do ");
    out.push_str(binding);
    out.push_str("+=(\"$__nacre_mapped\"); done < <(");
    emit_mapper_command(out, mapper);
    out.push_str(" \"$__nacre_item\")\n");
    out.push_str("done\n");
}

pub(super) fn emit_array_find_binding(
    out: &mut String,
    binding: &str,
    source: &str,
    predicate: &Expr,
    readonly: bool,
    local: bool,
) {
    emit_scalar_binding_prefix(out, binding, readonly, local);
    out.push_str("\"$(for __nacre_item in \"${");
    out.push_str(source);
    out.push_str("[@]}\"; do if [[ \"$(");
    emit_mapper_command(out, predicate);
    out.push_str(" \"$__nacre_item\")\" == true ]]; then printf '1%s' \"$__nacre_item\"; break; fi; done)\"\n");
}

pub(super) fn emit_array_find_value_binding(
    out: &mut String,
    binding: &str,
    source: &Expr,
    predicate: &Expr,
    readonly: bool,
    local: bool,
) {
    emit_array_temp_assignment(out, "__nacre_array_value", source, local);
    emit_array_find_binding(
        out,
        binding,
        "__nacre_array_value",
        predicate,
        readonly,
        local,
    );
    out.push_str("unset __nacre_array_value\n");
}

pub(super) fn emit_array_bool_binding(
    out: &mut String,
    binding: &str,
    source: &str,
    predicate: &Expr,
    any: bool,
    readonly: bool,
    local: bool,
) {
    emit_scalar_binding_prefix(out, binding, readonly, local);
    out.push_str("\"$(for __nacre_item in \"${");
    out.push_str(source);
    out.push_str("[@]}\"; do if [[ \"$(");
    emit_mapper_command(out, predicate);
    out.push_str(" \"$__nacre_item\")\" == true ]]; then ");
    if any {
        out.push_str("printf true; exit");
    } else {
        out.push(':');
    }
    out.push_str("; else ");
    if any {
        out.push(':');
    } else {
        out.push_str("printf false; exit");
    }
    out.push_str("; fi; done; printf ");
    emit_shell_word(out, if any { "false" } else { "true" });
    out.push_str(")\"\n");
}

pub(super) fn emit_array_bool_value_binding(
    out: &mut String,
    binding: &str,
    source: &Expr,
    predicate: &Expr,
    any: bool,
    readonly: bool,
    local: bool,
) {
    emit_array_temp_assignment(out, "__nacre_array_value", source, local);
    emit_array_bool_binding(
        out,
        binding,
        "__nacre_array_value",
        predicate,
        any,
        readonly,
        local,
    );
    out.push_str("unset __nacre_array_value\n");
}

pub(super) fn emit_array_fold_binding(
    out: &mut String,
    binding: &str,
    source: &str,
    initial: &Expr,
    reducer: &Expr,
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local ");
    }
    out.push_str(binding);
    out.push('=');
    emit_call_arg(out, initial);
    out.push('\n');
    out.push_str("for __nacre_item in \"${");
    out.push_str(source);
    out.push_str("[@]}\"; do\n");
    out.push_str(binding);
    out.push_str("=\"$(");
    emit_mapper_command(out, reducer);
    out.push_str(" \"$");
    out.push_str(binding);
    out.push_str("\" \"$__nacre_item\")\"\n");
    out.push_str("done\n");
    if readonly && !local {
        out.push_str("readonly ");
        out.push_str(binding);
        out.push('\n');
    }
}

pub(super) fn emit_array_fold_value_binding(
    out: &mut String,
    binding: &str,
    source: &Expr,
    initial: &Expr,
    reducer: &Expr,
    readonly: bool,
    local: bool,
) {
    emit_array_temp_assignment(out, "__nacre_array_value", source, local);
    emit_array_fold_binding(
        out,
        binding,
        "__nacre_array_value",
        initial,
        reducer,
        readonly,
        local,
    );
    out.push_str("unset __nacre_array_value\n");
}

pub(super) fn emit_scalar_binding_prefix(
    out: &mut String,
    binding: &str,
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local ");
    } else if readonly {
        out.push_str("readonly ");
    }
    out.push_str(binding);
    out.push('=');
}

pub(super) fn emit_mapper_command(out: &mut String, mapper: &Expr) {
    match mapper {
        Expr::Ident(name) => emit_call_head(out, name),
        Expr::Closure { name, captures } => {
            out.push_str("__nacre_call ");
            emit_closure(out, name, captures);
        }
        _ => emit_expr(out, mapper),
    }
}

pub(super) fn emit_array_slice_value(out: &mut String, source: &str, start: &Expr, end: &Expr) {
    emit_array_slice_elements(out, source, start, end);
}

pub(super) fn emit_array_slice_value_expr(
    out: &mut String,
    source: &Expr,
    start: &Expr,
    end: &Expr,
) {
    out.push_str("$(__nacre_array_value=");
    emit_expr(out, source);
    out.push_str("; printf '%s\\n' ");
    emit_array_slice_elements(out, "__nacre_array_value", start, end);
    out.push(')');
}

pub(super) fn emit_array_take_value(out: &mut String, source: &str, count: &Expr) {
    emit_array_take_elements(out, source, count);
}

pub(super) fn emit_array_take_value_expr(out: &mut String, source: &Expr, count: &Expr) {
    emit_array_slice_value_expr(out, source, &Expr::Int(0), count);
}

pub(super) fn emit_array_drop_value(out: &mut String, source: &str, count: &Expr) {
    emit_array_drop_elements(out, source, count);
}

pub(super) fn emit_array_drop_value_expr(out: &mut String, source: &Expr, count: &Expr) {
    out.push_str("$(__nacre_array_value=");
    emit_expr(out, source);
    out.push_str("; printf '%s\\n' ");
    emit_array_drop_elements(out, "__nacre_array_value", count);
    out.push(')');
}

pub(super) fn emit_array_slice_elements(out: &mut String, source: &str, start: &Expr, end: &Expr) {
    out.push_str("\"${");
    out.push_str(source);
    out.push_str("[@]:");
    emit_arithmetic_expansion(out, start);
    out.push(':');
    emit_slice_length(out, start, end);
    out.push_str("}\"");
}

pub(super) fn emit_array_take_elements(out: &mut String, source: &str, count: &Expr) {
    emit_array_slice_elements(out, source, &Expr::Int(0), count);
}

pub(super) fn emit_array_drop_elements(out: &mut String, source: &str, count: &Expr) {
    out.push_str("\"${");
    out.push_str(source);
    out.push_str("[@]:");
    emit_arithmetic_expansion(out, count);
    out.push_str("}\"");
}

pub(super) fn emit_arithmetic_expansion(out: &mut String, expr: &Expr) {
    out.push_str("$((");
    emit_bash_arith_expr(out, expr);
    out.push_str("))");
}

pub(super) fn emit_slice_length(out: &mut String, start: &Expr, end: &Expr) {
    out.push_str("$((");
    emit_bash_arith_expr(out, end);
    out.push_str(" - ");
    emit_bash_arith_expr(out, start);
    out.push_str("))");
}

pub(super) fn emit_is_empty(out: &mut String, name: &str) {
    out.push_str("$(if [ \"${#");
    out.push_str(name);
    out.push_str("[@]}\" -eq 0 ]; then printf true; else printf false; fi)");
}

pub(super) fn emit_array_len_value(out: &mut String, value: &Expr) {
    if let Expr::Array(values) = value {
        out.push_str(&values.len().to_string());
    } else {
        out.push('0');
    }
}

pub(super) fn emit_array_is_empty_value(out: &mut String, value: &Expr) {
    if matches!(value, Expr::Array(values) if values.is_empty()) {
        out.push_str("true");
    } else {
        out.push_str("false");
    }
}

pub(super) fn emit_array_first(out: &mut String, name: &str) {
    out.push_str("\"${");
    out.push_str(name);
    out.push_str("[0]}\"");
}

pub(super) fn emit_array_first_value(out: &mut String, value: &Expr) {
    if let Expr::Array(values) = value {
        if let Some(first) = values.first() {
            emit_expr(out, first);
            return;
        }
    }
    emit_bash_string(out, "");
}

pub(super) fn emit_array_last(out: &mut String, name: &str) {
    out.push_str("$(if [ \"${#");
    out.push_str(name);
    out.push_str("[@]}\" -gt 0 ]; then printf '%s' \"${");
    out.push_str(name);
    out.push_str("[$((${#");
    out.push_str(name);
    out.push_str("[@]} - 1))]}\"; fi)");
}

pub(super) fn emit_array_last_value(out: &mut String, value: &Expr) {
    if let Expr::Array(values) = value {
        if let Some(last) = values.last() {
            emit_expr(out, last);
            return;
        }
    }
    emit_bash_string(out, "");
}

pub(super) fn emit_array_reverse_value(out: &mut String, name: &str) {
    out.push_str("$(__nacre_reverse=(); for ((__nacre_i=${#");
    out.push_str(name);
    out.push_str("[@]} - 1; __nacre_i >= 0; __nacre_i--)); do __nacre_reverse+=(\"${");
    out.push_str(name);
    out.push_str("[$__nacre_i]}\"); done; printf '%s\\n' \"${__nacre_reverse[@]}\")");
}

pub(super) fn emit_array_reverse_value_expr(out: &mut String, value: &Expr) {
    out.push_str("$(printf '%s\\n'");
    emit_array_value_args_reversed(out, value);
    out.push(')');
}

pub(super) fn emit_array_sort_value(out: &mut String, name: &str) {
    out.push_str("$(if [ \"${#");
    out.push_str(name);
    out.push_str("[@]}\" -gt 0 ]; then printf '%s\\n' \"${");
    out.push_str(name);
    out.push_str("[@]}\" | sort; fi)");
}

pub(super) fn emit_array_sort_value_expr(out: &mut String, value: &Expr) {
    out.push_str("$(printf '%s\\n'");
    emit_array_value_args(out, value);
    out.push_str(" | sort)");
}

pub(super) fn emit_array_unique_value(out: &mut String, name: &str) {
    out.push_str("$(__nacre_unique=(); for __nacre_item in \"${");
    out.push_str(name);
    out.push_str("[@]}\"; do __nacre_seen=false; for __nacre_existing in \"${__nacre_unique[@]}\"; do if [ \"$__nacre_existing\" = \"$__nacre_item\" ]; then __nacre_seen=true; break; fi; done; if [ \"$__nacre_seen\" = false ]; then __nacre_unique+=(\"$__nacre_item\"); fi; done; if [ \"${#__nacre_unique[@]}\" -gt 0 ]; then printf '%s\\n' \"${__nacre_unique[@]}\"; fi)");
}

pub(super) fn emit_array_unique_value_expr(out: &mut String, value: &Expr) {
    out.push_str("$(__nacre_unique=(); for __nacre_item in ");
    emit_array_value_words(out, value);
    out.push_str("; do __nacre_seen=false; for __nacre_existing in \"${__nacre_unique[@]}\"; do if [ \"$__nacre_existing\" = \"$__nacre_item\" ]; then __nacre_seen=true; break; fi; done; if [ \"$__nacre_seen\" = false ]; then __nacre_unique+=(\"$__nacre_item\"); fi; done; if [ \"${#__nacre_unique[@]}\" -gt 0 ]; then printf '%s\\n' \"${__nacre_unique[@]}\"; fi)");
}

pub(super) fn emit_array_map_value(out: &mut String, name: &str, mapper: &Expr) {
    out.push_str("$(for __nacre_item in \"${");
    out.push_str(name);
    out.push_str("[@]}\"; do ");
    emit_mapper_command(out, mapper);
    out.push_str(" \"$__nacre_item\"; done)");
}

pub(super) fn emit_array_map_value_expr(out: &mut String, value: &Expr, mapper: &Expr) {
    out.push_str("$(__nacre_array_value=");
    emit_expr(out, value);
    out.push_str("; for __nacre_item in \"${__nacre_array_value[@]}\"; do ");
    emit_mapper_command(out, mapper);
    out.push_str(" \"$__nacre_item\"; done)");
}

pub(super) fn emit_array_filter_value(out: &mut String, name: &str, predicate: &Expr) {
    out.push_str("$(for __nacre_item in \"${");
    out.push_str(name);
    out.push_str("[@]}\"; do if [[ \"$(");
    emit_mapper_command(out, predicate);
    out.push_str(
        " \"$__nacre_item\")\" == true ]]; then printf '%s\\n' \"$__nacre_item\"; fi; done)",
    );
}

pub(super) fn emit_array_filter_value_expr(out: &mut String, value: &Expr, predicate: &Expr) {
    out.push_str("$(__nacre_array_value=");
    emit_expr(out, value);
    out.push_str("; for __nacre_item in \"${__nacre_array_value[@]}\"; do if [[ \"$(");
    emit_mapper_command(out, predicate);
    out.push_str(
        " \"$__nacre_item\")\" == true ]]; then printf '%s\\n' \"$__nacre_item\"; fi; done)",
    );
}

pub(super) fn emit_array_flat_map_value(out: &mut String, name: &str, mapper: &Expr) {
    out.push_str("$(for __nacre_item in \"${");
    out.push_str(name);
    out.push_str("[@]}\"; do ");
    emit_mapper_command(out, mapper);
    out.push_str(" \"$__nacre_item\"; done)");
}

pub(super) fn emit_array_flat_map_value_expr(out: &mut String, value: &Expr, mapper: &Expr) {
    emit_array_map_value_expr(out, value, mapper);
}

pub(super) fn emit_array_find_value(out: &mut String, name: &str, predicate: &Expr) {
    out.push_str("$(for __nacre_item in \"${");
    out.push_str(name);
    out.push_str("[@]}\"; do if [[ \"$(");
    emit_mapper_command(out, predicate);
    out.push_str(
        " \"$__nacre_item\")\" == true ]]; then printf '1%s' \"$__nacre_item\"; break; fi; done)",
    );
}

pub(super) fn emit_array_find_value_expr(out: &mut String, value: &Expr, predicate: &Expr) {
    out.push_str("$(__nacre_array_value=");
    emit_expr(out, value);
    out.push_str("; for __nacre_item in \"${__nacre_array_value[@]}\"; do if [[ \"$(");
    emit_mapper_command(out, predicate);
    out.push_str(
        " \"$__nacre_item\")\" == true ]]; then printf '1%s' \"$__nacre_item\"; break; fi; done)",
    );
}

pub(super) fn emit_array_bool_value(out: &mut String, name: &str, predicate: &Expr, any: bool) {
    out.push_str("$(for __nacre_item in \"${");
    out.push_str(name);
    out.push_str("[@]}\"; do if [[ \"$(");
    emit_mapper_command(out, predicate);
    out.push_str(" \"$__nacre_item\")\" == true ]]; then ");
    out.push_str(if any { "printf true; exit" } else { ":" });
    out.push_str("; else ");
    out.push_str(if any { ":" } else { "printf false; exit" });
    out.push_str("; fi; done; printf ");
    emit_shell_word(out, if any { "false" } else { "true" });
    out.push(')');
}

pub(super) fn emit_array_bool_value_expr(
    out: &mut String,
    value: &Expr,
    predicate: &Expr,
    any: bool,
) {
    out.push_str("$(__nacre_array_value=");
    emit_expr(out, value);
    out.push_str("; for __nacre_item in \"${__nacre_array_value[@]}\"; do if [[ \"$(");
    emit_mapper_command(out, predicate);
    out.push_str(" \"$__nacre_item\")\" == true ]]; then ");
    out.push_str(if any { "printf true; exit" } else { ":" });
    out.push_str("; else ");
    out.push_str(if any { ":" } else { "printf false; exit" });
    out.push_str("; fi; done; printf ");
    emit_shell_word(out, if any { "false" } else { "true" });
    out.push(')');
}

pub(super) fn emit_array_fold_value(out: &mut String, name: &str, initial: &Expr, reducer: &Expr) {
    out.push_str("$(__nacre_acc=");
    emit_call_arg(out, initial);
    out.push_str("; for __nacre_item in \"${");
    out.push_str(name);
    out.push_str("[@]}\"; do __nacre_acc=\"$(");
    emit_mapper_command(out, reducer);
    out.push_str(" \"$__nacre_acc\" \"$__nacre_item\")\"; done; printf '%s' \"$__nacre_acc\")");
}

pub(super) fn emit_array_fold_value_expr(
    out: &mut String,
    value: &Expr,
    initial: &Expr,
    reducer: &Expr,
) {
    out.push_str("$(__nacre_array_value=");
    emit_expr(out, value);
    out.push_str("; __nacre_acc=");
    emit_call_arg(out, initial);
    out.push_str("; for __nacre_item in \"${__nacre_array_value[@]}\"; do __nacre_acc=\"$(");
    emit_mapper_command(out, reducer);
    out.push_str(" \"$__nacre_acc\" \"$__nacre_item\")\"; done; printf '%s' \"$__nacre_acc\")");
}

pub(super) fn emit_map_keys_value(out: &mut String, name: &str) {
    out.push_str("\"${!");
    out.push_str(name);
    out.push_str("[@]}\"");
}

pub(super) fn emit_map_values_value(out: &mut String, name: &str) {
    out.push_str("\"${");
    out.push_str(name);
    out.push_str("[@]}\"");
}

pub(super) fn emit_map_len_value(out: &mut String, value: &Expr) {
    out.push_str("$(");
    emit_map_expr_binding(out, "__nacre_map_value", value);
    out.push_str("printf '%s' \"${#__nacre_map_value[@]}\")");
}

pub(super) fn emit_map_is_empty_value(out: &mut String, value: &Expr) {
    out.push_str("$(");
    emit_map_expr_binding(out, "__nacre_map_value", value);
    out.push_str(
        "if [ \"${#__nacre_map_value[@]}\" -eq 0 ]; then printf true; else printf false; fi)",
    );
}

pub(super) fn emit_map_has(out: &mut String, name: &str, key: &Expr) {
    out.push_str("$(if [[ -v ");
    out.push_str(name);
    out.push('[');
    emit_map_key(out, key);
    out.push_str("] ]]; then printf true; else printf false; fi)");
}

pub(super) fn emit_map_keys_value_expr(out: &mut String, value: &Expr) {
    out.push_str("$(");
    emit_map_expr_binding(out, "__nacre_map_value", value);
    out.push_str("if [ \"${#__nacre_map_value[@]}\" -gt 0 ]; then printf '%s\\n' \"${!__nacre_map_value[@]}\"; fi)");
}

pub(super) fn emit_map_values_value_expr(out: &mut String, value: &Expr) {
    out.push_str("$(");
    emit_map_expr_binding(out, "__nacre_map_value", value);
    out.push_str("if [ \"${#__nacre_map_value[@]}\" -gt 0 ]; then printf '%s\\n' \"${__nacre_map_value[@]}\"; fi)");
}

pub(super) fn emit_map_keys_value_binding(
    out: &mut String,
    binding: &str,
    value: &Expr,
    readonly: bool,
    local: bool,
) {
    emit_map_entries_value_binding(out, binding, value, readonly, local, true);
}

pub(super) fn emit_map_values_value_binding(
    out: &mut String,
    binding: &str,
    value: &Expr,
    readonly: bool,
    local: bool,
) {
    emit_map_entries_value_binding(out, binding, value, readonly, local, false);
}

pub(super) fn emit_map_entries_value_binding(
    out: &mut String,
    binding: &str,
    value: &Expr,
    readonly: bool,
    local: bool,
    keys: bool,
) {
    if local {
        out.push_str("local -a ");
        out.push_str(binding);
        out.push('\n');
    }
    out.push_str(binding);
    out.push_str("=()\n");
    out.push_str("unset __nacre_map_value\n");
    emit_map_expr_binding(out, "__nacre_map_value", value);
    if keys {
        out.push_str("for __nacre_item in \"${!__nacre_map_value[@]}\"; do ");
    } else {
        out.push_str("for __nacre_item in \"${__nacre_map_value[@]}\"; do ");
    }
    out.push_str(binding);
    out.push_str("+=(\"$__nacre_item\"); done\n");
    if readonly && !local {
        out.push_str("readonly -a ");
        out.push_str(binding);
        out.push('\n');
    }
}

pub(super) fn emit_map_has_value(out: &mut String, value: &Expr, key: &Expr) {
    out.push_str("$(");
    emit_map_expr_binding(out, "__nacre_map_value", value);
    out.push_str("if [[ -v __nacre_map_value[");
    emit_map_key(out, key);
    out.push_str("] ]]; then printf true; else printf false; fi)");
}

pub(super) fn emit_array_contains(out: &mut String, name: &str, value: &Expr) {
    out.push_str("$(__nacre_contains=false; for __nacre_item in \"${");
    out.push_str(name);
    out.push_str("[@]}\"; do if [ \"$__nacre_item\" = ");
    emit_call_arg(out, value);
    out.push_str(
        " ]; then __nacre_contains=true; break; fi; done; printf '%s' \"$__nacre_contains\")",
    );
}

pub(super) fn emit_array_contains_value(out: &mut String, value: &Expr, item: &Expr) {
    out.push_str("$(__nacre_contains=false; for __nacre_item in ");
    emit_array_value_words(out, value);
    out.push_str("; do if [ \"$__nacre_item\" = ");
    emit_call_arg(out, item);
    out.push_str(
        " ]; then __nacre_contains=true; break; fi; done; printf '%s' \"$__nacre_contains\")",
    );
}

pub(super) fn emit_array_index_of(out: &mut String, name: &str, value: &Expr) {
    out.push_str("$(__nacre_index=-1; __nacre_i=0; for __nacre_item in \"${");
    out.push_str(name);
    out.push_str("[@]}\"; do if [ \"$__nacre_item\" = ");
    emit_call_arg(out, value);
    out.push_str(
        " ]; then __nacre_index=$__nacre_i; break; fi; __nacre_i=$((__nacre_i + 1)); done; printf '%s' \"$__nacre_index\")",
    );
}

pub(super) fn emit_array_index_of_value(out: &mut String, value: &Expr, item: &Expr) {
    out.push_str("$(__nacre_index=-1; __nacre_i=0; for __nacre_item in ");
    emit_array_value_words(out, value);
    out.push_str("; do if [ \"$__nacre_item\" = ");
    emit_call_arg(out, item);
    out.push_str(
        " ]; then __nacre_index=$__nacre_i; break; fi; __nacre_i=$((__nacre_i + 1)); done; printf '%s' \"$__nacre_index\")",
    );
}

pub(super) fn emit_array_value_words(out: &mut String, value: &Expr) {
    if let Expr::Array(values) = value {
        if values.is_empty() {
            return;
        }
        for (index, value) in values.iter().enumerate() {
            if index > 0 {
                out.push(' ');
            }
            emit_array_element(out, value);
        }
        return;
    }
    emit_expr(out, value);
}

pub(super) fn emit_array_value_args(out: &mut String, value: &Expr) {
    if let Expr::Array(values) = value {
        for value in values {
            out.push(' ');
            emit_array_element(out, value);
        }
        return;
    }
    out.push(' ');
    emit_expr(out, value);
}

pub(super) fn emit_array_value_args_reversed(out: &mut String, value: &Expr) {
    if let Expr::Array(values) = value {
        for value in values.iter().rev() {
            out.push(' ');
            emit_array_element(out, value);
        }
        return;
    }
    out.push(' ');
    emit_expr(out, value);
}

pub(super) fn emit_join(out: &mut String, name: &str, separator: &Expr) {
    out.push_str("\"$(");
    emit_join_command(out, name, separator);
    out.push_str(")\"");
}

pub(super) fn emit_join_value(out: &mut String, value: &Expr, separator: &Expr) {
    out.push_str("\"$(");
    if let Expr::Array(values) = value {
        out.push_str("__nacre_join_value=");
        emit_array(out, values);
        out.push_str("; ");
    }
    emit_join_command(out, "__nacre_join_value", separator);
    out.push_str(")\"");
}

pub(super) fn emit_join_command(out: &mut String, name: &str, separator: &Expr) {
    out.push_str("__nacre_join_first=true; for __nacre_join_item in \"${");
    out.push_str(name);
    out.push_str("[@]}\"; do if [ \"$__nacre_join_first\" = true ]; then __nacre_join_first=false; else printf '%s' ");
    emit_expr(out, separator);
    out.push_str("; fi; printf '%s' \"$__nacre_join_item\"; done");
}

pub(super) fn emit_array_push(out: &mut String, name: &str, value: &Expr) {
    out.push_str(name);
    out.push_str("+=(");
    emit_array_element(out, value);
    out.push_str(")\n");
}

pub(super) fn emit_array_pop(out: &mut String, name: &str) {
    out.push_str("if [ \"${#");
    out.push_str(name);
    out.push_str("[@]}\" -gt 0 ]; then unset \"");
    out.push_str(name);
    out.push_str("[$((${#");
    out.push_str(name);
    out.push_str("[@]} - 1))]\"; ");
    out.push_str(name);
    out.push_str("=(\"${");
    out.push_str(name);
    out.push_str("[@]}\"); fi\n");
}

pub(super) fn emit_map_set(out: &mut String, name: &str, key: &Expr, value: &Expr) {
    out.push_str("__nacre_map_key=");
    emit_array_element(out, key);
    out.push('\n');
    out.push_str(name);
    out.push_str("[\"$__nacre_map_key\"]=");
    emit_array_element(out, value);
    out.push('\n');
}

pub(super) fn emit_map_remove(out: &mut String, name: &str, key: &Expr) {
    out.push_str("__nacre_map_key=");
    emit_array_element(out, key);
    out.push('\n');
    out.push_str("unset \"");
    out.push_str(name);
    out.push_str("[$__nacre_map_key]\"\n");
}

pub(super) fn emit_std_str_join(out: &mut String, args: &[Expr]) {
    if let [Expr::Ident(name), separator] = args {
        emit_join(out, name, separator);
    } else {
        emit_call(out, "str.join", args);
    }
}
