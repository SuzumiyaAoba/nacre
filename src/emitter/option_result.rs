use super::collections::emit_mapper_command;
use super::expressions::{emit_call_arg, emit_expr};
use super::shell::{emit_pipeline, emit_shell_command};
use crate::Expr;

pub(super) fn emit_option_some(out: &mut String, value: &Expr) {
    out.push_str("$(printf '1%s' ");
    emit_call_arg(out, value);
    out.push(')');
}

pub(super) fn emit_option_map(out: &mut String, name: &str, mapper: &Expr) {
    out.push_str("$(__nacre_option=\"$");
    out.push_str(name);
    out.push_str("\"; ");
    emit_option_map_case(out, mapper);
    out.push(')');
}

pub(super) fn emit_option_map_value(out: &mut String, value: &Expr, mapper: &Expr) {
    out.push_str("$(__nacre_option=");
    emit_expr(out, value);
    out.push_str("; ");
    emit_option_map_case(out, mapper);
    out.push(')');
}

pub(super) fn emit_option_map_case(out: &mut String, mapper: &Expr) {
    out.push_str("case \"$__nacre_option\" in 1*) printf '1%s' \"$(");
    emit_mapper_command(out, mapper);
    out.push_str(" \"${__nacre_option#?}\")\" ;; *) printf '0' ;; esac");
}

pub(super) fn emit_option_flat_map(out: &mut String, name: &str, mapper: &Expr) {
    out.push_str("$(__nacre_option=\"$");
    out.push_str(name);
    out.push_str("\"; ");
    emit_option_flat_map_case(out, mapper);
    out.push(')');
}

pub(super) fn emit_option_flat_map_value(out: &mut String, value: &Expr, mapper: &Expr) {
    out.push_str("$(__nacre_option=");
    emit_expr(out, value);
    out.push_str("; ");
    emit_option_flat_map_case(out, mapper);
    out.push(')');
}

pub(super) fn emit_option_flat_map_case(out: &mut String, mapper: &Expr) {
    out.push_str("case \"$__nacre_option\" in 1*) printf '%s' \"$(");
    emit_mapper_command(out, mapper);
    out.push_str(" \"${__nacre_option#?}\")\" ;; *) printf '0' ;; esac");
}

pub(super) fn emit_option_ap(out: &mut String, name: &str, value: &Expr) {
    out.push_str("$(__nacre_function=\"$");
    out.push_str(name);
    out.push_str("\"; ");
    emit_option_ap_case(out, value);
    out.push(')');
}

pub(super) fn emit_option_ap_value(out: &mut String, function: &Expr, value: &Expr) {
    out.push_str("$(__nacre_function=");
    emit_expr(out, function);
    out.push_str("; ");
    emit_option_ap_case(out, value);
    out.push(')');
}

pub(super) fn emit_option_ap_case(out: &mut String, value: &Expr) {
    out.push_str("case \"$__nacre_function\" in 1*) __nacre_value=");
    emit_expr(out, value);
    out.push_str(
        "; case \"$__nacre_value\" in 1*) printf '1%s' \"$(__nacre_call \"${__nacre_function#?}\" \"${__nacre_value#?}\")\" ;; *) printf '0' ;; esac ;; *) printf '0' ;; esac",
    );
}

pub(super) fn emit_option_or_else(out: &mut String, name: &str, fallback: &Expr) {
    out.push_str("$(__nacre_option=\"$");
    out.push_str(name);
    out.push_str("\"; ");
    emit_option_or_else_case(out, fallback);
    out.push(')');
}

pub(super) fn emit_option_or_else_value(out: &mut String, value: &Expr, fallback: &Expr) {
    out.push_str("$(__nacre_option=");
    emit_expr(out, value);
    out.push_str("; ");
    emit_option_or_else_case(out, fallback);
    out.push(')');
}

pub(super) fn emit_option_or_else_try(out: &mut String, value: &Expr, fallback: &Expr) {
    out.push_str("$(__nacre_option=");
    emit_expr(out, value);
    out.push_str(
        "; case \"$__nacre_option\" in 1*) printf '1%s' \"$__nacre_option\" ;; *) printf '%s' ",
    );
    emit_expr(out, fallback);
    out.push_str(" ;; esac)");
}

pub(super) fn emit_option_or_else_case(out: &mut String, fallback: &Expr) {
    out.push_str(
        "case \"$__nacre_option\" in 1*) printf '%s' \"$__nacre_option\" ;; *) printf '%s' ",
    );
    emit_call_arg(out, fallback);
    out.push_str(" ;; esac");
}

