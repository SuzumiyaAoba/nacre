use super::collections::emit_array_words;
use super::expressions::{emit_call_arg, emit_expr};
use super::strings::emit_checked_string_predicate_value;
use crate::Expr;

pub(super) fn emit_path_exists(out: &mut String, path: &Expr) {
    out.push_str("$(__nacre_guarded_path=");
    emit_expr(out, path);
    out.push_str("; __nacre_guarded_path=\"$(__nacre_checked_path read \"$__nacre_guarded_path\")\" || exit $?; if [ -e \"$__nacre_guarded_path\" ]; then printf true; else printf false; fi)");
}

pub(super) fn emit_fs_test(out: &mut String, test: &str, path: &Expr) {
    out.push_str("$(__nacre_guarded_path=");
    emit_expr(out, path);
    out.push_str("; __nacre_guarded_path=\"$(__nacre_checked_path read \"$__nacre_guarded_path\")\" || exit $?; if [ ");
    out.push_str(test);
    out.push_str(" \"$__nacre_guarded_path\" ]; then printf true; else printf false; fi)");
}

pub(super) fn emit_fs_size(out: &mut String, path: &Expr) {
    out.push_str("$(__nacre_guarded_path=");
    emit_expr(out, path);
    out.push_str("; __nacre_guarded_path=\"$(__nacre_checked_path read \"$__nacre_guarded_path\")\" || exit $?; wc -c < \"$__nacre_guarded_path\" | tr -d '[:space:]')");
}

pub(super) fn emit_fs_read_lines_binding(
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

pub(super) fn emit_fs_read_lines_value(out: &mut String, path: &Expr) {
    out.push_str("\"$(__nacre_guarded_path=");
    emit_expr(out, path);
    out.push_str("; __nacre_guarded_path=\"$(__nacre_checked_path read \"$__nacre_guarded_path\")\" || exit $?; printf '%s' \"$(<\"$__nacre_guarded_path\")\")\"");
}

pub(super) fn emit_fs_list_binding(
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

pub(super) fn emit_fs_list_value(out: &mut String, path: &Expr) {
    out.push_str("\"$(");
    emit_fs_list_command(out, path);
    out.push_str(")\"");
}

pub(super) fn emit_fs_list_command(out: &mut String, path: &Expr) {
    out.push_str("__nacre_guarded_path=");
    emit_expr(out, path);
    out.push_str("; __nacre_guarded_path=\"$(__nacre_checked_path read \"$__nacre_guarded_path\")\" || exit $?; find \"$__nacre_guarded_path\" -mindepth 1 -maxdepth 1 -print | sort");
}

pub(super) fn emit_fs_write_lines_statement(out: &mut String, path: &Expr, lines: &Expr) {
    emit_fs_write_lines_command(out, path, lines);
    out.push('\n');
}

pub(super) fn emit_fs_append_lines_statement(out: &mut String, path: &Expr, lines: &Expr) {
    emit_fs_append_lines_command(out, path, lines);
    out.push('\n');
}

pub(super) fn emit_fs_write_lines_expr(out: &mut String, path: &Expr, lines: &Expr) {
    out.push_str("\"$(");
    emit_fs_write_lines_command(out, path, lines);
    out.push_str(")\"");
}

pub(super) fn emit_fs_append_lines_expr(out: &mut String, path: &Expr, lines: &Expr) {
    out.push_str("\"$(");
    emit_fs_append_lines_command(out, path, lines);
    out.push_str(")\"");
}

pub(super) fn emit_fs_write_lines_command(out: &mut String, path: &Expr, lines: &Expr) {
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

pub(super) fn emit_fs_append_lines_command(out: &mut String, path: &Expr, lines: &Expr) {
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

#[derive(Clone, Copy)]
pub(super) enum PathMethod {
    Basename,
    Dirname,
    Stem,
    Extname,
    IsAbsolute,
}

pub(super) fn emit_path_method_value_binding(
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

pub(super) fn emit_path_method_name(out: &mut String, name: &str, method: PathMethod) {
    match method {
        PathMethod::Basename => emit_path_basename(out, name),
        PathMethod::Dirname => emit_path_dirname(out, name),
        PathMethod::Stem => emit_path_stem(out, name),
        PathMethod::Extname => emit_path_extname(out, name),
        PathMethod::IsAbsolute => emit_path_is_absolute(out, name),
    }
}

pub(super) fn emit_path_method_expr(out: &mut String, value: &Expr, method: PathMethod) {
    match method {
        PathMethod::Basename => emit_path_basename_expr(out, value),
        PathMethod::Dirname => emit_path_dirname_expr(out, value),
        PathMethod::Stem => emit_path_stem_expr(out, value),
        PathMethod::Extname => emit_path_extname_expr(out, value),
        PathMethod::IsAbsolute => emit_path_is_absolute_expr(out, value),
    }
}

pub(super) fn emit_path_basename(out: &mut String, name: &str) {
    out.push_str("\"$(basename \"$");
    out.push_str(name);
    out.push_str("\")\"");
}

pub(super) fn emit_path_basename_expr(out: &mut String, value: &Expr) {
    out.push_str("\"$(basename ");
    emit_call_arg(out, value);
    out.push_str(")\"");
}

pub(super) fn emit_path_dirname(out: &mut String, name: &str) {
    out.push_str("\"$(dirname \"$");
    out.push_str(name);
    out.push_str("\")\"");
}

pub(super) fn emit_path_dirname_expr(out: &mut String, value: &Expr) {
    out.push_str("\"$(dirname ");
    emit_call_arg(out, value);
    out.push_str(")\"");
}

pub(super) fn emit_path_stem(out: &mut String, name: &str) {
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

pub(super) fn emit_path_stem_expr(out: &mut String, value: &Expr) {
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

pub(super) fn emit_path_extname(out: &mut String, name: &str) {
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

pub(super) fn emit_path_extname_expr(out: &mut String, value: &Expr) {
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

pub(super) fn emit_path_is_absolute(out: &mut String, name: &str) {
    out.push_str("$(case \"$");
    out.push_str(name);
    out.push_str("\" in /*) printf true ;; *) printf false ;; esac)");
}

pub(super) fn emit_path_is_absolute_expr(out: &mut String, value: &Expr) {
    out.push_str("$(case ");
    emit_call_arg(out, value);
    out.push_str(" in /*) printf true ;; *) printf false ;; esac)");
}
