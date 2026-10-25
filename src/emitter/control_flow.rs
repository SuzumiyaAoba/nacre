use super::bindings::emit_destructure;
use super::collections::{
    emit_array_drop_elements, emit_array_drop_value_binding, emit_array_drop_value_expr,
    emit_array_map_binding, emit_array_map_value, emit_array_map_value_binding,
    emit_array_map_value_expr, emit_array_reverse_value, emit_array_reverse_value_binding,
    emit_array_reverse_value_expr, emit_array_slice_elements, emit_array_slice_value_binding,
    emit_array_slice_value_expr, emit_array_sort_value, emit_array_sort_value_binding,
    emit_array_sort_value_expr, emit_array_take_elements, emit_array_take_value_binding,
    emit_array_take_value_expr, emit_array_unique_to, emit_array_unique_value,
    emit_array_unique_value_binding, emit_array_unique_value_expr,
};
use super::expressions::{emit_call_arg, emit_call_command, emit_condition, emit_expr};
use super::filesystem::emit_fs_list_command;
use super::quoting::emit_shell_word;
use super::shell::emit_shell_command;
use super::strings::{
    emit_checked_string_split_value, emit_string_split_command, emit_string_split_expr_command,
};
use super::values::emit_array_element;
use super::{emit_statement, EmitScope};
use crate::{BindingPattern, Expr, ForBinding, Program, Statement};

pub(super) fn emit_return(out: &mut String, expr: &Expr) {
    if let Expr::TryResult(value) = expr {
        emit_return_try_result(out, value);
        return;
    }
    if matches!(
        expr,
        Expr::Array(_)
            | Expr::ArrayMap { .. }
            | Expr::ArrayMapValue { .. }
            | Expr::ArrayFilter { .. }
            | Expr::ArrayFilterValue { .. }
            | Expr::ArrayFlatMap { .. }
            | Expr::ArrayFlatMapValue { .. }
            | Expr::ArrayReverse(_)
            | Expr::ArrayReverseValue(_)
            | Expr::ArraySort(_)
            | Expr::ArraySortValue(_)
            | Expr::ArrayUnique(_)
            | Expr::ArrayUniqueValue(_)
    ) {
        out.push_str("local -a __nacre_return_value\n__nacre_return_value=");
        emit_expr(out, expr);
        out.push_str("\nif [ \"${#__nacre_return_value[@]}\" -gt 0 ]; then printf '%s\\n' \"${__nacre_return_value[@]}\"; fi\n");
        out.push_str("return 0\n");
        return;
    }
    out.push_str("local __nacre_return_value\n__nacre_return_value=");
    emit_expr(out, expr);
    out.push_str("\nprintf '%s\\n' \"$__nacre_return_value\"\n");
    out.push_str("return 0\n");
}

pub(super) fn emit_try_result_binding(
    out: &mut String,
    name: &str,
    expr: &Expr,
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local ");
        out.push_str(name);
        out.push_str(" __nacre_try_result\n");
    }
    out.push_str("__nacre_try_result=");
    emit_expr(out, expr);
    out.push('\n');
    out.push_str("case \"$__nacre_try_result\" in\n");
    out.push_str("1*) ");
    out.push_str(name);
    out.push_str("=\"${__nacre_try_result#?}\" ;;\n");
    out.push_str("0*) printf '%s\\n' \"$__nacre_try_result\"; return 0 ;;\n");
    out.push_str("esac\n");
    if readonly && !local {
        out.push_str("readonly ");
        out.push_str(name);
        out.push('\n');
    }
}

pub(super) fn emit_return_try_result(out: &mut String, expr: &Expr) {
    out.push_str("__nacre_return_result=");
    emit_expr(out, expr);
    out.push('\n');
    out.push_str("case \"$__nacre_return_result\" in\n");
    out.push_str(
        "1*) printf '%s\\n' \"$(printf '1%s' \"${__nacre_return_result#?}\")\"; return 0 ;;\n",
    );
    out.push_str("0*) printf '%s\\n' \"$__nacre_return_result\"; return 0 ;;\n");
    out.push_str("esac\n");
}

