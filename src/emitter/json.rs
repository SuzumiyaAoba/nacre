use super::expressions::emit_call_arg;
use super::quoting::emit_shell_word;
use super::values::emit_map;
use crate::Expr;

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

pub(super) fn emit_json_parse_binding(
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

pub(super) fn emit_json_stringify(out: &mut String, name: &str) {
    out.push_str("\"$(");
    emit_json_stringify_command(out, name);
    out.push_str(")\"");
}

pub(super) fn emit_json_stringify_value(out: &mut String, value: &Expr) {
    out.push_str("\"$(");
    emit_map_expr_binding(out, "__nacre_json_map", value);
    emit_json_stringify_command(out, "__nacre_json_map");
    out.push_str(")\"");
}

pub(super) fn emit_map_expr_binding(out: &mut String, name: &str, value: &Expr) {
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

pub(super) fn emit_json_stringify_command(out: &mut String, name: &str) {
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
