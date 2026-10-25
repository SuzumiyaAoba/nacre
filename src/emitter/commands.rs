use super::collections::emit_array_expansion_binding;
use super::expressions::{emit_call_arg, emit_expr};
use super::quoting::{emit_bash_string, emit_shell_word, emit_string};
use super::shell::emit_shell_command;
use super::AllowedCommandParts;
use crate::Expr;

pub(super) fn emit_redirect(
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

pub(super) fn emit_require_one_of(out: &mut String, commands: &[String]) {
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

pub(super) fn emit_require(out: &mut String, command: &str, version: Option<&str>) {
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

pub(super) fn emit_allowed_command(
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

pub(super) fn emit_allowed_command_result_binding(
    out: &mut String,
    name: &str,
    command: AllowedCommandParts<'_>,
    readonly: bool,
    local: bool,
) {
    if local {
        out.push_str("local ");
        out.push_str(name);
        out.push_str("_stdout ");
        out.push_str(name);
        out.push_str("_stderr ");
        out.push_str(name);
        out.push_str("_status ");
        out.push_str(name);
        out.push_str("_success\n");
    }
    let Some(program) = command.program else {
        out.push_str(name);
        out.push_str("_stdout=''\n");
        out.push_str(name);
        out.push_str("_stderr='nacre: unresolved command policy'\n");
        out.push_str(name);
        out.push_str("_status=126\n");
        out.push_str(name);
        out.push_str("_success=false\n");
        return;
    };

    emit_allowed_command_args(out, command.args, command.read_args, command.write_args);
    out.push_str("__nacre_result_stderr_file=\"$(mktemp)\"\n");
    out.push_str("if ");
    out.push_str(name);
    out.push_str("_stdout=\"$({ ");
    emit_allowed_command_invocation(out, program, command.args.len());
    out.push_str("; } 2>\"$__nacre_result_stderr_file\")\"; then\n");
    out.push_str(name);
    out.push_str("_status=0\n");
    out.push_str(name);
    out.push_str("_stderr=''\n");
    out.push_str(name);
    out.push_str("_success=true\n");
    out.push_str("else\n__nacre_result_status=$?\n");
    out.push_str(name);
    out.push_str("_status=\"$__nacre_result_status\"\n");
    out.push_str(name);
    out.push_str("_stderr=\"$(cat \"$__nacre_result_stderr_file\")\"\n");
    out.push_str(name);
    out.push_str("_success=false\nfi\n");
    out.push_str("rm -f \"$__nacre_result_stderr_file\"\n");
    if readonly && !local {
        out.push_str("readonly ");
        out.push_str(name);
        out.push_str("_stdout ");
        out.push_str(name);
        out.push_str("_stderr ");
        out.push_str(name);
        out.push_str("_status ");
        out.push_str(name);
        out.push_str("_success\n");
    }
}

pub(super) fn emit_allowed_command_result_discard(
    out: &mut String,
    command: AllowedCommandParts<'_>,
) {
    let Some(program) = command.program else {
        out.push_str("printf 'nacre: unresolved command policy\\n' >&2; exit 126");
        return;
    };
    emit_allowed_command_args(out, command.args, command.read_args, command.write_args);
    out.push_str("__nacre_result_stderr_file=\"$(mktemp)\"; { ");
    emit_allowed_command_invocation(out, program, command.args.len());
    out.push_str("; } >/dev/null 2>\"$__nacre_result_stderr_file\" || true; ");
    out.push_str("rm -f \"$__nacre_result_stderr_file\"");
}

pub(super) fn emit_allowed_command_args(
    out: &mut String,
    args: &[Expr],
    read_args: &[usize],
    write_args: &[usize],
) {
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
}

pub(super) fn emit_allowed_command_invocation(out: &mut String, program: &str, arg_count: usize) {
    emit_bash_string(out, program);
    for index in 0..arg_count {
        out.push_str(" \"$__nacre_run_arg_");
        out.push_str(&index.to_string());
        out.push('"');
    }
}

pub(super) fn emit_process_env(out: &mut String, name: &Expr) {
    out.push_str("\"$(__nacre_env_name=");
    emit_call_arg(out, name);
    out.push_str("; if [[ \"$__nacre_env_name\" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]]; then printf '%s' \"${!__nacre_env_name-}\"; fi)\"");
}

pub(super) fn emit_process_args_binding(
    out: &mut String,
    binding: &str,
    readonly: bool,
    local: bool,
) {
    emit_array_expansion_binding(out, binding, "${args[@]}", readonly, local);
}

pub(super) fn emit_cli_parse_binding(out: &mut String, binding: &str, readonly: bool, local: bool) {
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