pub(super) fn emit_try_result_value(out: &mut String, expr: &Expr) {
    out.push_str("$(__nacre_try_result=");
    emit_expr(out, expr);
    out.push_str("; case \"$__nacre_try_result\" in 1*) printf '%s' \"${__nacre_try_result#?}\" ;; 0*) printf '%s' \"$__nacre_try_result\"; return 0 ;; esac)");
}

pub(super) fn emit_try_command_result(out: &mut String, command: &str, scope: EmitScope) {
    if scope.is_function() {
        out.push_str(
            "local __nacre_try_stderr_file __nacre_try_output __nacre_try_code __nacre_try_stderr\n",
        );
    }
    out.push_str("__nacre_try_stderr_file=\"$(mktemp)\"\n");
    out.push_str("if __nacre_try_output=\"$({ ");
    emit_shell_command(out, command);
    out.push_str("; } 2>\"$__nacre_try_stderr_file\")\"; then\n");
    out.push_str("rm -f \"$__nacre_try_stderr_file\"\n");
    out.push_str("else\n");
    out.push_str("__nacre_try_code=$?\n");
    out.push_str("__nacre_try_stderr=\"$(cat \"$__nacre_try_stderr_file\")\"\n");
    out.push_str("rm -f \"$__nacre_try_stderr_file\"\n");
    out.push_str(
        "printf '%s\\n' \"$(printf '0%s\\037%s' \"$__nacre_try_code\" \"$__nacre_try_stderr\")\"\n",
    );
    out.push_str("return 0\n");
    out.push_str("fi\n");
}

pub(super) fn emit_try_result(out: &mut String, expr: &Expr, scope: EmitScope) {
    if scope.is_function() {
        out.push_str("local ");
    }
    out.push_str("__nacre_try_result=");
    emit_expr(out, expr);
    out.push('\n');
    out.push_str("case \"$__nacre_try_result\" in 1*) : ;; 0*) printf '%s\\n' \"$__nacre_try_result\"; return 0 ;; esac\n");
}

pub(super) fn emit_if_with_defers(
    out: &mut String,
    condition: &Expr,
    then_branch: &Program,
    else_branch: Option<&Program>,
    scope: EmitScope,
    defers: &[Statement],
) {
    out.push_str("if ");
    emit_condition(out, condition);
    out.push_str("; then\n");
    emit_block_with_defers(out, then_branch, scope, defers);
    if let Some(else_branch) = else_branch {
        out.push_str("else\n");
        emit_block_with_defers(out, else_branch, scope, defers);
    }
    out.push_str("fi\n");
}

pub(super) fn emit_while(out: &mut String, condition: &Expr, body: &Program, scope: EmitScope) {
    out.push_str("while ");
    emit_condition(out, condition);
    out.push_str("; do\n");
    emit_block(out, body, scope.loop_body());
    out.push_str("done\n");
}

pub(super) fn emit_range_bounds(out: &mut String, start: &Expr, end: &Expr, local: bool) {
    if local {
        out.push_str("local __nacre_range_start __nacre_range_end __nacre_range_stop\n");
    }
    out.push_str("__nacre_range_start=");
    emit_expr(out, start);
    out.push('\n');
    out.push_str("__nacre_range_end=");
    emit_expr(out, end);
    out.push('\n');
}

pub(super) fn emit_range_binding(
    out: &mut String,
    name: &str,
    start: &Expr,
    end: &Expr,
    inclusive: bool,
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local -a ");
    } else {
        out.push_str("declare -a ");
    }
    out.push_str(name);
    out.push('\n');
    out.push_str(name);
    out.push_str("=()\n");
    emit_range_bounds(out, start, end, local);
    emit_range_append_loop(out, name, inclusive);
    if readonly {
        out.push_str("readonly -a ");
        out.push_str(name);
        out.push('\n');
    }
}

pub(super) fn emit_range_value(out: &mut String, start: &Expr, end: &Expr, inclusive: bool) {
    out.push_str("$( ");
    out.push_str("__nacre_range_start=");
    emit_expr(out, start);
    out.push_str("; __nacre_range_end=");
    emit_expr(out, end);
    out.push_str("; ");
    emit_range_printf_loop(out, inclusive);
    out.push_str(" )");
}

