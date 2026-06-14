use crate::policy::ExecutionPolicy;
use crate::{Program, Statement};

use super::quoting::emit_bash_string;

pub(super) fn emit_policy_runtime(out: &mut String, policy: &ExecutionPolicy) {
    out.push_str("__nacre_read_roots=(");
    for root in policy.read_roots() {
        out.push(' ');
        emit_bash_string(out, &root.to_string_lossy());
    }
    out.push_str(" )\n__nacre_write_roots=(");
    for root in policy.write_roots() {
        out.push(' ');
        emit_bash_string(out, &root.to_string_lossy());
    }
    out.push_str(
        r#" )
__nacre_resolve_guarded_path() {
  local __nacre_path="$1"
  if [[ "$__nacre_path" != /* ]]; then
    __nacre_path="$PWD/$__nacre_path"
  fi
  if [[ -L "$__nacre_path" ]]; then
    return 1
  fi
  if [[ -d "$__nacre_path" ]]; then
    (cd -P -- "$__nacre_path" && pwd -P)
    return
  fi
  local __nacre_parent="${__nacre_path%/*}"
  local __nacre_base="${__nacre_path##*/}"
  if [[ ! -d "$__nacre_parent" ]]; then
    return 1
  fi
  __nacre_parent="$(cd -P -- "$__nacre_parent" && pwd -P)" || return 1
  printf '%s/%s\n' "$__nacre_parent" "$__nacre_base"
}

__nacre_assert_path_in_roots() {
  local __nacre_access="$1"
  local __nacre_path="$2"
  local __nacre_resolved
  __nacre_resolved="$(__nacre_resolve_guarded_path "$__nacre_path")" || {
    printf 'nacre: denied %s path: %s\n' "$__nacre_access" "$__nacre_path" >&2
    return 126
  }
  local -a __nacre_roots
  if [[ "$__nacre_access" == read ]]; then
    __nacre_roots=("${__nacre_read_roots[@]}")
  else
    __nacre_roots=("${__nacre_write_roots[@]}")
  fi
  local __nacre_root
  for __nacre_root in "${__nacre_roots[@]}"; do
    if [[ "$__nacre_resolved" == "$__nacre_root" || "$__nacre_resolved" == "$__nacre_root/"* ]]; then
      return 0
    fi
  done
  printf 'nacre: denied %s path: %s\n' "$__nacre_access" "$__nacre_path" >&2
  return 126
}
"#,
    );
}

pub(super) fn program_needs_runtime(program: &Program) -> bool {
    program.statements().iter().any(statement_needs_runtime)
}

fn statement_needs_runtime(statement: &Statement) -> bool {
    match statement {
        Statement::Function { .. }
        | Statement::ExternalFunction { .. }
        | Statement::Impl { .. } => true,
        Statement::SumType { variants, .. } => {
            variants.iter().any(|variant| !variant.fields.is_empty())
        }
        Statement::Block { body } | Statement::While { body, .. } | Statement::For { body, .. } => {
            program_needs_runtime(body)
        }
        Statement::If {
            then_branch,
            else_branch,
            ..
        } => {
            program_needs_runtime(then_branch)
                || else_branch.as_ref().is_some_and(program_needs_runtime)
        }
        _ => false,
    }
}

pub(super) const CLOSURE_RUNTIME: &str = r#"
__nacre_closure_pack() {
  local __nacre_runtime_closure_function="$1"
  shift
  printf '__nacre_closure:%s:%s:' "$#" "$__nacre_runtime_closure_function"
  local __nacre_runtime_closure_value
  for __nacre_runtime_closure_value in "$@"; do
    printf '%s:%s' "${#__nacre_runtime_closure_value}" "$__nacre_runtime_closure_value"
  done
}

__nacre_capture() {
  local __nacre_runtime_capture_source="$1"
  local __nacre_runtime_capture_target="$2"
  local __nacre_runtime_capture_declaration
  __nacre_runtime_capture_declaration="$(declare -p "$__nacre_runtime_capture_source")"
  __nacre_runtime_capture_declaration="${__nacre_runtime_capture_declaration/ ${__nacre_runtime_capture_source}=/ ${__nacre_runtime_capture_target}=}"
  printf '%s' "$__nacre_runtime_capture_declaration"
}

__nacre_variant_pack() {
  local __nacre_runtime_variant_tag="$1"
  shift
  printf '__nacre_variant:%s:%s:%s:' "${#__nacre_runtime_variant_tag}" "$__nacre_runtime_variant_tag" "$#"
  local __nacre_runtime_variant_value
  for __nacre_runtime_variant_value in "$@"; do
    printf '%s:%s' "${#__nacre_runtime_variant_value}" "$__nacre_runtime_variant_value"
  done
}

__nacre_variant_unpack() {
  local __nacre_runtime_variant_data="${1#__nacre_variant:}"
  local __nacre_runtime_variant_length="${__nacre_runtime_variant_data%%:*}"
  __nacre_runtime_variant_data="${__nacre_runtime_variant_data#*:}"
  local __nacre_runtime_variant_tag="${__nacre_runtime_variant_data:0:__nacre_runtime_variant_length}"
  __nacre_runtime_variant_data="${__nacre_runtime_variant_data:__nacre_runtime_variant_length}"
  __nacre_runtime_variant_data="${__nacre_runtime_variant_data#:}"
  local __nacre_runtime_variant_count="${__nacre_runtime_variant_data%%:*}"
  __nacre_runtime_variant_data="${__nacre_runtime_variant_data#*:}"
  printf 'declare -- __nacre_match_tag=%q\n' "$__nacre_runtime_variant_tag"
  local __nacre_runtime_variant_value
  local __nacre_runtime_variant_index
  for ((__nacre_runtime_variant_index = 0; __nacre_runtime_variant_index < __nacre_runtime_variant_count; __nacre_runtime_variant_index++)); do
    __nacre_runtime_variant_length="${__nacre_runtime_variant_data%%:*}"
    __nacre_runtime_variant_data="${__nacre_runtime_variant_data#*:}"
    __nacre_runtime_variant_value="${__nacre_runtime_variant_data:0:__nacre_runtime_variant_length}"
    __nacre_runtime_variant_data="${__nacre_runtime_variant_data:__nacre_runtime_variant_length}"
    printf '%s\n' "$__nacre_runtime_variant_value"
  done
}

__nacre_call() {
  local __nacre_runtime_callable="$1"
  shift
  if [[ "$__nacre_runtime_callable" != __nacre_closure:* ]]; then
    "$__nacre_runtime_callable" "$@"
    return
  fi
  local __nacre_runtime_closure_data="${__nacre_runtime_callable#__nacre_closure:}"
  local __nacre_runtime_closure_count="${__nacre_runtime_closure_data%%:*}"
  __nacre_runtime_closure_data="${__nacre_runtime_closure_data#*:}"
  local __nacre_runtime_closure_function="${__nacre_runtime_closure_data%%:*}"
  __nacre_runtime_closure_data="${__nacre_runtime_closure_data#*:}"
  local __nacre_runtime_closure_length
  local __nacre_runtime_closure_value
  local __nacre_runtime_closure_index
  for ((__nacre_runtime_closure_index = 0; __nacre_runtime_closure_index < __nacre_runtime_closure_count; __nacre_runtime_closure_index++)); do
    __nacre_runtime_closure_length="${__nacre_runtime_closure_data%%:*}"
    __nacre_runtime_closure_data="${__nacre_runtime_closure_data#*:}"
    __nacre_runtime_closure_value="${__nacre_runtime_closure_data:0:__nacre_runtime_closure_length}"
    __nacre_runtime_closure_data="${__nacre_runtime_closure_data:__nacre_runtime_closure_length}"
    eval "$__nacre_runtime_closure_value"
  done
  "$__nacre_runtime_closure_function" "$@"
}
"#;

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use crate::{Expr, Param, Type};

    #[test]
    fn detects_programs_that_need_runtime_helpers() {
        let empty = Program::new(Vec::new(), Vec::new());
        assert!(!program_needs_runtime(&empty));

        let function = Program::new(
            vec![Statement::Function {
                name: "identity".into(),
                override_constructor: false,
                type_params: Vec::new(),
                params: vec![Param {
                    name: "value".into(),
                    ty: Type::String,
                    default: None,
                    variadic: false,
                    capture_name: None,
                }],
                return_type: Type::String,
                body: Program::new(
                    vec![Statement::Return(Expr::Ident("value".into()))],
                    vec![1],
                ),
            }],
            vec![1],
        );
        assert!(program_needs_runtime(&function));
    }
}
