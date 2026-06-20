mod arithmetic;
mod local_mangling;
mod match_analysis;
mod matching;
mod quoting;
mod runtime;
mod shell;
mod value_layout;

use arithmetic::*;
use matching::*;
use quoting::*;
use runtime::*;
use shell::*;
use value_layout::*;

use crate::lowering::lower_method_calls;
use crate::policy::ExecutionPolicy;
use crate::{BindingPattern, ClosureCapture, Expr, Param, Program, Statement, Type};

#[cfg(test)]
pub(crate) fn transpile(program: &Program) -> String {
    transpile_with_policy(program, &ExecutionPolicy::deny_all())
}

pub(crate) fn transpile_with_policy(program: &Program, policy: &ExecutionPolicy) -> String {
    let program = lower_method_calls(program);
    let program = local_mangling::mangle_function_locals(&program);
    let mut out = String::from("#!/usr/bin/env bash\nset -euo pipefail\nargs=(\"$@\")\n");
    emit_policy_runtime(&mut out, policy);
    if program_needs_runtime(&program) {
        out.push_str(CLOSURE_RUNTIME);
    }
    for statement in program.statements() {
        out.push('\n');
        emit_statement(&mut out, statement, EmitScope::TopLevel);
    }
    out
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum EmitScope {
    TopLevel,
    Function,
}

impl EmitScope {
    fn is_function(self) -> bool {
        matches!(self, Self::Function)
    }
}

fn emit_statement(out: &mut String, statement: &Statement, scope: EmitScope) {
    match statement {
        Statement::Use { path } => emit_use(out, path),
        Statement::ExternalFunction { .. } => {}
        Statement::Trait { .. } => {}
        Statement::Impl { methods, .. } => {
            for method in methods {
                emit_function(out, &method.name, &method.params, &method.body);
            }
        }
        Statement::TypeAlias { .. } => {}
        Statement::SumType { .. } => {}
        Statement::Newtype { .. } => {}
        Statement::Function {
            name, params, body, ..
        } => emit_function(out, name, params, body),
        Statement::Const { name, expr, .. } => {
            emit_binding(out, name, expr, true, scope.is_function());
        }
        Statement::Let { name, expr, .. } => {
            emit_binding(out, name, expr, false, scope.is_function());
        }
        Statement::Destructure {
            mutable,
            pattern,
            expr,
        } => emit_destructure(out, pattern, expr, !mutable, scope.is_function()),
        Statement::Assign { name, expr } => {
            emit_assignment(out, name, expr);
        }
        Statement::Expr(expr) => emit_expr_statement(out, expr),
        Statement::TryCommand(command) => {
            emit_shell_command(out, command);
            out.push_str(" || exit $?\n");
        }
        Statement::TryCommandResult(command) => emit_try_command_result(out, command, scope),
        Statement::TryResult(expr) => emit_try_result(out, expr, scope),
        Statement::TryPipeline { input, commands } => {
            emit_pipeline(out, input.as_deref(), commands);
            out.push_str(" || exit $?\n");
        }
        Statement::TryPipelineResult { input, commands } => {
            let mut command = String::new();
            emit_pipeline(&mut command, input.as_deref(), commands);
            emit_try_command_result(out, &command, scope);
        }
        Statement::Command(command) => {
            out.push_str(command);
            out.push('\n');
        }
        Statement::Redirect {
            command,
            target,
            stderr,
            append,
        } => emit_redirect(out, command, target, stderr.as_deref(), *append),
        Statement::Require { command, version } => emit_require(out, command, version.as_deref()),
        Statement::RequireOneOf { commands } => emit_require_one_of(out, commands),
        Statement::Block { body } => emit_block(out, body, scope),
        Statement::If {
            condition,
            then_branch,
            else_branch,
        } => emit_if(out, condition, then_branch, else_branch.as_ref(), scope),
        Statement::While { condition, body } => emit_while(out, condition, body, scope),
        Statement::For {
            name,
            iterable,
            body,
        } => emit_for(out, name, iterable, body, scope),
        Statement::Break => out.push_str("break\n"),
        Statement::Continue => out.push_str("continue\n"),
        Statement::Return(expr) => emit_return(out, expr),
        Statement::Raw(raw) => out.push_str(raw),
    }
}

fn emit_expr_statement(out: &mut String, expr: &Expr) {
    match expr {
        Expr::ArrayPush { name, value } => emit_array_push(out, name, value),
        Expr::ArrayPop { name } => emit_array_pop(out, name),
        Expr::MapSet { name, key, value } => emit_map_set(out, name, key, value),
        Expr::MapRemove { name, key } => emit_map_remove(out, name, key),
        Expr::FsWriteLines { path, lines } => emit_fs_write_lines_statement(out, path, lines),
        Expr::FsAppendLines { path, lines } => emit_fs_append_lines_statement(out, path, lines),
        Expr::AllowedCommand {
            program,
            args,
            read_args,
            write_args,
            ..
        } => {
            emit_allowed_command(out, program.as_deref(), args, read_args, write_args, false);
            out.push('\n');
        }
        Expr::Call { name, args } => {
            emit_call_head(out, name);
            for arg in args {
                out.push(' ');
                emit_call_arg(out, arg);
            }
            out.push('\n');
        }
        _ => {
            emit_expr(out, expr);
            out.push('\n');
        }
    }
}

fn emit_use(out: &mut String, path: &[String]) {
    out.push_str("source \"$(dirname \"$0\")/");
    out.push_str(&path.join("/"));
    out.push_str(".sh\"\n");
}

fn emit_function(out: &mut String, name: &str, params: &[Param], body: &Program) {
    let shell_name = shell_function_name(name);
    out.push_str(&shell_name);
    out.push_str("() {\n");
    let mut position = 1;
    for param in params {
        if param.capture_name.is_some() {
            continue;
        }
        if param.variadic {
            out.push_str("local -a ");
            out.push_str(&param.name);
            out.push_str("=(\"${@:");
            out.push_str(&position.to_string());
            out.push_str("}\")\n");
        } else if let Some(default) = &param.default {
            out.push_str("if [ \"$#\" -ge ");
            out.push_str(&position.to_string());
            out.push_str(" ]; then\nlocal ");
            out.push_str(&param.name);
            out.push_str("=\"$");
            out.push_str(&position.to_string());
            out.push_str("\"\nelse\nlocal ");
            out.push_str(&param.name);
            out.push('=');
            emit_expr(out, default);
            out.push_str("\nfi\n");
        } else {
            out.push_str("local ");
            out.push_str(&param.name);
            out.push_str("=\"$");
            out.push_str(&position.to_string());
            out.push_str("\"\n");
        }
        position += 1;
    }
    emit_block(out, body, EmitScope::Function);
    out.push_str("}\n");
    if is_shell_name(name) {
        out.push_str("readonly ");
        out.push_str(name);
        out.push('=');
        emit_shell_word(out, &shell_name);
        out.push('\n');
    }
}

fn shell_function_name(name: &str) -> String {
    if is_bash_reserved_word(name) {
        format!("__nacre_keyword_{name}")
    } else {
        name.to_string()
    }
}

fn is_bash_reserved_word(name: &str) -> bool {
    matches!(
        name,
        "if" | "then"
            | "else"
            | "elif"
            | "fi"
            | "case"
            | "esac"
            | "for"
            | "select"
            | "while"
            | "until"
            | "do"
            | "done"
            | "in"
            | "function"
            | "time"
            | "coproc"
    )
}

fn is_shell_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn emit_redirect(
    out: &mut String,
    command: &str,
    target: &str,
    stderr: Option<&str>,
    append: bool,
) {
    emit_shell_command(out, command);
    if append {
        out.push_str(" >> ");
    } else {
        out.push_str(" > ");
    }
    emit_string(out, target);
    if let Some(stderr) = stderr {
        if append {
            out.push_str(" 2>> ");
        } else {
            out.push_str(" 2> ");
        }
        emit_string(out, stderr);
    }
    out.push('\n');
}

fn emit_return(out: &mut String, expr: &Expr) {
    if let Expr::TryResult(value) = expr {
        emit_return_try_result(out, value);
        return;
    }
    out.push_str("local __nacre_return_value\n__nacre_return_value=");
    emit_expr(out, expr);
    out.push_str("\nprintf '%s\\n' \"$__nacre_return_value\"\n");
    out.push_str("return 0\n");
}

fn emit_try_result_binding(out: &mut String, name: &str, expr: &Expr, readonly: bool, local: bool) {
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

fn emit_return_try_result(out: &mut String, expr: &Expr) {
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

fn emit_try_result_value(out: &mut String, expr: &Expr) {
    out.push_str("$(__nacre_try_result=");
    emit_expr(out, expr);
    out.push_str("; case \"$__nacre_try_result\" in 1*) printf '%s' \"${__nacre_try_result#?}\" ;; 0*) printf '%s' \"$__nacre_try_result\"; return 0 ;; esac)");
}

fn emit_try_command_result(out: &mut String, command: &str, scope: EmitScope) {
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

fn emit_try_result(out: &mut String, expr: &Expr, scope: EmitScope) {
    if scope.is_function() {
        out.push_str("local ");
    }
    out.push_str("__nacre_try_result=");
    emit_expr(out, expr);
    out.push('\n');
    out.push_str("case \"$__nacre_try_result\" in 1*) : ;; 0*) printf '%s\\n' \"$__nacre_try_result\"; return 0 ;; esac\n");
}

fn emit_if(
    out: &mut String,
    condition: &Expr,
    then_branch: &Program,
    else_branch: Option<&Program>,
    scope: EmitScope,
) {
    out.push_str("if ");
    emit_condition(out, condition);
    out.push_str("; then\n");
    emit_block(out, then_branch, scope);
    if let Some(else_branch) = else_branch {
        out.push_str("else\n");
        emit_block(out, else_branch, scope);
    }
    out.push_str("fi\n");
}

fn emit_while(out: &mut String, condition: &Expr, body: &Program, scope: EmitScope) {
    out.push_str("while ");
    emit_condition(out, condition);
    out.push_str("; do\n");
    emit_block(out, body, scope);
    out.push_str("done\n");
}

fn emit_for(out: &mut String, name: &str, iterable: &Expr, body: &Program, scope: EmitScope) {
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
        emit_for_temp_array(out, name, "__nacre_array_value_iter", body, scope);
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
        emit_for_temp_array(out, name, "__nacre_array_value_iter", body, scope);
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
        emit_for_temp_array(out, name, "__nacre_array_value_iter", body, scope);
        return;
    }
    if let Expr::ArrayReverse(source) = iterable {
        emit_for_array_reverse(out, name, source, body, scope);
        return;
    }
    if let Expr::ArrayReverseValue(source) = iterable {
        emit_for_array_value_transform(
            out,
            name,
            source,
            body,
            scope,
            ArrayValueTransform::Reverse,
        );
        return;
    }
    if let Expr::ArraySort(source) = iterable {
        emit_for_array_sort(out, name, source, body, scope);
        return;
    }
    if let Expr::ArraySortValue(source) = iterable {
        emit_for_array_value_transform(out, name, source, body, scope, ArrayValueTransform::Sort);
        return;
    }
    if let Expr::ArrayUnique(source) = iterable {
        emit_for_array_unique(out, name, source, body, scope);
        return;
    }
    if let Expr::ArrayUniqueValue(source) = iterable {
        emit_for_array_value_transform(out, name, source, body, scope, ArrayValueTransform::Unique);
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
        emit_for_temp_array(out, name, "__nacre_array_map_iter", body, scope);
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
        emit_for_temp_array(out, name, "__nacre_array_map_iter", body, scope);
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
        emit_block(out, body, scope);
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
            emit_block(out, body, scope);
            out.push_str("done < <(");
            emit_string_split_command(out, "__nacre_split_value", separator);
            out.push_str(")\n");
            return;
        }
        out.push_str("while IFS= read -r ");
        out.push_str(name);
        out.push_str("; do\n");
        emit_block(out, body, scope);
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
        emit_block(out, body, scope);
        out.push_str("done < ");
        emit_call_arg(out, path);
        out.push('\n');
        return;
    }
    if let Expr::FsList { path } = iterable {
        out.push_str("while IFS= read -r ");
        out.push_str(name);
        out.push_str("; do\n");
        emit_block(out, body, scope);
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
            emit_block(out, body, scope);
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
    emit_block(out, body, scope);
    out.push_str("done\n");
}

fn emit_for_temp_array(
    out: &mut String,
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
    emit_block(out, body, scope);
    out.push_str("done\n");
    out.push_str("unset ");
    out.push_str(source);
    out.push('\n');
}

fn emit_for_array_reverse(
    out: &mut String,
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
    emit_block(out, body, scope);
    out.push_str("done\n");
    out.push_str("unset __nacre_reverse_iter\n");
}

fn emit_for_array_sort(
    out: &mut String,
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
    emit_block(out, body, scope);
    out.push_str("done\n");
    out.push_str("unset __nacre_sort_iter\n");
}

fn emit_for_array_unique(
    out: &mut String,
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
    emit_block(out, body, scope);
    out.push_str("done\n");
    out.push_str("unset __nacre_unique_iter\n");
}

#[derive(Clone, Copy)]
enum ArrayValueTransform {
    Reverse,
    Sort,
    Unique,
}

fn emit_for_array_value_transform(
    out: &mut String,
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
    emit_block(out, body, scope);
    out.push_str("done\n");
    out.push_str("unset __nacre_array_value_iter\n");
}

fn emit_for_iterable(out: &mut String, iterable: &Expr) {
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

fn emit_block(out: &mut String, program: &Program, scope: EmitScope) {
    for statement in program.statements() {
        emit_statement(out, statement, scope);
    }
}

fn emit_require_one_of(out: &mut String, commands: &[String]) {
    for (index, command) in commands.iter().enumerate() {
        if index > 0 {
            out.push_str(" || ");
        }
        out.push_str("command -v ");
        emit_shell_word(out, command);
        out.push_str(" >/dev/null 2>&1");
    }
    out.push_str(" || { printf ");
    emit_shell_word(
        out,
        &format!(
            "required one of commands not found: {}\\n",
            commands.join(", ")
        ),
    );
    out.push_str(" >&2; exit 127; }\n");
}

fn emit_require(out: &mut String, command: &str, version: Option<&str>) {
    out.push_str("command -v ");
    emit_shell_word(out, command);
    out.push_str(" >/dev/null 2>&1 || { printf ");
    emit_shell_word(out, &format!("required command not found: {command}\\n"));
    out.push_str(" >&2; exit 127; }\n");

    let Some(version) = version else {
        return;
    };

    out.push_str("__nacre_version=\"$(");
    emit_shell_word(out, command);
    out.push_str(" --version 2>/dev/null | head -n 1 || true)\"\n");
    out.push_str("awk -v actual=\"$__nacre_version\" -v required=");
    emit_shell_word(out, version);
    out.push(' ');
    emit_shell_word(out, REQUIRE_VERSION_AWK);
    out.push_str(" || { printf ");
    emit_shell_word(
        out,
        &format!("required command version not satisfied: {command} {version}\\n"),
    );
    out.push_str(" >&2; exit 127; }\n");
}

const REQUIRE_VERSION_AWK: &str = r#"function first_version(s) {
  if (match(s, /[0-9]+([.][0-9]+)*/)) return substr(s, RSTART, RLENGTH)
  return ""
}
function cmp(a, b, aa, bb, an, bn, n, i, av, bv) {
  an = split(a, aa, ".")
  bn = split(b, bb, ".")
  n = an > bn ? an : bn
  for (i = 1; i <= n; i++) {
    av = (i <= an) ? aa[i] + 0 : 0
    bv = (i <= bn) ? bb[i] + 0 : 0
    if (av < bv) return -1
    if (av > bv) return 1
  }
  return 0
}
BEGIN {
  req = required
  if (req ~ /^(>=|<=|==|=|>|<)[[:space:]]*/) {
    if (req ~ /^>=/) op = ">="
    else if (req ~ /^<=/) op = "<="
    else if (req ~ /^==/) op = "=="
    else if (req ~ /^>/) op = ">"
    else if (req ~ /^</) op = "<"
    else op = "="
    sub(/^(>=|<=|==|=|>|<)[[:space:]]*/, "", req)
    actual_v = first_version(actual)
    req_v = first_version(req)
    if (actual_v == "" || req_v == "") exit 1
    c = cmp(actual_v, req_v)
    if (op == ">=") exit (c >= 0 ? 0 : 1)
    if (op == "<=") exit (c <= 0 ? 0 : 1)
    if (op == ">" ) exit (c > 0 ? 0 : 1)
    if (op == "<" ) exit (c < 0 ? 0 : 1)
    exit (c == 0 ? 0 : 1)
  }
  exit (index(actual, req) > 0 ? 0 : 1)
}"#;

fn emit_binding(out: &mut String, name: &str, expr: &Expr, readonly: bool, local: bool) {
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
            read_args,
            write_args,
            ..
        } => {
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
        Expr::Await(future) => emit_await_binding(out, name, future, readonly, local),
        _ => {
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

fn emit_assignment(out: &mut String, name: &str, expr: &Expr) {
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
            out.push_str(name);
            out.push('=');
            emit_bound_expr(out, expr);
        }
    }
}

fn emit_destructure(
    out: &mut String,
    pattern: &BindingPattern,
    expr: &Expr,
    readonly: bool,
    local: bool,
) {
    match pattern {
        BindingPattern::Array { names, rest } => {
            for (index, name) in names.iter().enumerate() {
                let value = destructured_array_value(expr, index);
                emit_binding(out, name, &value, readonly, local);
            }
            if let Some(rest) = rest {
                emit_array_rest_binding(out, rest, expr, names.len(), readonly, local);
            }
        }
        BindingPattern::Tuple(names) => {
            for (index, name) in names.iter().enumerate() {
                let value = destructured_tuple_value(expr, index + 1);
                emit_binding(out, name, &value, readonly, local);
            }
        }
        BindingPattern::Record(bindings) => {
            for (field, name) in bindings {
                let value = destructured_record_value(expr, field);
                emit_binding(out, name, &value, readonly, local);
            }
        }
    }
}

fn destructured_array_value(expr: &Expr, index: usize) -> Expr {
    match expr {
        Expr::Array(values) => values.get(index).cloned().unwrap_or(Expr::Unit),
        Expr::Ident(name) => Expr::Index {
            name: name.clone(),
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
        _ => Expr::Index {
            name: destructure_source_name(expr),
            index: Box::new(Expr::Int(index as i64)),
        },
    }
}

fn emit_array_rest_binding(
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

fn destructured_tuple_value(expr: &Expr, field: usize) -> Expr {
    match expr {
        Expr::Tuple(values) => values[field - 1].clone(),
        Expr::Ident(name) => Expr::TupleField {
            name: name.clone(),
            field,
        },
        _ => Expr::TupleField {
            name: destructure_source_name(expr),
            field,
        },
    }
}

fn destructured_record_value(expr: &Expr, field: &str) -> Expr {
    match expr {
        Expr::Record(fields) => fields
            .iter()
            .find(|(name, _)| name == field)
            .map(|(_, value)| value.clone())
            .unwrap_or_else(|| Expr::Unit),
        Expr::Ident(name) => Expr::Field {
            name: name.clone(),
            field: field.to_string(),
        },
        _ => Expr::Field {
            name: destructure_source_name(expr),
            field: field.to_string(),
        },
    }
}

fn destructure_source_name(expr: &Expr) -> String {
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

fn emit_discard_expr(out: &mut String, expr: &Expr) {
    match expr {
        Expr::MatchGuardResult(value) => emit_discard_expr(out, value),
        Expr::AllowedCommand {
            program,
            args,
            read_args,
            write_args,
            ..
        } => {
            emit_allowed_command(out, program.as_deref(), args, read_args, write_args, false);
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
        Expr::Closure { name, captures } => emit_closure(out, name, captures),
        Expr::Lambda { .. } => unreachable!("lambdas are lowered before emission"),
    }
}

fn emit_async_binding(out: &mut String, name: &str, command: &str, readonly: bool, local: bool) {
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

fn emit_await_binding(out: &mut String, name: &str, future: &str, readonly: bool, local: bool) {
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
    out.push_str("rm -f \"$");
    out.push_str(future);
    out.push_str("_out\"\n");
    out.push_str("else\n__nacre_status=$?\nrm -f \"$");
    out.push_str(future);
    out.push_str("_out\"\nexit $__nacre_status\nfi\n");
    if readonly && !local {
        out.push_str("readonly ");
        out.push_str(name);
        out.push('\n');
    }
}

fn emit_discard_await(out: &mut String, future: &str) {
    out.push_str("if wait \"$");
    out.push_str(future);
    out.push_str("_pid\"; then\nrm -f \"$");
    out.push_str(future);
    out.push_str("_out\"\nelse\n__nacre_status=$?\nrm -f \"$");
    out.push_str(future);
    out.push_str("_out\"\nexit $__nacre_status\nfi\n");
}

fn emit_bound_expr(out: &mut String, expr: &Expr) {
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

fn emit_condition(out: &mut String, expr: &Expr) {
    match expr {
        Expr::Binary { .. } | Expr::Not(_) => emit_awk_condition(out, expr),
        _ => emit_expr(out, expr),
    }
}

fn emit_expr(out: &mut String, expr: &Expr) {
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
        Expr::Closure { name, captures } => emit_closure(out, name, captures),
        Expr::Lambda { .. } => unreachable!("lambdas are lowered before emission"),
    }
}

fn emit_allowed_command(
    out: &mut String,
    program: Option<&str>,
    args: &[Expr],
    read_args: &[usize],
    write_args: &[usize],
    capture: bool,
) {
    if capture {
        out.push_str("\"$(");
    }
    let Some(program) = program else {
        out.push_str("printf 'nacre: unresolved command policy\\n' >&2; exit 126");
        if capture {
            out.push_str(")\"");
        }
        return;
    };

    for (index, arg) in args.iter().enumerate() {
        out.push_str("__nacre_run_arg_");
        out.push_str(&index.to_string());
        out.push('=');
        emit_expr(out, arg);
        out.push_str("; ");
    }
    for index in read_args {
        out.push_str("__nacre_run_arg_");
        out.push_str(&index.to_string());
        out.push_str("=\"$(__nacre_checked_path read \"$__nacre_run_arg_");
        out.push_str(&index.to_string());
        out.push_str("\")\" || exit $?; ");
    }
    for index in write_args {
        out.push_str("__nacre_run_arg_");
        out.push_str(&index.to_string());
        out.push_str("=\"$(__nacre_checked_path write \"$__nacre_run_arg_");
        out.push_str(&index.to_string());
        out.push_str("\")\" || exit $?; ");
    }
    emit_bash_string(out, program);
    for index in 0..args.len() {
        out.push_str(" \"$__nacre_run_arg_");
        out.push_str(&index.to_string());
        out.push('"');
    }
    if capture {
        out.push_str(")\"");
    }
}

fn emit_bash_arithmetic(out: &mut String, expr: &Expr) {
    out.push_str("$((");
    emit_bash_arith_expr(out, expr);
    out.push_str("))");
}

fn emit_bash_arith_expr(out: &mut String, expr: &Expr) {
    match expr {
        Expr::Int(value) => out.push_str(&value.to_string()),
        Expr::Ident(name) => out.push_str(name),
        Expr::Value(name) => out.push_str(name),
        Expr::Len(name) => {
            out.push_str("${#");
            out.push_str(name);
            out.push_str("[@]}");
        }
        Expr::ArrayLenValue(_) => emit_expr(out, expr),
        Expr::IsEmpty(_) => emit_expr(out, expr),
        Expr::ArrayIsEmptyValue(_) => emit_expr(out, expr),
        Expr::ArrayFirst(_) => emit_expr(out, expr),
        Expr::ArrayFirstValue(_) => emit_expr(out, expr),
        Expr::ArrayLast(_) => emit_expr(out, expr),
        Expr::ArrayLastValue(_) => emit_expr(out, expr),
        Expr::ArrayReverse(_) => emit_expr(out, expr),
        Expr::ArrayReverseValue(_) => emit_expr(out, expr),
        Expr::ArraySort(_) => emit_expr(out, expr),
        Expr::ArraySortValue(_) => emit_expr(out, expr),
        Expr::ArrayUnique(_) => emit_expr(out, expr),
        Expr::ArrayUniqueValue(_) => emit_expr(out, expr),
        Expr::ArrayMap { .. } => emit_expr(out, expr),
        Expr::ArrayMapValue { .. } => emit_expr(out, expr),
        Expr::OptionMap { .. } => emit_expr(out, expr),
        Expr::OptionMapValue { .. } => emit_expr(out, expr),
        Expr::OptionFlatMap { .. } => emit_expr(out, expr),
        Expr::OptionFlatMapValue { .. } => emit_expr(out, expr),
        Expr::ResultMap { .. } => emit_expr(out, expr),
        Expr::ResultMapValue { .. } => emit_expr(out, expr),
        Expr::ResultFlatMap { .. } => emit_expr(out, expr),
        Expr::ResultFlatMapValue { .. } => emit_expr(out, expr),
        Expr::OptionAp { .. } => emit_expr(out, expr),
        Expr::OptionApValue { .. } => emit_expr(out, expr),
        Expr::ResultAp { .. } => emit_expr(out, expr),
        Expr::ResultApValue { .. } => emit_expr(out, expr),
        Expr::OptionOrElse { .. } => emit_expr(out, expr),
        Expr::OptionOrElseValue { .. } => emit_expr(out, expr),
        Expr::ArrayTake { .. } => emit_expr(out, expr),
        Expr::ArrayTakeValue { .. } => emit_expr(out, expr),
        Expr::ArrayDrop { .. } => emit_expr(out, expr),
        Expr::ArrayDropValue { .. } => emit_expr(out, expr),
        Expr::Join { .. } => emit_expr(out, expr),
        Expr::JoinValue { .. } => emit_expr(out, expr),
        Expr::ArrayPush { .. } => emit_expr(out, expr),
        Expr::ArrayPop { .. } => emit_expr(out, expr),
        Expr::ArrayContains { .. } => emit_expr(out, expr),
        Expr::ArrayContainsValue { .. } => emit_expr(out, expr),
        Expr::ArrayIndexOf { .. } => emit_expr(out, expr),
        Expr::ArrayIndexOfValue { .. } => emit_expr(out, expr),
        Expr::Slice { .. } => emit_expr(out, expr),
        Expr::ArraySliceValue { .. } => emit_expr(out, expr),
        Expr::MapKeys(_)
        | Expr::MapKeysValue(_)
        | Expr::MapValues(_)
        | Expr::MapValuesValue(_)
        | Expr::MapHas { .. }
        | Expr::MapHasValue { .. } => emit_expr(out, expr),
        Expr::StringContains { .. } => emit_expr(out, expr),
        Expr::StringContainsValue { .. } => emit_expr(out, expr),
        Expr::StringIndexOf { .. } => emit_expr(out, expr),
        Expr::StringIndexOfValue { .. } => emit_expr(out, expr),
        Expr::StringStartsWith { .. } => emit_expr(out, expr),
        Expr::StringStartsWithValue { .. } => emit_expr(out, expr),
        Expr::StringEndsWith { .. } => emit_expr(out, expr),
        Expr::StringEndsWithValue { .. } => emit_expr(out, expr),
        Expr::StringLen(_) => emit_expr(out, expr),
        Expr::StringLenValue(_) => emit_expr(out, expr),
        Expr::StringIsEmpty(_) => emit_expr(out, expr),
        Expr::StringIsEmptyValue(_) => emit_expr(out, expr),
        Expr::StringSlice { .. } => emit_expr(out, expr),
        Expr::StringSliceValue { .. } => emit_expr(out, expr),
        Expr::StringTrim(_) => emit_expr(out, expr),
        Expr::StringTrimValue(_) => emit_expr(out, expr),
        Expr::StringTrimStart(_) => emit_expr(out, expr),
        Expr::StringTrimStartValue(_) => emit_expr(out, expr),
        Expr::StringTrimEnd(_) => emit_expr(out, expr),
        Expr::StringTrimEndValue(_) => emit_expr(out, expr),
        Expr::StringToUpper(_) => emit_expr(out, expr),
        Expr::StringToUpperValue(_) => emit_expr(out, expr),
        Expr::StringToLower(_) => emit_expr(out, expr),
        Expr::StringToLowerValue(_) => emit_expr(out, expr),
        Expr::StringRepeat { .. } => emit_expr(out, expr),
        Expr::StringRepeatValue { .. } => emit_expr(out, expr),
        Expr::StringSplit { .. } => emit_expr(out, expr),
        Expr::StringReplace { .. } => emit_expr(out, expr),
        Expr::StringReplaceValue { .. } => emit_expr(out, expr),
        Expr::Index { name, index } => {
            out.push_str("${");
            out.push_str(name);
            out.push('[');
            emit_index_expr(out, index);
            out.push_str("]}");
        }
        Expr::IndexValue { .. } => emit_expr(out, expr),
        Expr::TupleField { name, field } => {
            out.push_str("${");
            out.push_str(name);
            out.push('_');
            out.push_str(&field.to_string());
            out.push('}');
        }
        Expr::TupleFieldValue { .. } => emit_expr(out, expr),
        Expr::Field { name, field } => {
            out.push_str("${");
            out.push_str(name);
            out.push('_');
            out.push_str(field);
            out.push('}');
        }
        Expr::FieldValue { .. } => emit_expr(out, expr),
        Expr::NewtypeCtor { value, .. } => emit_bash_arith_expr(out, value),
        Expr::Cast { expr, .. } => emit_bash_arith_expr(out, expr),
        Expr::Call { name, args } => {
            out.push_str("$(");
            emit_call_head(out, name);
            for arg in args {
                out.push(' ');
                emit_call_arg(out, arg);
            }
            out.push(')');
        }
        Expr::BitNot(expr) => {
            out.push_str("~(");
            emit_bash_arith_expr(out, expr);
            out.push(')');
        }
        Expr::Binary { left, op, right } if op.is_arithmetic() || op.is_bitwise() => {
            out.push('(');
            emit_bash_arith_expr(out, left);
            out.push(' ');
            out.push_str(op.bash());
            out.push(' ');
            emit_bash_arith_expr(out, right);
            out.push(')');
        }
        _ => emit_expr(out, expr),
    }
}

fn emit_tuple_binding(out: &mut String, name: &str, values: &[Expr], readonly: bool, local: bool) {
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

fn emit_record_binding(
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

fn constructor_record_fields(expr: &Expr) -> Option<(char, &[(String, Expr)])> {
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

fn constructor_tuple_values(expr: &Expr) -> Option<(char, &[Expr])> {
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

fn emit_constructor_record_binding(
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

fn emit_constructor_tuple_binding(
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

fn emit_record_value(out: &mut String, fields: &[(String, Expr)]) {
    out.push('(');
    for (index, (_field, value)) in fields.iter().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        emit_array_element(out, value);
    }
    out.push(')');
}

fn emit_tuple_value(out: &mut String, values: &[Expr]) {
    out.push('(');
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        emit_array_element(out, value);
    }
    out.push(')');
}

fn emit_tuple_field(out: &mut String, name: &str, field: usize) {
    out.push('"');
    out.push('$');
    out.push_str(name);
    out.push('_');
    out.push_str(&field.to_string());
    out.push('"');
}

fn emit_tuple_field_value(out: &mut String, value: &Expr, field: usize) {
    if let Expr::Tuple(values) = value {
        if let Some(value) = values.get(field - 1) {
            emit_expr(out, value);
            return;
        }
    }
    emit_expr(out, value);
}

fn emit_variable_ref(out: &mut String, name: &str) {
    out.push('"');
    out.push('$');
    out.push_str(name);
    out.push('"');
}

fn emit_ident_value(out: &mut String, name: &str) {
    if is_shell_name(name) {
        emit_variable_ref(out, name);
    } else {
        emit_shell_word(out, name);
    }
}

fn emit_closure(out: &mut String, name: &str, captures: &[ClosureCapture]) {
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

fn emit_let_in(
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

fn emit_inline_declaration_copy(out: &mut String, source: &str, target: &str, ty: &Type) {
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

fn emit_path_exists(out: &mut String, path: &Expr) {
    out.push_str("$(__nacre_guarded_path=");
    emit_expr(out, path);
    out.push_str("; __nacre_guarded_path=\"$(__nacre_checked_path read \"$__nacre_guarded_path\")\" || exit $?; if [ -e \"$__nacre_guarded_path\" ]; then printf true; else printf false; fi)");
}

fn emit_fs_test(out: &mut String, test: &str, path: &Expr) {
    out.push_str("$(__nacre_guarded_path=");
    emit_expr(out, path);
    out.push_str("; __nacre_guarded_path=\"$(__nacre_checked_path read \"$__nacre_guarded_path\")\" || exit $?; if [ ");
    out.push_str(test);
    out.push_str(" \"$__nacre_guarded_path\" ]; then printf true; else printf false; fi)");
}

fn emit_fs_size(out: &mut String, path: &Expr) {
    out.push_str("$(__nacre_guarded_path=");
    emit_expr(out, path);
    out.push_str("; __nacre_guarded_path=\"$(__nacre_checked_path read \"$__nacre_guarded_path\")\" || exit $?; wc -c < \"$__nacre_guarded_path\" | tr -d '[:space:]')");
}

fn emit_fs_read_lines_binding(
    out: &mut String,
    binding: &str,
    path: &Expr,
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local -a ");
        out.push_str(binding);
        out.push('\n');
    }
    out.push_str("__nacre_guarded_path=");
    emit_expr(out, path);
    out.push_str(
        "\n__nacre_guarded_path=\"$(__nacre_checked_path read \"$__nacre_guarded_path\")\" || exit $?\nmapfile -t ",
    );
    out.push_str(binding);
    out.push_str(" < \"$__nacre_guarded_path\"\n");
    if readonly && !local {
        out.push_str("readonly -a ");
        out.push_str(binding);
        out.push('\n');
    }
}

fn emit_fs_read_lines_value(out: &mut String, path: &Expr) {
    out.push_str("\"$(__nacre_guarded_path=");
    emit_expr(out, path);
    out.push_str("; __nacre_guarded_path=\"$(__nacre_checked_path read \"$__nacre_guarded_path\")\" || exit $?; printf '%s' \"$(<\"$__nacre_guarded_path\")\")\"");
}

fn emit_fs_list_binding(out: &mut String, binding: &str, path: &Expr, readonly: bool, local: bool) {
    if local {
        out.push_str("local -a ");
        out.push_str(binding);
        out.push('\n');
    }
    out.push_str("mapfile -t ");
    out.push_str(binding);
    out.push_str(" < <(");
    emit_fs_list_command(out, path);
    out.push_str(")\n");
    if readonly && !local {
        out.push_str("readonly -a ");
        out.push_str(binding);
        out.push('\n');
    }
}

fn emit_fs_list_value(out: &mut String, path: &Expr) {
    out.push_str("\"$(");
    emit_fs_list_command(out, path);
    out.push_str(")\"");
}

fn emit_fs_list_command(out: &mut String, path: &Expr) {
    out.push_str("__nacre_guarded_path=");
    emit_expr(out, path);
    out.push_str("; __nacre_guarded_path=\"$(__nacre_checked_path read \"$__nacre_guarded_path\")\" || exit $?; find \"$__nacre_guarded_path\" -mindepth 1 -maxdepth 1 -print | sort");
}

fn emit_fs_write_lines_statement(out: &mut String, path: &Expr, lines: &Expr) {
    emit_fs_write_lines_command(out, path, lines);
    out.push('\n');
}

fn emit_fs_append_lines_statement(out: &mut String, path: &Expr, lines: &Expr) {
    emit_fs_append_lines_command(out, path, lines);
    out.push('\n');
}

fn emit_fs_write_lines_expr(out: &mut String, path: &Expr, lines: &Expr) {
    out.push_str("\"$(");
    emit_fs_write_lines_command(out, path, lines);
    out.push_str(")\"");
}

fn emit_fs_append_lines_expr(out: &mut String, path: &Expr, lines: &Expr) {
    out.push_str("\"$(");
    emit_fs_append_lines_command(out, path, lines);
    out.push_str(")\"");
}

fn emit_fs_write_lines_command(out: &mut String, path: &Expr, lines: &Expr) {
    out.push_str("__nacre_guarded_path=");
    emit_expr(out, path);
    out.push_str("; __nacre_guarded_path=\"$(__nacre_checked_path write \"$__nacre_guarded_path\")\" || exit $?; ");
    if let Expr::FsReadLines { path: source } = lines {
        out.push_str("__nacre_guarded_source=");
        emit_expr(out, source);
        out.push_str("; __nacre_guarded_source=\"$(__nacre_checked_path read \"$__nacre_guarded_source\")\" || exit $?; cat \"$__nacre_guarded_source\" > \"$__nacre_guarded_path\"");
        return;
    }
    out.push_str("printf '%s\\n'");
    emit_array_words(out, lines);
    out.push_str(" > \"$__nacre_guarded_path\"");
}

fn emit_fs_append_lines_command(out: &mut String, path: &Expr, lines: &Expr) {
    out.push_str("__nacre_guarded_path=");
    emit_expr(out, path);
    out.push_str("; __nacre_guarded_path=\"$(__nacre_checked_path write \"$__nacre_guarded_path\")\" || exit $?; ");
    if let Expr::FsReadLines { path: source } = lines {
        out.push_str("__nacre_guarded_source=");
        emit_expr(out, source);
        out.push_str("; __nacre_guarded_source=\"$(__nacre_checked_path read \"$__nacre_guarded_source\")\" || exit $?; cat \"$__nacre_guarded_source\" >> \"$__nacre_guarded_path\"");
        return;
    }
    out.push_str("printf '%s\\n'");
    emit_array_words(out, lines);
    out.push_str(" >> \"$__nacre_guarded_path\"");
}

fn emit_array_words(out: &mut String, expr: &Expr) {
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

fn emit_process_env(out: &mut String, name: &Expr) {
    out.push_str("\"$(__nacre_env_name=");
    emit_call_arg(out, name);
    out.push_str("; if [[ \"$__nacre_env_name\" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]]; then printf '%s' \"${!__nacre_env_name-}\"; fi)\"");
}

fn emit_field(out: &mut String, name: &str, field: &str) {
    out.push('"');
    out.push('$');
    out.push_str(name);
    out.push('_');
    out.push_str(field);
    out.push('"');
}

fn emit_field_value(out: &mut String, value: &Expr, field: &str) {
    if let Expr::Record(fields) = value {
        if let Some((_, value)) = fields.iter().find(|(candidate, _)| candidate == field) {
            emit_expr(out, value);
            return;
        }
    }
    emit_expr(out, value);
}

fn emit_index(out: &mut String, name: &str, index: &Expr) {
    out.push_str("\"${");
    out.push_str(name);
    out.push('[');
    emit_index_expr(out, index);
    out.push_str("]}\"");
}

fn emit_index_value(out: &mut String, value: &Expr, index: &Expr) {
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
        _ => emit_expr(out, value),
    }
    out.push(')');
}

fn emit_index_expr(out: &mut String, expr: &Expr) {
    match expr {
        Expr::Int(value) => out.push_str(&value.to_string()),
        Expr::Ident(name) => out.push_str(name),
        _ => emit_expr(out, expr),
    }
}

fn emit_array_slice_binding(
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

fn emit_array_slice_value_binding(
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

fn emit_array_take_binding(
    out: &mut String,
    binding: &str,
    source: &str,
    count: &Expr,
    readonly: bool,
    local: bool,
) {
    emit_array_slice_binding(out, binding, source, &Expr::Int(0), count, readonly, local);
}

fn emit_array_take_value_binding(
    out: &mut String,
    binding: &str,
    source: &Expr,
    count: &Expr,
    readonly: bool,
    local: bool,
) {
    emit_array_slice_value_binding(out, binding, source, &Expr::Int(0), count, readonly, local);
}

fn emit_array_drop_binding(
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

fn emit_array_drop_value_binding(
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

fn emit_array_temp_assignment(out: &mut String, name: &str, source: &Expr, local: bool) {
    if local {
        out.push_str("local -a ");
    }
    out.push_str(name);
    out.push('=');
    emit_expr(out, source);
    out.push('\n');
}

fn emit_array_expansion_binding(
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

fn emit_array_reverse_binding(
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

fn emit_array_reverse_value_binding(
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

fn emit_array_sort_binding(
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

fn emit_array_sort_value_binding(
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

fn emit_array_unique_binding(
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

fn emit_array_unique_value_binding(
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

fn emit_array_unique_to(out: &mut String, binding: &str, source: &str) {
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

fn emit_array_map_binding(
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

fn emit_array_map_value_binding(
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

fn emit_array_map_to(out: &mut String, binding: &str, source: &str, mapper: &Expr) {
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

fn emit_mapper_command(out: &mut String, mapper: &Expr) {
    match mapper {
        Expr::Ident(name) => emit_call_head(out, name),
        Expr::Closure { name, captures } => {
            out.push_str("__nacre_call ");
            emit_closure(out, name, captures);
        }
        _ => emit_expr(out, mapper),
    }
}

fn emit_process_args_binding(out: &mut String, binding: &str, readonly: bool, local: bool) {
    emit_array_expansion_binding(out, binding, "${args[@]}", readonly, local);
}

fn emit_cli_parse_binding(out: &mut String, binding: &str, readonly: bool, local: bool) {
    if local {
        out.push_str("local -A ");
    } else {
        out.push_str("declare -A ");
    }
    out.push_str(binding);
    out.push('\n');
    if local {
        out.push_str("local ");
    }
    out.push_str("__nacre_cli_pending=''\nfor __nacre_cli_arg in \"${args[@]}\"; do\n");
    out.push_str("if [[ \"$__nacre_cli_arg\" == --*=* ]]; then\n");
    out.push_str("__nacre_cli_key=\"${__nacre_cli_arg%%=*}\"\n");
    out.push_str("__nacre_cli_key=\"${__nacre_cli_key#--}\"\n");
    out.push_str(binding);
    out.push_str("[\"$__nacre_cli_key\"]=\"${__nacre_cli_arg#*=}\"\n");
    out.push_str("__nacre_cli_pending=''\n");
    out.push_str("elif [[ \"$__nacre_cli_arg\" == --* ]]; then\n");
    out.push_str("__nacre_cli_key=\"${__nacre_cli_arg#--}\"\n");
    out.push_str(binding);
    out.push_str("[\"$__nacre_cli_key\"]=true\n");
    out.push_str("__nacre_cli_pending=\"$__nacre_cli_key\"\n");
    out.push_str("elif [ -n \"$__nacre_cli_pending\" ]; then\n");
    out.push_str(binding);
    out.push_str("[\"$__nacre_cli_pending\"]=\"$__nacre_cli_arg\"\n");
    out.push_str("__nacre_cli_pending=''\n");
    out.push_str("fi\ndone\nunset __nacre_cli_pending __nacre_cli_arg __nacre_cli_key\n");
    if readonly && !local {
        out.push_str("readonly -A ");
        out.push_str(binding);
        out.push('\n');
    }
}

const JSON_PARSE_AWK: &str = r#"function skip_ws() {
  while (i <= n && substr(s, i, 1) ~ /[ \t\r\n]/) i++
}
function read_string(    out, c, esc) {
  out = ""
  i++
  esc = 0
  while (i <= n) {
    c = substr(s, i, 1)
    if (esc) {
      if (c == "n") out = out "\n"
      else if (c == "r") out = out "\r"
      else if (c == "t") out = out "\t"
      else out = out c
      esc = 0
    } else if (c == "\\") {
      esc = 1
    } else if (c == "\"") {
      i++
      return out
    } else {
      out = out c
    }
    i++
  }
  return out
}
BEGIN { RS = "" }
{
  s = $0
  n = length(s)
  i = 1
  skip_ws()
  if (substr(s, i, 1) == "{") i++
  while (i <= n) {
    skip_ws()
    if (substr(s, i, 1) == "}") break
    if (substr(s, i, 1) != "\"") break
    key = read_string()
    skip_ws()
    if (substr(s, i, 1) != ":") break
    i++
    skip_ws()
    if (substr(s, i, 1) == "\"") {
      value = read_string()
    } else {
      value = ""
      while (i <= n) {
        c = substr(s, i, 1)
        if (c == "," || c == "}") break
        value = value c
        i++
      }
      gsub(/^[ \t\r\n]+|[ \t\r\n]+$/, "", value)
    }
    printf "%s\t%s\n", key, value
    skip_ws()
    if (substr(s, i, 1) == ",") i++
  }
}"#;

const JSON_ESCAPE_AWK: &str = r#"{ gsub(/\\/,"\\\\"); gsub(/"/,"\\\""); gsub(/\r/,"\\r"); gsub(/\t/,"\\t"); printf "%s", $0 }"#;

fn emit_json_parse_binding(
    out: &mut String,
    binding: &str,
    value: &Expr,
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local -A ");
    } else {
        out.push_str("declare -A ");
    }
    out.push_str(binding);
    out.push('\n');
    out.push_str("while IFS=$'\\t' read -r __nacre_json_key __nacre_json_value; do\n");
    out.push_str(binding);
    out.push_str("[\"$__nacre_json_key\"]=\"$__nacre_json_value\"\n");
    out.push_str("done < <(printf '%s' ");
    emit_call_arg(out, value);
    out.push_str(" | awk ");
    emit_shell_word(out, JSON_PARSE_AWK);
    out.push_str(")\nunset __nacre_json_key __nacre_json_value\n");
    if readonly && !local {
        out.push_str("readonly -A ");
        out.push_str(binding);
        out.push('\n');
    }
}

fn emit_json_stringify(out: &mut String, name: &str) {
    out.push_str("\"$(");
    emit_json_stringify_command(out, name);
    out.push_str(")\"");
}

fn emit_json_stringify_value(out: &mut String, value: &Expr) {
    out.push_str("\"$(");
    emit_map_expr_binding(out, "__nacre_json_map", value);
    emit_json_stringify_command(out, "__nacre_json_map");
    out.push_str(")\"");
}

fn emit_map_expr_binding(out: &mut String, name: &str, value: &Expr) {
    match value {
        Expr::Map(entries) => {
            out.push_str("declare -A ");
            out.push_str(name);
            out.push('=');
            emit_map(out, entries);
            out.push_str("; ");
        }
        Expr::JsonParse { value } => {
            emit_json_parse_binding(out, name, value, false, false);
        }
        _ => {
            out.push_str("declare -A ");
            out.push_str(name);
            out.push_str("=(); ");
        }
    }
}

fn emit_json_stringify_command(out: &mut String, name: &str) {
    out.push_str("printf '{'; __nacre_json_first=true; for __nacre_json_key in \"${!");
    out.push_str(name);
    out.push_str("[@]}\"; do if [ \"$__nacre_json_first\" = true ]; then __nacre_json_first=false; else printf ','; fi; printf '\"'; printf '%s' \"$__nacre_json_key\" | awk ");
    emit_shell_word(out, JSON_ESCAPE_AWK);
    out.push_str("; printf '\":\"'; printf '%s' \"${");
    out.push_str(name);
    out.push_str("[$__nacre_json_key]}\" | awk ");
    emit_shell_word(out, JSON_ESCAPE_AWK);
    out.push_str("; printf '\"'; done; printf '}'");
}

fn emit_array_slice_value(out: &mut String, source: &str, start: &Expr, end: &Expr) {
    emit_array_slice_elements(out, source, start, end);
}

fn emit_array_slice_value_expr(out: &mut String, source: &Expr, start: &Expr, end: &Expr) {
    out.push_str("$(__nacre_array_value=");
    emit_expr(out, source);
    out.push_str("; printf '%s\\n' ");
    emit_array_slice_elements(out, "__nacre_array_value", start, end);
    out.push(')');
}

fn emit_array_take_value(out: &mut String, source: &str, count: &Expr) {
    emit_array_take_elements(out, source, count);
}

fn emit_array_take_value_expr(out: &mut String, source: &Expr, count: &Expr) {
    emit_array_slice_value_expr(out, source, &Expr::Int(0), count);
}

fn emit_array_drop_value(out: &mut String, source: &str, count: &Expr) {
    emit_array_drop_elements(out, source, count);
}

fn emit_array_drop_value_expr(out: &mut String, source: &Expr, count: &Expr) {
    out.push_str("$(__nacre_array_value=");
    emit_expr(out, source);
    out.push_str("; printf '%s\\n' ");
    emit_array_drop_elements(out, "__nacre_array_value", count);
    out.push(')');
}

fn emit_array_slice_elements(out: &mut String, source: &str, start: &Expr, end: &Expr) {
    out.push_str("\"${");
    out.push_str(source);
    out.push_str("[@]:");
    emit_arithmetic_expansion(out, start);
    out.push(':');
    emit_slice_length(out, start, end);
    out.push_str("}\"");
}

fn emit_array_take_elements(out: &mut String, source: &str, count: &Expr) {
    emit_array_slice_elements(out, source, &Expr::Int(0), count);
}

fn emit_array_drop_elements(out: &mut String, source: &str, count: &Expr) {
    out.push_str("\"${");
    out.push_str(source);
    out.push_str("[@]:");
    emit_arithmetic_expansion(out, count);
    out.push_str("}\"");
}

fn emit_arithmetic_expansion(out: &mut String, expr: &Expr) {
    out.push_str("$((");
    emit_bash_arith_expr(out, expr);
    out.push_str("))");
}

fn emit_slice_length(out: &mut String, start: &Expr, end: &Expr) {
    out.push_str("$((");
    emit_bash_arith_expr(out, end);
    out.push_str(" - ");
    emit_bash_arith_expr(out, start);
    out.push_str("))");
}

fn emit_is_empty(out: &mut String, name: &str) {
    out.push_str("$(if [ \"${#");
    out.push_str(name);
    out.push_str("[@]}\" -eq 0 ]; then printf true; else printf false; fi)");
}

fn emit_array_len_value(out: &mut String, value: &Expr) {
    if let Expr::Array(values) = value {
        out.push_str(&values.len().to_string());
    } else {
        out.push('0');
    }
}

fn emit_array_is_empty_value(out: &mut String, value: &Expr) {
    if matches!(value, Expr::Array(values) if values.is_empty()) {
        out.push_str("true");
    } else {
        out.push_str("false");
    }
}

fn emit_array_first(out: &mut String, name: &str) {
    out.push_str("\"${");
    out.push_str(name);
    out.push_str("[0]}\"");
}

fn emit_array_first_value(out: &mut String, value: &Expr) {
    if let Expr::Array(values) = value {
        if let Some(first) = values.first() {
            emit_expr(out, first);
            return;
        }
    }
    emit_bash_string(out, "");
}

fn emit_array_last(out: &mut String, name: &str) {
    out.push_str("$(if [ \"${#");
    out.push_str(name);
    out.push_str("[@]}\" -gt 0 ]; then printf '%s' \"${");
    out.push_str(name);
    out.push_str("[$((${#");
    out.push_str(name);
    out.push_str("[@]} - 1))]}\"; fi)");
}

fn emit_array_last_value(out: &mut String, value: &Expr) {
    if let Expr::Array(values) = value {
        if let Some(last) = values.last() {
            emit_expr(out, last);
            return;
        }
    }
    emit_bash_string(out, "");
}

fn emit_array_reverse_value(out: &mut String, name: &str) {
    out.push_str("$(__nacre_reverse=(); for ((__nacre_i=${#");
    out.push_str(name);
    out.push_str("[@]} - 1; __nacre_i >= 0; __nacre_i--)); do __nacre_reverse+=(\"${");
    out.push_str(name);
    out.push_str("[$__nacre_i]}\"); done; printf '%s\\n' \"${__nacre_reverse[@]}\")");
}

fn emit_array_reverse_value_expr(out: &mut String, value: &Expr) {
    out.push_str("$(printf '%s\\n'");
    emit_array_value_args_reversed(out, value);
    out.push(')');
}

fn emit_array_sort_value(out: &mut String, name: &str) {
    out.push_str("$(if [ \"${#");
    out.push_str(name);
    out.push_str("[@]}\" -gt 0 ]; then printf '%s\\n' \"${");
    out.push_str(name);
    out.push_str("[@]}\" | sort; fi)");
}

fn emit_array_sort_value_expr(out: &mut String, value: &Expr) {
    out.push_str("$(printf '%s\\n'");
    emit_array_value_args(out, value);
    out.push_str(" | sort)");
}

fn emit_array_unique_value(out: &mut String, name: &str) {
    out.push_str("$(__nacre_unique=(); for __nacre_item in \"${");
    out.push_str(name);
    out.push_str("[@]}\"; do __nacre_seen=false; for __nacre_existing in \"${__nacre_unique[@]}\"; do if [ \"$__nacre_existing\" = \"$__nacre_item\" ]; then __nacre_seen=true; break; fi; done; if [ \"$__nacre_seen\" = false ]; then __nacre_unique+=(\"$__nacre_item\"); fi; done; if [ \"${#__nacre_unique[@]}\" -gt 0 ]; then printf '%s\\n' \"${__nacre_unique[@]}\"; fi)");
}

fn emit_array_unique_value_expr(out: &mut String, value: &Expr) {
    out.push_str("$(__nacre_unique=(); for __nacre_item in ");
    emit_array_value_words(out, value);
    out.push_str("; do __nacre_seen=false; for __nacre_existing in \"${__nacre_unique[@]}\"; do if [ \"$__nacre_existing\" = \"$__nacre_item\" ]; then __nacre_seen=true; break; fi; done; if [ \"$__nacre_seen\" = false ]; then __nacre_unique+=(\"$__nacre_item\"); fi; done; if [ \"${#__nacre_unique[@]}\" -gt 0 ]; then printf '%s\\n' \"${__nacre_unique[@]}\"; fi)");
}

fn emit_array_map_value(out: &mut String, name: &str, mapper: &Expr) {
    out.push_str("$(for __nacre_item in \"${");
    out.push_str(name);
    out.push_str("[@]}\"; do ");
    emit_mapper_command(out, mapper);
    out.push_str(" \"$__nacre_item\"; done)");
}

fn emit_array_map_value_expr(out: &mut String, value: &Expr, mapper: &Expr) {
    out.push_str("$(__nacre_array_value=");
    emit_expr(out, value);
    out.push_str("; for __nacre_item in \"${__nacre_array_value[@]}\"; do ");
    emit_mapper_command(out, mapper);
    out.push_str(" \"$__nacre_item\"; done)");
}

fn emit_map_keys_value(out: &mut String, name: &str) {
    out.push_str("\"${!");
    out.push_str(name);
    out.push_str("[@]}\"");
}

fn emit_map_values_value(out: &mut String, name: &str) {
    out.push_str("\"${");
    out.push_str(name);
    out.push_str("[@]}\"");
}

fn emit_map_len_value(out: &mut String, value: &Expr) {
    out.push_str("$(");
    emit_map_expr_binding(out, "__nacre_map_value", value);
    out.push_str("printf '%s' \"${#__nacre_map_value[@]}\")");
}

fn emit_map_is_empty_value(out: &mut String, value: &Expr) {
    out.push_str("$(");
    emit_map_expr_binding(out, "__nacre_map_value", value);
    out.push_str(
        "if [ \"${#__nacre_map_value[@]}\" -eq 0 ]; then printf true; else printf false; fi)",
    );
}

fn emit_map_has(out: &mut String, name: &str, key: &Expr) {
    out.push_str("$(if [[ -v ");
    out.push_str(name);
    out.push('[');
    emit_map_key(out, key);
    out.push_str("] ]]; then printf true; else printf false; fi)");
}

fn emit_map_keys_value_expr(out: &mut String, value: &Expr) {
    out.push_str("$(");
    emit_map_expr_binding(out, "__nacre_map_value", value);
    out.push_str("if [ \"${#__nacre_map_value[@]}\" -gt 0 ]; then printf '%s\\n' \"${!__nacre_map_value[@]}\"; fi)");
}

fn emit_map_values_value_expr(out: &mut String, value: &Expr) {
    out.push_str("$(");
    emit_map_expr_binding(out, "__nacre_map_value", value);
    out.push_str("if [ \"${#__nacre_map_value[@]}\" -gt 0 ]; then printf '%s\\n' \"${__nacre_map_value[@]}\"; fi)");
}

fn emit_map_keys_value_binding(
    out: &mut String,
    binding: &str,
    value: &Expr,
    readonly: bool,
    local: bool,
) {
    emit_map_entries_value_binding(out, binding, value, readonly, local, true);
}

fn emit_map_values_value_binding(
    out: &mut String,
    binding: &str,
    value: &Expr,
    readonly: bool,
    local: bool,
) {
    emit_map_entries_value_binding(out, binding, value, readonly, local, false);
}

fn emit_map_entries_value_binding(
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

fn emit_map_has_value(out: &mut String, value: &Expr, key: &Expr) {
    out.push_str("$(");
    emit_map_expr_binding(out, "__nacre_map_value", value);
    out.push_str("if [[ -v __nacre_map_value[");
    emit_map_key(out, key);
    out.push_str("] ]]; then printf true; else printf false; fi)");
}

fn emit_array_contains(out: &mut String, name: &str, value: &Expr) {
    out.push_str("$(__nacre_contains=false; for __nacre_item in \"${");
    out.push_str(name);
    out.push_str("[@]}\"; do if [ \"$__nacre_item\" = ");
    emit_call_arg(out, value);
    out.push_str(
        " ]; then __nacre_contains=true; break; fi; done; printf '%s' \"$__nacre_contains\")",
    );
}

fn emit_array_contains_value(out: &mut String, value: &Expr, item: &Expr) {
    out.push_str("$(__nacre_contains=false; for __nacre_item in ");
    emit_array_value_words(out, value);
    out.push_str("; do if [ \"$__nacre_item\" = ");
    emit_call_arg(out, item);
    out.push_str(
        " ]; then __nacre_contains=true; break; fi; done; printf '%s' \"$__nacre_contains\")",
    );
}

fn emit_array_index_of(out: &mut String, name: &str, value: &Expr) {
    out.push_str("$(__nacre_index=-1; __nacre_i=0; for __nacre_item in \"${");
    out.push_str(name);
    out.push_str("[@]}\"; do if [ \"$__nacre_item\" = ");
    emit_call_arg(out, value);
    out.push_str(
        " ]; then __nacre_index=$__nacre_i; break; fi; __nacre_i=$((__nacre_i + 1)); done; printf '%s' \"$__nacre_index\")",
    );
}

fn emit_array_index_of_value(out: &mut String, value: &Expr, item: &Expr) {
    out.push_str("$(__nacre_index=-1; __nacre_i=0; for __nacre_item in ");
    emit_array_value_words(out, value);
    out.push_str("; do if [ \"$__nacre_item\" = ");
    emit_call_arg(out, item);
    out.push_str(
        " ]; then __nacre_index=$__nacre_i; break; fi; __nacre_i=$((__nacre_i + 1)); done; printf '%s' \"$__nacre_index\")",
    );
}

fn emit_array_value_words(out: &mut String, value: &Expr) {
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

fn emit_array_value_args(out: &mut String, value: &Expr) {
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

fn emit_array_value_args_reversed(out: &mut String, value: &Expr) {
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

fn emit_string_contains(out: &mut String, name: &str, needle: &Expr) {
    out.push_str("$(if [[ \"$");
    out.push_str(name);
    out.push_str("\" == *");
    emit_expr(out, needle);
    out.push_str("* ]]; then printf true; else printf false; fi)");
}

#[derive(Clone, Copy)]
enum StringPredicate {
    Contains,
    IndexOf,
    StartsWith,
    EndsWith,
}

fn emit_string_predicate_value_binding(
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

fn emit_checked_string_predicate_value(
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

fn emit_string_predicate_name(
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

fn emit_string_predicate_expr(
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

fn emit_string_contains_expr(out: &mut String, value: &Expr, needle: &Expr) {
    out.push_str("$(if [[ ");
    emit_call_arg(out, value);
    out.push_str(" == *");
    emit_expr(out, needle);
    out.push_str("* ]]; then printf true; else printf false; fi)");
}

fn emit_string_index_of(out: &mut String, name: &str, needle: &Expr) {
    out.push_str("$(awk -v __nacre_haystack=\"$");
    out.push_str(name);
    out.push_str("\" -v __nacre_needle=");
    emit_call_arg(out, needle);
    out.push_str(" 'BEGIN { __nacre_index = index(__nacre_haystack, __nacre_needle); printf \"%s\", (__nacre_index ? __nacre_index - 1 : -1) }')");
}

fn emit_string_index_of_expr(out: &mut String, value: &Expr, needle: &Expr) {
    out.push_str("$(awk -v __nacre_haystack=");
    emit_call_arg(out, value);
    out.push_str(" -v __nacre_needle=");
    emit_call_arg(out, needle);
    out.push_str(" 'BEGIN { __nacre_index = index(__nacre_haystack, __nacre_needle); printf \"%s\", (__nacre_index ? __nacre_index - 1 : -1) }')");
}

fn emit_string_starts_with(out: &mut String, name: &str, prefix: &Expr) {
    out.push_str("$(if [[ \"$");
    out.push_str(name);
    out.push_str("\" == ");
    emit_expr(out, prefix);
    out.push_str("* ]]; then printf true; else printf false; fi)");
}

fn emit_string_starts_with_expr(out: &mut String, value: &Expr, prefix: &Expr) {
    out.push_str("$(if [[ ");
    emit_call_arg(out, value);
    out.push_str(" == ");
    emit_expr(out, prefix);
    out.push_str("* ]]; then printf true; else printf false; fi)");
}

fn emit_string_ends_with(out: &mut String, name: &str, suffix: &Expr) {
    out.push_str("$(if [[ \"$");
    out.push_str(name);
    out.push_str("\" == *");
    emit_expr(out, suffix);
    out.push_str(" ]]; then printf true; else printf false; fi)");
}

fn emit_string_ends_with_expr(out: &mut String, value: &Expr, suffix: &Expr) {
    out.push_str("$(if [[ ");
    emit_call_arg(out, value);
    out.push_str(" == *");
    emit_expr(out, suffix);
    out.push_str(" ]]; then printf true; else printf false; fi)");
}

fn emit_string_len(out: &mut String, name: &str) {
    out.push_str("\"${#");
    out.push_str(name);
    out.push_str("}\"");
}

#[derive(Clone, Copy)]
enum StringUnary {
    Len,
    IsEmpty,
}

fn emit_string_unary_value_binding(
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

fn emit_string_unary_name(out: &mut String, name: &str, op: StringUnary) {
    match op {
        StringUnary::Len => emit_string_len(out, name),
        StringUnary::IsEmpty => emit_string_is_empty(out, name),
    }
}

fn emit_string_unary_expr(out: &mut String, value: &Expr, op: StringUnary) {
    match op {
        StringUnary::Len => emit_string_len_expr(out, value),
        StringUnary::IsEmpty => emit_string_is_empty_expr(out, value),
    }
}

fn emit_string_len_expr(out: &mut String, value: &Expr) {
    out.push_str("$(__nacre_string_value=");
    emit_call_arg(out, value);
    out.push_str("; printf '%s' \"${#__nacre_string_value}\")");
}

fn emit_string_is_empty(out: &mut String, name: &str) {
    out.push_str("$(if [ -z \"$");
    out.push_str(name);
    out.push_str("\" ]; then printf true; else printf false; fi)");
}

fn emit_string_is_empty_expr(out: &mut String, value: &Expr) {
    out.push_str("$(if [ -z ");
    emit_call_arg(out, value);
    out.push_str(" ]; then printf true; else printf false; fi)");
}

fn emit_string_slice(out: &mut String, name: &str, start: &Expr, end: &Expr) {
    out.push_str("\"${");
    out.push_str(name);
    out.push(':');
    emit_arithmetic_expansion(out, start);
    out.push(':');
    emit_slice_length(out, start, end);
    out.push_str("}\"");
}

fn emit_string_slice_value_binding(
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

fn emit_string_slice_expr(out: &mut String, value: &Expr, start: &Expr, end: &Expr) {
    out.push_str("\"$(__nacre_slice_value=");
    emit_call_arg(out, value);
    out.push_str("; printf '%s' \"${__nacre_slice_value:");
    emit_arithmetic_expansion(out, start);
    out.push(':');
    emit_slice_length(out, start, end);
    out.push_str("}\")\"");
}

#[derive(Clone, Copy)]
enum StringTransform {
    Trim,
    TrimStart,
    TrimEnd,
    ToUpper,
    ToLower,
}

fn string_transform_temp_name(transform: StringTransform) -> &'static str {
    match transform {
        StringTransform::Trim => "__nacre_trim_value",
        StringTransform::TrimStart => "__nacre_trim_start_value",
        StringTransform::TrimEnd => "__nacre_trim_end_value",
        StringTransform::ToUpper => "__nacre_upper_value",
        StringTransform::ToLower => "__nacre_lower_value",
    }
}

fn emit_string_transform_name(out: &mut String, name: &str, transform: StringTransform) {
    match transform {
        StringTransform::Trim => emit_string_trim(out, name),
        StringTransform::TrimStart => emit_string_trim_start(out, name),
        StringTransform::TrimEnd => emit_string_trim_end(out, name),
        StringTransform::ToUpper => emit_string_case_transform(out, name, "[:lower:]", "[:upper:]"),
        StringTransform::ToLower => emit_string_case_transform(out, name, "[:upper:]", "[:lower:]"),
    }
}

fn emit_string_transform_expr(out: &mut String, value: &Expr, transform: StringTransform) {
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

fn emit_string_trim(out: &mut String, name: &str) {
    out.push_str("\"$(printf '%s' \"$");
    out.push_str(name);
    out.push_str("\" | awk ");
    emit_shell_word(
        out,
        r#"{gsub(/^[[:space:]]+|[[:space:]]+$/, ""); printf "%s", $0}"#,
    );
    out.push_str(")\"");
}

fn emit_string_trim_expr(out: &mut String, value: &Expr) {
    out.push_str("\"$(printf '%s' ");
    emit_call_arg(out, value);
    out.push_str(" | awk ");
    emit_shell_word(
        out,
        r#"{gsub(/^[[:space:]]+|[[:space:]]+$/, ""); printf "%s", $0}"#,
    );
    out.push_str(")\"");
}

fn emit_string_trim_start_expr(out: &mut String, value: &Expr) {
    out.push_str("\"$(printf '%s' ");
    emit_call_arg(out, value);
    out.push_str(" | awk ");
    emit_shell_word(out, r#"{gsub(/^[[:space:]]+/, ""); printf "%s", $0}"#);
    out.push_str(")\"");
}

fn emit_string_trim_start(out: &mut String, name: &str) {
    out.push_str("\"$(printf '%s' \"$");
    out.push_str(name);
    out.push_str("\" | awk ");
    emit_shell_word(out, r#"{gsub(/^[[:space:]]+/, ""); printf "%s", $0}"#);
    out.push_str(")\"");
}

fn emit_string_trim_end_expr(out: &mut String, value: &Expr) {
    out.push_str("\"$(printf '%s' ");
    emit_call_arg(out, value);
    out.push_str(" | awk ");
    emit_shell_word(out, r#"{gsub(/[[:space:]]+$/, ""); printf "%s", $0}"#);
    out.push_str(")\"");
}

fn emit_string_trim_end(out: &mut String, name: &str) {
    out.push_str("\"$(printf '%s' \"$");
    out.push_str(name);
    out.push_str("\" | awk ");
    emit_shell_word(out, r#"{gsub(/[[:space:]]+$/, ""); printf "%s", $0}"#);
    out.push_str(")\"");
}

fn emit_string_case_transform(out: &mut String, name: &str, from: &str, to: &str) {
    out.push_str("\"$(printf '%s' \"$");
    out.push_str(name);
    out.push_str("\" | tr ");
    emit_shell_word(out, from);
    out.push(' ');
    emit_shell_word(out, to);
    out.push_str(")\"");
}

fn emit_string_case_transform_expr(out: &mut String, value: &Expr, from: &str, to: &str) {
    out.push_str("\"$(printf '%s' ");
    emit_call_arg(out, value);
    out.push_str(" | tr ");
    emit_shell_word(out, from);
    out.push(' ');
    emit_shell_word(out, to);
    out.push_str(")\"");
}

fn emit_string_repeat(out: &mut String, name: &str, count: &Expr) {
    out.push_str("\"$(for ((__nacre_i=0; __nacre_i<");
    emit_arithmetic_expansion(out, count);
    out.push_str("; __nacre_i++)); do printf '%s' \"$");
    out.push_str(name);
    out.push_str("\"; done)\"");
}

fn emit_string_repeat_value_binding(
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

fn emit_string_repeat_expr(out: &mut String, value: &Expr, count: &Expr) {
    out.push_str("\"$(__nacre_repeat_value=");
    emit_call_arg(out, value);
    out.push_str("; for ((__nacre_i=0; __nacre_i<");
    emit_arithmetic_expansion(out, count);
    out.push_str("; __nacre_i++)); do printf '%s' \"$__nacre_repeat_value\"; done)\"");
}

fn emit_string_split_binding(
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

fn emit_string_split_value_binding(
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

fn emit_string_trim_value_binding(
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

fn emit_string_transform_value_binding(
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

fn emit_checked_string_transform_value(
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

fn emit_checked_string_split_value(
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

fn emit_call_output_array_binding(
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

fn emit_string_split_value(out: &mut String, name: &str, separator: &Expr) {
    out.push_str("\"$(");
    emit_string_split_command(out, name, separator);
    out.push_str(")\"");
}

fn emit_string_split_expr_value(out: &mut String, value: &Expr, separator: &Expr) {
    out.push_str("\"$(");
    emit_string_split_expr_command(out, value, separator);
    out.push_str(")\"");
}

fn emit_string_split_command(out: &mut String, name: &str, separator: &Expr) {
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

fn emit_string_split_expr_command(out: &mut String, value: &Expr, separator: &Expr) {
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

fn is_newline_separator(separator: &Expr) -> bool {
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

fn emit_string_replace_value_binding(
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

fn emit_checked_string_replace_value(
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

fn emit_string_replace(out: &mut String, name: &str, from: &Expr, to: &Expr) {
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

fn emit_string_replace_expr(out: &mut String, value: &Expr, from: &Expr, to: &Expr) {
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

#[derive(Clone, Copy)]
enum PathMethod {
    Basename,
    Dirname,
    Stem,
    Extname,
    IsAbsolute,
}

fn emit_path_method_value_binding(
    out: &mut String,
    binding: &str,
    value: &Expr,
    readonly: bool,
    local: bool,
    method: PathMethod,
) {
    if emit_checked_string_predicate_value(out, binding, value, local) {
        out.push_str(binding);
        out.push('=');
        emit_path_method_name(out, "__nacre_string_value", method);
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
    emit_path_method_expr(out, value, method);
    out.push('\n');
}

fn emit_path_method_name(out: &mut String, name: &str, method: PathMethod) {
    match method {
        PathMethod::Basename => emit_path_basename(out, name),
        PathMethod::Dirname => emit_path_dirname(out, name),
        PathMethod::Stem => emit_path_stem(out, name),
        PathMethod::Extname => emit_path_extname(out, name),
        PathMethod::IsAbsolute => emit_path_is_absolute(out, name),
    }
}

fn emit_path_method_expr(out: &mut String, value: &Expr, method: PathMethod) {
    match method {
        PathMethod::Basename => emit_path_basename_expr(out, value),
        PathMethod::Dirname => emit_path_dirname_expr(out, value),
        PathMethod::Stem => emit_path_stem_expr(out, value),
        PathMethod::Extname => emit_path_extname_expr(out, value),
        PathMethod::IsAbsolute => emit_path_is_absolute_expr(out, value),
    }
}

fn emit_path_basename(out: &mut String, name: &str) {
    out.push_str("\"$(basename \"$");
    out.push_str(name);
    out.push_str("\")\"");
}

fn emit_path_basename_expr(out: &mut String, value: &Expr) {
    out.push_str("\"$(basename ");
    emit_call_arg(out, value);
    out.push_str(")\"");
}

fn emit_path_dirname(out: &mut String, name: &str) {
    out.push_str("\"$(dirname \"$");
    out.push_str(name);
    out.push_str("\")\"");
}

fn emit_path_dirname_expr(out: &mut String, value: &Expr) {
    out.push_str("\"$(dirname ");
    emit_call_arg(out, value);
    out.push_str(")\"");
}

fn emit_path_stem(out: &mut String, name: &str) {
    out.push_str("\"$(");
    out.push_str("__nacre_path_base=$(basename \"$");
    out.push_str(name);
    out.push_str("\"); ");
    out.push_str(
        "case \"$__nacre_path_base\" in .*.*) printf '%s' \"${__nacre_path_base%.*}\" ;; ",
    );
    out.push_str(".*) printf '%s' \"$__nacre_path_base\" ;; ");
    out.push_str("*.*) printf '%s' \"${__nacre_path_base%.*}\" ;; ");
    out.push_str("*) printf '%s' \"$__nacre_path_base\" ;; esac)\"");
}

fn emit_path_stem_expr(out: &mut String, value: &Expr) {
    out.push_str("\"$(");
    out.push_str("__nacre_path_base=$(basename ");
    emit_call_arg(out, value);
    out.push_str("); ");
    out.push_str(
        "case \"$__nacre_path_base\" in .*.*) printf '%s' \"${__nacre_path_base%.*}\" ;; ",
    );
    out.push_str(".*) printf '%s' \"$__nacre_path_base\" ;; ");
    out.push_str("*.*) printf '%s' \"${__nacre_path_base%.*}\" ;; ");
    out.push_str("*) printf '%s' \"$__nacre_path_base\" ;; esac)\"");
}

fn emit_path_extname(out: &mut String, name: &str) {
    out.push_str("\"$(");
    out.push_str("__nacre_path_base=$(basename \"$");
    out.push_str(name);
    out.push_str("\"); ");
    out.push_str(
        "case \"$__nacre_path_base\" in .*.*) printf '%s' \".${__nacre_path_base##*.}\" ;; ",
    );
    out.push_str(".*) printf '' ;; *.*) printf '%s' \".${__nacre_path_base##*.}\" ;; ");
    out.push_str("*) printf '' ;; esac)\"");
}

fn emit_path_extname_expr(out: &mut String, value: &Expr) {
    out.push_str("\"$(");
    out.push_str("__nacre_path_base=$(basename ");
    emit_call_arg(out, value);
    out.push_str("); ");
    out.push_str(
        "case \"$__nacre_path_base\" in .*.*) printf '%s' \".${__nacre_path_base##*.}\" ;; ",
    );
    out.push_str(".*) printf '' ;; *.*) printf '%s' \".${__nacre_path_base##*.}\" ;; ");
    out.push_str("*) printf '' ;; esac)\"");
}

fn emit_path_is_absolute(out: &mut String, name: &str) {
    out.push_str("$(case \"$");
    out.push_str(name);
    out.push_str("\" in /*) printf true ;; *) printf false ;; esac)");
}

fn emit_path_is_absolute_expr(out: &mut String, value: &Expr) {
    out.push_str("$(case ");
    emit_call_arg(out, value);
    out.push_str(" in /*) printf true ;; *) printf false ;; esac)");
}

fn emit_join(out: &mut String, name: &str, separator: &Expr) {
    out.push_str("\"$(");
    emit_join_command(out, name, separator);
    out.push_str(")\"");
}

fn emit_join_value(out: &mut String, value: &Expr, separator: &Expr) {
    out.push_str("\"$(");
    if let Expr::Array(values) = value {
        out.push_str("__nacre_join_value=");
        emit_array(out, values);
        out.push_str("; ");
    }
    emit_join_command(out, "__nacre_join_value", separator);
    out.push_str(")\"");
}

fn emit_join_command(out: &mut String, name: &str, separator: &Expr) {
    out.push_str("__nacre_join_first=true; for __nacre_join_item in \"${");
    out.push_str(name);
    out.push_str("[@]}\"; do if [ \"$__nacre_join_first\" = true ]; then __nacre_join_first=false; else printf '%s' ");
    emit_expr(out, separator);
    out.push_str("; fi; printf '%s' \"$__nacre_join_item\"; done");
}

fn emit_array_push(out: &mut String, name: &str, value: &Expr) {
    out.push_str(name);
    out.push_str("+=(");
    emit_array_element(out, value);
    out.push_str(")\n");
}

fn emit_array_pop(out: &mut String, name: &str) {
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

fn emit_map_set(out: &mut String, name: &str, key: &Expr, value: &Expr) {
    out.push_str("__nacre_map_key=");
    emit_array_element(out, key);
    out.push('\n');
    out.push_str(name);
    out.push_str("[\"$__nacre_map_key\"]=");
    emit_array_element(out, value);
    out.push('\n');
}

fn emit_map_remove(out: &mut String, name: &str, key: &Expr) {
    out.push_str("__nacre_map_key=");
    emit_array_element(out, key);
    out.push('\n');
    out.push_str("unset \"");
    out.push_str(name);
    out.push_str("[$__nacre_map_key]\"\n");
}

fn emit_std_str_join(out: &mut String, args: &[Expr]) {
    if let [Expr::Ident(name), separator] = args {
        emit_join(out, name, separator);
    } else {
        emit_call(out, "str.join", args);
    }
}

fn emit_option_some(out: &mut String, value: &Expr) {
    out.push_str("$(printf '1%s' ");
    emit_call_arg(out, value);
    out.push(')');
}

fn emit_option_map(out: &mut String, name: &str, mapper: &Expr) {
    out.push_str("$(__nacre_option=\"$");
    out.push_str(name);
    out.push_str("\"; ");
    emit_option_map_case(out, mapper);
    out.push(')');
}

fn emit_option_map_value(out: &mut String, value: &Expr, mapper: &Expr) {
    out.push_str("$(__nacre_option=");
    emit_expr(out, value);
    out.push_str("; ");
    emit_option_map_case(out, mapper);
    out.push(')');
}

fn emit_option_map_case(out: &mut String, mapper: &Expr) {
    out.push_str("case \"$__nacre_option\" in 1*) printf '1%s' \"$(");
    emit_mapper_command(out, mapper);
    out.push_str(" \"${__nacre_option#?}\")\" ;; *) printf '0' ;; esac");
}

fn emit_option_flat_map(out: &mut String, name: &str, mapper: &Expr) {
    out.push_str("$(__nacre_option=\"$");
    out.push_str(name);
    out.push_str("\"; ");
    emit_option_flat_map_case(out, mapper);
    out.push(')');
}

fn emit_option_flat_map_value(out: &mut String, value: &Expr, mapper: &Expr) {
    out.push_str("$(__nacre_option=");
    emit_expr(out, value);
    out.push_str("; ");
    emit_option_flat_map_case(out, mapper);
    out.push(')');
}

fn emit_option_flat_map_case(out: &mut String, mapper: &Expr) {
    out.push_str("case \"$__nacre_option\" in 1*) printf '%s' \"$(");
    emit_mapper_command(out, mapper);
    out.push_str(" \"${__nacre_option#?}\")\" ;; *) printf '0' ;; esac");
}

fn emit_option_ap(out: &mut String, name: &str, value: &Expr) {
    out.push_str("$(__nacre_function=\"$");
    out.push_str(name);
    out.push_str("\"; ");
    emit_option_ap_case(out, value);
    out.push(')');
}

fn emit_option_ap_value(out: &mut String, function: &Expr, value: &Expr) {
    out.push_str("$(__nacre_function=");
    emit_expr(out, function);
    out.push_str("; ");
    emit_option_ap_case(out, value);
    out.push(')');
}

fn emit_option_ap_case(out: &mut String, value: &Expr) {
    out.push_str("case \"$__nacre_function\" in 1*) __nacre_value=");
    emit_expr(out, value);
    out.push_str(
        "; case \"$__nacre_value\" in 1*) printf '1%s' \"$(__nacre_call \"${__nacre_function#?}\" \"${__nacre_value#?}\")\" ;; *) printf '0' ;; esac ;; *) printf '0' ;; esac",
    );
}

fn emit_option_or_else(out: &mut String, name: &str, fallback: &Expr) {
    out.push_str("$(__nacre_option=\"$");
    out.push_str(name);
    out.push_str("\"; ");
    emit_option_or_else_case(out, fallback);
    out.push(')');
}

fn emit_option_or_else_value(out: &mut String, value: &Expr, fallback: &Expr) {
    out.push_str("$(__nacre_option=");
    emit_expr(out, value);
    out.push_str("; ");
    emit_option_or_else_case(out, fallback);
    out.push(')');
}

fn emit_option_or_else_try(out: &mut String, value: &Expr, fallback: &Expr) {
    out.push_str("$(__nacre_option=");
    emit_expr(out, value);
    out.push_str(
        "; case \"$__nacre_option\" in 1*) printf '1%s' \"$__nacre_option\" ;; *) printf '%s' ",
    );
    emit_expr(out, fallback);
    out.push_str(" ;; esac)");
}

fn emit_option_or_else_case(out: &mut String, fallback: &Expr) {
    out.push_str(
        "case \"$__nacre_option\" in 1*) printf '%s' \"$__nacre_option\" ;; *) printf '%s' ",
    );
    emit_call_arg(out, fallback);
    out.push_str(" ;; esac");
}

fn emit_result_err(out: &mut String, value: &Expr) {
    out.push_str("$(printf '0%s' ");
    emit_call_arg(out, value);
    out.push(')');
}

fn emit_result_map(out: &mut String, name: &str, mapper: &Expr) {
    out.push_str("$(__nacre_result=\"$");
    out.push_str(name);
    out.push_str("\"; ");
    emit_result_map_case(out, mapper);
    out.push(')');
}

fn emit_result_map_value(out: &mut String, value: &Expr, mapper: &Expr) {
    out.push_str("$(__nacre_result=");
    emit_expr(out, value);
    out.push_str("; ");
    emit_result_map_case(out, mapper);
    out.push(')');
}

fn emit_result_map_case(out: &mut String, mapper: &Expr) {
    out.push_str("case \"$__nacre_result\" in 1*) printf '1%s' \"$(");
    emit_mapper_command(out, mapper);
    out.push_str(" \"${__nacre_result#?}\")\" ;; *) printf '%s' \"$__nacre_result\" ;; esac");
}

fn emit_result_flat_map(out: &mut String, name: &str, mapper: &Expr) {
    out.push_str("$(__nacre_result=\"$");
    out.push_str(name);
    out.push_str("\"; ");
    emit_result_flat_map_case(out, mapper);
    out.push(')');
}

fn emit_result_flat_map_value(out: &mut String, value: &Expr, mapper: &Expr) {
    out.push_str("$(__nacre_result=");
    emit_expr(out, value);
    out.push_str("; ");
    emit_result_flat_map_case(out, mapper);
    out.push(')');
}

fn emit_result_flat_map_case(out: &mut String, mapper: &Expr) {
    out.push_str("case \"$__nacre_result\" in 1*) printf '%s' \"$(");
    emit_mapper_command(out, mapper);
    out.push_str(" \"${__nacre_result#?}\")\" ;; *) printf '%s' \"$__nacre_result\" ;; esac");
}

fn emit_result_ap(out: &mut String, name: &str, value: &Expr) {
    out.push_str("$(__nacre_function=\"$");
    out.push_str(name);
    out.push_str("\"; ");
    emit_result_ap_case(out, value);
    out.push(')');
}

fn emit_result_ap_value(out: &mut String, function: &Expr, value: &Expr) {
    out.push_str("$(__nacre_function=");
    emit_expr(out, function);
    out.push_str("; ");
    emit_result_ap_case(out, value);
    out.push(')');
}

fn emit_result_ap_case(out: &mut String, value: &Expr) {
    out.push_str("case \"$__nacre_function\" in 1*) __nacre_value=");
    emit_expr(out, value);
    out.push_str(
        "; case \"$__nacre_value\" in 1*) printf '1%s' \"$(__nacre_call \"${__nacre_function#?}\" \"${__nacre_value#?}\")\" ;; *) printf '%s' \"$__nacre_value\" ;; esac ;; *) printf '%s' \"$__nacre_function\" ;; esac",
    );
}

fn emit_result_option(out: &mut String, value: &Expr) {
    match value {
        Expr::Command { command, .. } => emit_command_option(out, command),
        Expr::Pipeline { input, commands } => {
            let mut command = String::new();
            emit_pipeline(&mut command, input.as_deref(), commands);
            emit_command_option(out, &command);
        }
        _ => emit_expr(out, value),
    }
}

fn emit_default(out: &mut String, value: &Expr, fallback: &Expr) {
    match value {
        Expr::Command { command, .. } => emit_command_default(out, command, fallback),
        Expr::Pipeline { input, commands } => {
            let mut command = String::new();
            emit_pipeline(&mut command, input.as_deref(), commands);
            emit_command_default(out, &command, fallback);
        }
        _ => emit_option_default(out, value, fallback),
    }
}

fn emit_default_try(out: &mut String, value: &Expr, fallback: &Expr) {
    match value {
        Expr::Command { command, .. } => {
            out.push_str("$(if __nacre_output=\"$(");
            emit_shell_command(out, command);
            out.push_str(")\"; then printf '1%s' \"$__nacre_output\"; else printf '%s' ");
            emit_expr(out, fallback);
            out.push_str("; fi)");
        }
        Expr::Pipeline { input, commands } => {
            out.push_str("$(if __nacre_output=\"$(");
            emit_pipeline(out, input.as_deref(), commands);
            out.push_str(")\"; then printf '1%s' \"$__nacre_output\"; else printf '%s' ");
            emit_expr(out, fallback);
            out.push_str("; fi)");
        }
        _ => {
            out.push_str("$(__nacre_option=");
            emit_expr(out, value);
            out.push_str(
                "; case \"$__nacre_option\" in 1*) printf '1%s' \"${__nacre_option#?}\" ;; *) printf '%s' ",
            );
            emit_expr(out, fallback);
            out.push_str(" ;; esac)");
        }
    }
}

fn emit_command_option(out: &mut String, command: &str) {
    out.push_str("$(if __nacre_output=\"$(");
    emit_shell_command(out, command);
    out.push_str(")\"; then printf '1%s' \"$__nacre_output\"; else printf '0'; fi)");
}

fn emit_command_default(out: &mut String, command: &str, fallback: &Expr) {
    out.push_str("$(if __nacre_output=\"$(");
    emit_shell_command(out, command);
    out.push_str(")\"; then printf '%s' \"$__nacre_output\"; else printf '%s' ");
    emit_call_arg(out, fallback);
    out.push_str("; fi)");
}

fn emit_option_default(out: &mut String, value: &Expr, fallback: &Expr) {
    out.push_str("$(__nacre_option=");
    emit_expr(out, value);
    out.push_str(
        "; case \"$__nacre_option\" in 1*) printf '%s' \"${__nacre_option#?}\" ;; *) printf '%s' ",
    );
    emit_call_arg(out, fallback);
    out.push_str(" ;; esac)");
}

fn emit_array(out: &mut String, values: &[Expr]) {
    out.push('(');
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        emit_array_element(out, value);
    }
    out.push(')');
}

fn emit_array_element(out: &mut String, expr: &Expr) {
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
        Expr::MatchGuardResult(value) => emit_array_element(out, value),
        Expr::None => emit_shell_word(out, "0"),
        Expr::Default { value, fallback } => emit_default(out, value, fallback),
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
        Expr::NewtypeCtor { value, .. } => emit_array_element(out, value),
        Expr::Variant { .. } => emit_expr(out, expr),
        Expr::Cast { expr, .. } => emit_array_element(out, expr),
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
        | Expr::Await(_)
        | Expr::Pipeline { .. }
        | Expr::TryPipeline { .. }
        | Expr::TryResult(_)
        | Expr::DefaultTry { .. }
        | Expr::PipelineResult { .. }
        | Expr::OptionOrElseTry { .. }
        | Expr::HasCommand(_)
        | Expr::PathExists(_)
        | Expr::Array(_)
        | Expr::Map(_)
        | Expr::Record(_)
        | Expr::RecordPattern(_)
        | Expr::Tuple(_)
        | Expr::Binary { .. }
        | Expr::LetIn { .. } => emit_expr(out, expr),
        Expr::Do { .. } => unreachable!("do expressions are lowered before emission"),
        Expr::Closure { name, captures } => emit_closure(out, name, captures),
        Expr::Lambda { .. } => unreachable!("lambdas are lowered before emission"),
    }
}

fn emit_call(out: &mut String, name: &str, args: &[Expr]) {
    out.push_str("\"$(");
    emit_call_command(out, name, args);
    out.push_str(")\"");
}

fn emit_call_command(out: &mut String, name: &str, args: &[Expr]) {
    emit_call_head(out, name);
    for arg in args {
        out.push(' ');
        emit_call_arg(out, arg);
    }
}

fn emit_call_head(out: &mut String, name: &str) {
    if is_shell_name(name) {
        out.push_str("__nacre_call \"$");
        out.push_str(name);
        out.push('"');
    } else {
        out.push_str(name);
    }
}

fn emit_variant(out: &mut String, name: &str, args: &[Expr], field_types: &[Type]) {
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

fn emit_snapshot_declarations(out: &mut String, source: &str, target: &str, ty: &Type) {
    for suffix in value_suffixes(ty) {
        out.push_str("__nacre_capture ");
        emit_shell_word(out, &format!("{source}{suffix}"));
        out.push(' ');
        emit_shell_word(out, &format!("{target}{suffix}"));
        out.push_str("; printf '\\n'; ");
    }
}

fn emit_if_expr(out: &mut String, condition: &Expr, then_expr: &Expr, else_expr: &Expr) {
    out.push_str("$(if ");
    emit_condition(out, condition);
    out.push_str("; then printf '%s\\n' ");
    emit_expr(out, then_expr);
    out.push_str("; else printf '%s\\n' ");
    emit_expr(out, else_expr);
    out.push_str("; fi)");
}

fn emit_call_arg(out: &mut String, arg: &Expr) {
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
        | Expr::Await(_)
        | Expr::Pipeline { .. }
        | Expr::TryPipeline { .. }
        | Expr::TryResult(_)
        | Expr::PipelineResult { .. }
        | Expr::HasCommand(_)
        | Expr::PathExists(_)
        | Expr::Array(_)
        | Expr::Map(_)
        | Expr::Tuple(_)
        | Expr::Record(_)
        | Expr::RecordPattern(_)
        | Expr::Binary { .. }
        | Expr::LetIn { .. } => emit_expr(out, arg),
        Expr::Do { .. } => unreachable!("do expressions are lowered before emission"),
        Expr::Closure { name, captures } => emit_closure(out, name, captures),
        Expr::Lambda { .. } => unreachable!("lambdas are lowered before emission"),
    }
}

fn emit_map(out: &mut String, entries: &[(Expr, Expr)]) {
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

fn emit_map_key(out: &mut String, expr: &Expr) {
    match expr {
        Expr::String(value) | Expr::RawString(value) => emit_shell_word(out, value),
        Expr::Int(value) => out.push_str(&value.to_string()),
        Expr::Ident(name) => emit_ident_value(out, name),
        _ => emit_expr(out, expr),
    }
}

#[cfg(test)]
mod tests;