pub(super) fn emit_range_append_loop(out: &mut String, name: &str, inclusive: bool) {
    out.push_str("if [ \"$__nacre_range_start\" -le \"$__nacre_range_end\" ]; then\n");
    out.push_str("__nacre_range_stop=$__nacre_range_end\n");
    if !inclusive {
        out.push_str("__nacre_range_stop=$((__nacre_range_end - 1))\n");
    }
    out.push_str(
        "for ((__nacre_i=__nacre_range_start; __nacre_i<=__nacre_range_stop; __nacre_i++)); do\n",
    );
    out.push_str(name);
    out.push_str("+=(\"$__nacre_i\")\n");
    out.push_str("done\n");
    out.push_str("else\n");
    out.push_str("__nacre_range_stop=$__nacre_range_end\n");
    if !inclusive {
        out.push_str("__nacre_range_stop=$((__nacre_range_end + 1))\n");
    }
    out.push_str(
        "for ((__nacre_i=__nacre_range_start; __nacre_i>=__nacre_range_stop; __nacre_i--)); do\n",
    );
    out.push_str(name);
    out.push_str("+=(\"$__nacre_i\")\n");
    out.push_str("done\n");
    out.push_str("fi\n");
}

pub(super) fn emit_range_printf_loop(out: &mut String, inclusive: bool) {
    out.push_str("if [ \"$__nacre_range_start\" -le \"$__nacre_range_end\" ]; then ");
    out.push_str("__nacre_range_stop=$__nacre_range_end; ");
    if !inclusive {
        out.push_str("__nacre_range_stop=$((__nacre_range_end - 1)); ");
    }
    out.push_str("for ((__nacre_i=__nacre_range_start; __nacre_i<=__nacre_range_stop; __nacre_i++)); do printf '%s\\n' \"$__nacre_i\"; done; ");
    out.push_str("else ");
    out.push_str("__nacre_range_stop=$__nacre_range_end; ");
    if !inclusive {
        out.push_str("__nacre_range_stop=$((__nacre_range_end + 1)); ");
    }
    out.push_str("for ((__nacre_i=__nacre_range_start; __nacre_i>=__nacre_range_stop; __nacre_i--)); do printf '%s\\n' \"$__nacre_i\"; done; ");
    out.push_str("fi");
}