pub(super) fn emit_result_err(out: &mut String, value: &Expr) {
    out.push_str("$(printf '0%s' ");
    emit_call_arg(out, value);
    out.push(')');
}

pub(super) fn emit_result_map(out: &mut String, name: &str, mapper: &Expr) {
    out.push_str("$(__nacre_result=\"$");
    out.push_str(name);
    out.push_str("\"; ");
    emit_result_map_case(out, mapper);
    out.push(')');
}

pub(super) fn emit_result_map_value(out: &mut String, value: &Expr, mapper: &Expr) {
    out.push_str("$(__nacre_result=");
    emit_expr(out, value);
    out.push_str("; ");
    emit_result_map_case(out, mapper);
    out.push(')');
}

pub(super) fn emit_result_map_case(out: &mut String, mapper: &Expr) {
    out.push_str("case \"$__nacre_result\" in 1*) printf '1%s' \"$(");
    emit_mapper_command(out, mapper);
    out.push_str(" \"${__nacre_result#?}\")\" ;; *) printf '%s' \"$__nacre_result\" ;; esac");
}

pub(super) fn emit_result_flat_map(out: &mut String, name: &str, mapper: &Expr) {
    out.push_str("$(__nacre_result=\"$");
    out.push_str(name);
    out.push_str("\"; ");
    emit_result_flat_map_case(out, mapper);
    out.push(')');
}

pub(super) fn emit_result_flat_map_value(out: &mut String, value: &Expr, mapper: &Expr) {
    out.push_str("$(__nacre_result=");
    emit_expr(out, value);
    out.push_str("; ");
    emit_result_flat_map_case(out, mapper);
    out.push(')');
}

pub(super) fn emit_result_flat_map_case(out: &mut String, mapper: &Expr) {
    out.push_str("case \"$__nacre_result\" in 1*) printf '%s' \"$(");
    emit_mapper_command(out, mapper);
    out.push_str(" \"${__nacre_result#?}\")\" ;; *) printf '%s' \"$__nacre_result\" ;; esac");
}

pub(super) fn emit_result_ap(out: &mut String, name: &str, value: &Expr) {
    out.push_str("$(__nacre_function=\"$");
    out.push_str(name);
    out.push_str("\"; ");
    emit_result_ap_case(out, value);
    out.push(')');
}

pub(super) fn emit_result_ap_value(out: &mut String, function: &Expr, value: &Expr) {
    out.push_str("$(__nacre_function=");
    emit_expr(out, function);
    out.push_str("; ");
    emit_result_ap_case(out, value);
    out.push(')');
}

pub(super) fn emit_result_ap_case(out: &mut String, value: &Expr) {
    out.push_str("case \"$__nacre_function\" in 1*) __nacre_value=");
    emit_expr(out, value);
    out.push_str(
        "; case \"$__nacre_value\" in 1*) printf '1%s' \"$(__nacre_call \"${__nacre_function#?}\" \"${__nacre_value#?}\")\" ;; *) printf '%s' \"$__nacre_value\" ;; esac ;; *) printf '%s' \"$__nacre_function\" ;; esac",
    );
}

pub(super) fn emit_result_option(out: &mut String, value: &Expr) {
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

pub(super) fn emit_default(out: &mut String, value: &Expr, fallback: &Expr) {
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

pub(super) fn emit_default_try(out: &mut String, value: &Expr, fallback: &Expr) {
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

pub(super) fn emit_command_option(out: &mut String, command: &str) {
    out.push_str("$(if __nacre_output=\"$(");
    emit_shell_command(out, command);
    out.push_str(")\"; then printf '1%s' \"$__nacre_output\"; else printf '0'; fi)");
}

pub(super) fn emit_command_default(out: &mut String, command: &str, fallback: &Expr) {
    out.push_str("$(if __nacre_output=\"$(");
    emit_shell_command(out, command);
    out.push_str(")\"; then printf '%s' \"$__nacre_output\"; else printf '%s' ");
    emit_call_arg(out, fallback);
    out.push_str("; fi)");
}

pub(super) fn emit_option_default(out: &mut String, value: &Expr, fallback: &Expr) {
    out.push_str("$(__nacre_option=");
    emit_expr(out, value);
    out.push_str(
        "; case \"$__nacre_option\" in 1*) printf '%s' \"${__nacre_option#?}\" ;; *) printf '%s' ",
    );
    emit_call_arg(out, fallback);
    out.push_str(" ;; esac)");
}