pub(super) fn emit_for(
    out: &mut String,
    binding: &ForBinding,
    iterable: &Expr,
    body: &Program,
    scope: EmitScope,
) {
    let name = for_binding_iter_name(binding);
    if let (ForBinding::Pattern(pattern), Expr::Array(values)) = (binding, iterable) {
        emit_for_array_literal_pattern(out, pattern, values, body, scope);
        return;
    }
    if let Expr::Range {
        start,
        end,
        inclusive,
    } = iterable
    {
        emit_for_range(out, binding, start, end, *inclusive, body, scope);
        return;
    }
    if let Expr::ArraySliceValue { value, start, end } = iterable {
        emit_array_slice_value_binding(
            out,
            "__nacre_array_value_iter",
            value,
            start,
            end,
            false,
            scope.is_function(),
        );
        emit_for_temp_array(out, binding, name, "__nacre_array_value_iter", body, scope);
        return;
    }
    if let Expr::ArrayTakeValue { value, count } = iterable {
        emit_array_take_value_binding(
            out,
            "__nacre_array_value_iter",
            value,
            count,
            false,
            scope.is_function(),
        );
        emit_for_temp_array(out, binding, name, "__nacre_array_value_iter", body, scope);
        return;
    }
    if let Expr::ArrayDropValue { value, count } = iterable {
        emit_array_drop_value_binding(
            out,
            "__nacre_array_value_iter",
            value,
            count,
            false,
            scope.is_function(),
        );
        emit_for_temp_array(out, binding, name, "__nacre_array_value_iter", body, scope);
        return;
    }
    if let Expr::ArrayReverse(source) = iterable {
        emit_for_array_reverse(out, binding, name, source, body, scope);
        return;
    }
    if let Expr::ArrayReverseValue(source) = iterable {
        emit_for_array_value_transform(
            out,
            binding,
            name,
            source,
            body,
            scope,
            ArrayValueTransform::Reverse,
        );
        return;
    }
    if let Expr::ArraySort(source) = iterable {
        emit_for_array_sort(out, binding, name, source, body, scope);
        return;
    }
    if let Expr::ArraySortValue(source) = iterable {
        emit_for_array_value_transform(
            out,
            binding,
            name,
            source,
            body,
            scope,
            ArrayValueTransform::Sort,
        );
        return;
    }
    if let Expr::ArrayUnique(source) = iterable {
        emit_for_array_unique(out, binding, name, source, body, scope);
        return;
    }
    if let Expr::ArrayUniqueValue(source) = iterable {
        emit_for_array_value_transform(
            out,
            binding,
            name,
            source,
            body,
            scope,
            ArrayValueTransform::Unique,
        );
        return;
    }
    if let Expr::ArrayMap {
        name: source,
        mapper,
    } = iterable
    {
        emit_array_map_binding(
            out,
            "__nacre_array_map_iter",
            source,
            mapper,
            false,
            scope.is_function(),
        );
        emit_for_temp_array(out, binding, name, "__nacre_array_map_iter", body, scope);
        return;
    }
    if let Expr::ArrayMapValue {
        value: source,
        mapper,
    } = iterable
    {
        emit_array_map_value_binding(
            out,
            "__nacre_array_map_iter",
            source,
            mapper,
            false,
            scope.is_function(),
        );
        emit_for_temp_array(out, binding, name, "__nacre_array_map_iter", body, scope);
        return;
    }
    if let Expr::StringSplit {
        name: source,
        separator,
    } = iterable
    {
        out.push_str("while IFS= read -r ");
        out.push_str(name);
        out.push_str("; do\n");
        emit_for_body(out, binding, body, scope);
        out.push_str("done < <(");
        emit_string_split_command(out, source, separator);
        out.push_str(")\n");
        return;
    }
    if let Expr::StringSplitValue { value, separator } = iterable {
        if emit_checked_string_split_value(out, value, separator, scope.is_function()) {
            out.push_str("while IFS= read -r ");
            out.push_str(name);
            out.push_str("; do\n");
            emit_for_body(out, binding, body, scope);
            out.push_str("done < <(");
            emit_string_split_command(out, "__nacre_split_value", separator);
            out.push_str(")\n");
            return;
        }
        out.push_str("while IFS= read -r ");
        out.push_str(name);
        out.push_str("; do\n");
        emit_for_body(out, binding, body, scope);
        out.push_str("done < <(");
        emit_string_split_expr_command(out, value, separator);
        out.push_str(")\n");
        return;
    }
    if let Expr::FsReadLines { path } = iterable {
        out.push_str("while IFS= read -r ");
        out.push_str(name);
        out.push_str(" || [ -n \"$");
        out.push_str(name);
        out.push_str("\" ]");
        out.push_str("; do\n");
        emit_for_body(out, binding, body, scope);
        out.push_str("done < ");
        emit_call_arg(out, path);
        out.push('\n');
        return;
    }
    if let Expr::FsList { path } = iterable {
        out.push_str("while IFS= read -r ");
        out.push_str(name);
        out.push_str("; do\n");
        emit_for_body(out, binding, body, scope);
        out.push_str("done < <(");
        emit_fs_list_command(out, path);
        out.push_str(")\n");
        return;
    }
    if let Expr::Call { name: call, args } = iterable {
        if call == "str.split" {
            out.push_str("while IFS= read -r ");
            out.push_str(name);
            out.push_str("; do\n");
            emit_for_body(out, binding, body, scope);
            out.push_str("done < <(");
            emit_call_command(out, call, args);
            out.push_str(")\n");
            return;
        }
    }

    out.push_str("for ");
    out.push_str(name);
    out.push_str(" in ");
    emit_for_iterable(out, iterable);
    out.push_str("; do\n");
    emit_for_body(out, binding, body, scope);
    out.push_str("done\n");
}

pub(super) fn emit_for_array_literal_pattern(
    out: &mut String,
    pattern: &BindingPattern,
    values: &[Expr],
    body: &Program,
    scope: EmitScope,
) {
    if scope.is_function() {
        out.push_str("local __nacre_for_index\n");
    }
    out.push_str("for ((__nacre_for_index=0; __nacre_for_index<");
    out.push_str(&values.len().to_string());
    out.push_str("; __nacre_for_index++)); do\n");
    out.push_str("case \"$__nacre_for_index\" in\n");
    for (index, value) in values.iter().enumerate() {
        out.push_str(&index.to_string());
        out.push_str(")\n");
        emit_destructure(out, pattern, value, false, scope.is_function());
        out.push_str(";;\n");
    }
    out.push_str("esac\n");
    emit_block(out, body, scope.loop_body());
    out.push_str("done\n");
}

pub(super) fn for_binding_iter_name(binding: &ForBinding) -> &str {
    match binding {
        ForBinding::Name(name) => name,
        ForBinding::Pattern(_) => "__nacre_for_item",
    }
}

pub(super) fn emit_for_body(
    out: &mut String,
    binding: &ForBinding,
    body: &Program,
    scope: EmitScope,
) {
    if let ForBinding::Pattern(pattern) = binding {
        emit_for_item_destructure(out, pattern, scope.is_function());
    }
    emit_block(out, body, scope.loop_body());
}

pub(super) fn emit_for_item_destructure(out: &mut String, pattern: &BindingPattern, local: bool) {
    match pattern {
        BindingPattern::Name(name) => {
            emit_scalar_assignment_prefix(out, name, false, local);
            out.push_str("\"$__nacre_for_item\"\n");
        }
        BindingPattern::Tuple(_) | BindingPattern::Record(_) => {
            emit_destructure(
                out,
                pattern,
                &Expr::Ident("__nacre_for_item".into()),
                false,
                local,
            );
        }
        BindingPattern::Array { .. } => {
            emit_for_item_array_destructure(out, pattern, local);
        }
    }
}

pub(super) fn emit_for_item_array_destructure(
    out: &mut String,
    pattern: &BindingPattern,
    local: bool,
) {
    let BindingPattern::Array { patterns, rest } = pattern else {
        return;
    };
    for (index, pattern) in patterns.iter().enumerate() {
        let temp = format!("__nacre_for_array_field_{index}");
        if local {
            out.push_str("local ");
        }
        out.push_str(&temp);
        out.push_str("=\"$(__nacre_array_field \"$__nacre_for_item\" ");
        out.push_str(&index.to_string());
        out.push_str(")\"\n");
        emit_destructure(out, pattern, &Expr::Ident(temp), false, local);
    }
    if let Some(rest) = rest {
        if rest == "_" {
            return;
        }
        if local {
            out.push_str("local -a ");
            out.push_str(rest);
            out.push('\n');
        }
        out.push_str("eval \"$(__nacre_array_rest_decl \"$__nacre_for_item\" ");
        emit_shell_word(out, rest);
        out.push(' ');
        out.push_str(&patterns.len().to_string());
        out.push_str(")\"\n");
    }
}

pub(super) fn emit_scalar_assignment_prefix(
    out: &mut String,
    name: &str,
    readonly: bool,
    local: bool,
) {
    if name == "_" {
        out.push_str(": ");
        return;
    }
    if local {
        out.push_str("local ");
    } else if readonly {
        out.push_str("readonly ");
    }
    out.push_str(name);
    out.push('=');
}

pub(super) fn emit_for_range(
    out: &mut String,
    binding: &ForBinding,
    start: &Expr,
    end: &Expr,
    inclusive: bool,
    body: &Program,
    scope: EmitScope,
) {
    let name = for_binding_iter_name(binding);
    emit_range_bounds(out, start, end, scope.is_function());
    out.push_str("if [ \"$__nacre_range_start\" -le \"$__nacre_range_end\" ]; then\n");
    out.push_str("__nacre_range_stop=$__nacre_range_end\n");
    if !inclusive {
        out.push_str("__nacre_range_stop=$((__nacre_range_end - 1))\n");
    }
    out.push_str("for ((");
    out.push_str(name);
    out.push_str("=__nacre_range_start; ");
    out.push_str(name);
    out.push_str("<=__nacre_range_stop; ");
    out.push_str(name);
    out.push_str("++)); do\n");
    emit_for_body(out, binding, body, scope);
    out.push_str("done\n");
    out.push_str("else\n");
    out.push_str("__nacre_range_stop=$__nacre_range_end\n");
    if !inclusive {
        out.push_str("__nacre_range_stop=$((__nacre_range_end + 1))\n");
    }
    out.push_str("for ((");
    out.push_str(name);
    out.push_str("=__nacre_range_start; ");
    out.push_str(name);
    out.push_str(">=__nacre_range_stop; ");
    out.push_str(name);
    out.push_str("--)); do\n");
    emit_for_body(out, binding, body, scope);
    out.push_str("done\n");
    out.push_str("fi\n");
}

pub(super) fn emit_for_temp_array(
    out: &mut String,
    binding: &ForBinding,
    name: &str,
    source: &str,
    body: &Program,
    scope: EmitScope,
) {
    out.push_str("for ");
    out.push_str(name);
    out.push_str(" in \"${");
    out.push_str(source);
    out.push_str("[@]}\"; do\n");
    emit_for_body(out, binding, body, scope);
    out.push_str("done\n");
    out.push_str("unset ");
    out.push_str(source);
    out.push('\n');
}

pub(super) fn emit_for_array_reverse(
    out: &mut String,
    binding: &ForBinding,
    name: &str,
    source: &str,
    body: &Program,
    scope: EmitScope,
) {
    if scope.is_function() {
        out.push_str("local -a __nacre_reverse_iter\n");
    }
    out.push_str("__nacre_reverse_iter=()\n");
    out.push_str("for ((__nacre_i=${#");
    out.push_str(source);
    out.push_str("[@]} - 1; __nacre_i >= 0; __nacre_i--)); do\n");
    out.push_str("__nacre_reverse_iter+=(\"${");
    out.push_str(source);
    out.push_str("[$__nacre_i]}\")\n");
    out.push_str("done\n");
    out.push_str("for ");
    out.push_str(name);
    out.push_str(" in \"${__nacre_reverse_iter[@]}\"; do\n");
    emit_for_body(out, binding, body, scope);
    out.push_str("done\n");
    out.push_str("unset __nacre_reverse_iter\n");
}

pub(super) fn emit_for_array_sort(
    out: &mut String,
    binding: &ForBinding,
    name: &str,
    source: &str,
    body: &Program,
    scope: EmitScope,
) {
    if scope.is_function() {
        out.push_str("local -a __nacre_sort_iter\n");
    }
    out.push_str("__nacre_sort_iter=()\n");
    out.push_str("if [ \"${#");
    out.push_str(source);
    out.push_str("[@]}\" -gt 0 ]; then\n");
    out.push_str("mapfile -t __nacre_sort_iter < <(printf '%s\\n' \"${");
    out.push_str(source);
    out.push_str("[@]}\" | sort)\n");
    out.push_str("fi\n");
    out.push_str("for ");
    out.push_str(name);
    out.push_str(" in \"${__nacre_sort_iter[@]}\"; do\n");
    emit_for_body(out, binding, body, scope);
    out.push_str("done\n");
    out.push_str("unset __nacre_sort_iter\n");
}

pub(super) fn emit_for_array_unique(
    out: &mut String,
    binding: &ForBinding,
    name: &str,
    source: &str,
    body: &Program,
    scope: EmitScope,
) {
    if scope.is_function() {
        out.push_str("local -a __nacre_unique_iter\n");
    }
    emit_array_unique_to(out, "__nacre_unique_iter", source);
    out.push_str("for ");
    out.push_str(name);
    out.push_str(" in \"${__nacre_unique_iter[@]}\"; do\n");
    emit_for_body(out, binding, body, scope);
    out.push_str("done\n");
    out.push_str("unset __nacre_unique_iter\n");
}

#[derive(Clone, Copy)]
pub(super) enum ArrayValueTransform {
    Reverse,
    Sort,
    Unique,
}

pub(super) fn emit_for_array_value_transform(
    out: &mut String,
    binding: &ForBinding,
    name: &str,
    source: &Expr,
    body: &Program,
    scope: EmitScope,
    transform: ArrayValueTransform,
) {
    match transform {
        ArrayValueTransform::Reverse => {
            emit_array_reverse_value_binding(
                out,
                "__nacre_array_value_iter",
                source,
                false,
                scope.is_function(),
            );
        }
        ArrayValueTransform::Sort => {
            emit_array_sort_value_binding(
                out,
                "__nacre_array_value_iter",
                source,
                false,
                scope.is_function(),
            );
        }
        ArrayValueTransform::Unique => {
            emit_array_unique_value_binding(
                out,
                "__nacre_array_value_iter",
                source,
                false,
                scope.is_function(),
            );
        }
    }
    out.push_str("for ");
    out.push_str(name);
    out.push_str(" in \"${__nacre_array_value_iter[@]}\"; do\n");
    emit_for_body(out, binding, body, scope);
    out.push_str("done\n");
    out.push_str("unset __nacre_array_value_iter\n");
}

pub(super) fn emit_for_iterable(out: &mut String, iterable: &Expr) {
    match iterable {
        Expr::Ident(name) => {
            out.push_str("\"${");
            out.push_str(name);
            out.push_str("[@]}\"");
        }
        Expr::ProcessArgs => out.push_str("\"${args[@]}\""),
        Expr::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    out.push(' ');
                }
                emit_array_element(out, value);
            }
        }
        Expr::Range {
            start,
            end,
            inclusive,
        } => emit_range_value(out, start, end, *inclusive),
        Expr::Slice { name, start, end } => emit_array_slice_elements(out, name, start, end),
        Expr::ArraySliceValue { value, start, end } => {
            emit_array_slice_value_expr(out, value, start, end)
        }
        Expr::ArrayTake { name, count } => emit_array_take_elements(out, name, count),
        Expr::ArrayTakeValue { value, count } => emit_array_take_value_expr(out, value, count),
        Expr::ArrayDrop { name, count } => emit_array_drop_elements(out, name, count),
        Expr::ArrayDropValue { value, count } => emit_array_drop_value_expr(out, value, count),
        Expr::ArrayReverse(name) => emit_array_reverse_value(out, name),
        Expr::ArrayReverseValue(value) => emit_array_reverse_value_expr(out, value),
        Expr::ArraySort(name) => emit_array_sort_value(out, name),
        Expr::ArraySortValue(value) => emit_array_sort_value_expr(out, value),
        Expr::ArrayUnique(name) => emit_array_unique_value(out, name),
        Expr::ArrayUniqueValue(value) => emit_array_unique_value_expr(out, value),
        Expr::ArrayMap { name, mapper } => emit_array_map_value(out, name, mapper),
        Expr::ArrayMapValue { value, mapper } => emit_array_map_value_expr(out, value, mapper),
        Expr::MapKeys(name) => {
            out.push_str("\"${!");
            out.push_str(name);
            out.push_str("[@]}\"");
        }
        Expr::MapValues(name) => {
            out.push_str("\"${");
            out.push_str(name);
            out.push_str("[@]}\"");
        }
        Expr::FsReadLines { path } => {
            out.push_str("$(cat ");
            emit_call_arg(out, path);
            out.push(')');
        }
        Expr::FsList { path } => {
            out.push_str("$(find ");
            emit_call_arg(out, path);
            out.push_str(" -mindepth 1 -maxdepth 1 -print)");
        }
        _ => emit_expr(out, iterable),
    }
}

pub(super) fn emit_block(out: &mut String, program: &Program, scope: EmitScope) {
    emit_block_with_defers(out, program, scope, &[]);
}

pub(super) fn emit_block_with_defers(
    out: &mut String,
    program: &Program,
    scope: EmitScope,
    inherited_defers: &[Statement],
) {
    let mut local_defers = Vec::new();
    for statement in program.statements() {
        if let Statement::Defer(statement) = statement {
            local_defers.push(statement.as_ref().clone());
            continue;
        }
        let mut active_defers = Vec::with_capacity(inherited_defers.len() + local_defers.len());
        active_defers.extend_from_slice(inherited_defers);
        active_defers.extend(local_defers.iter().cloned());
        emit_statement(out, statement, scope, &active_defers);
    }
    emit_deferred_statements(out, &local_defers, scope);
}

pub(super) fn emit_deferred_statements(out: &mut String, defers: &[Statement], scope: EmitScope) {
    for statement in defers.iter().rev() {
        emit_statement(out, statement, scope, &[]);
    }
}
