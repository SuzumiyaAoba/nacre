use crate::policy::ExecutionPolicy;
use crate::{BindingPattern, Expr, ForBinding, Program, Statement};

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
__nacre_reject_symlink_components() {
  local __nacre_path="$1"
  local __nacre_root="$2"
  if [[ "$__nacre_path" != /* ]]; then
    __nacre_path="$PWD/$__nacre_path"
  fi
  local __nacre_current="/"
  local __nacre_rest="${__nacre_path#/}"
  local __nacre_part
  local __nacre_candidate
  while [[ -n "$__nacre_rest" ]]; do
    if [[ "$__nacre_rest" == */* ]]; then
      __nacre_part="${__nacre_rest%%/*}"
      __nacre_rest="${__nacre_rest#*/}"
    else
      __nacre_part="$__nacre_rest"
      __nacre_rest=""
    fi
    [[ -z "$__nacre_part" || "$__nacre_part" == "." ]] && continue
    if [[ "$__nacre_part" == ".." ]]; then
      __nacre_current="${__nacre_current%/*}"
      [[ -z "$__nacre_current" ]] && __nacre_current="/"
      continue
    fi
    if [[ "$__nacre_current" == "/" ]]; then
      __nacre_candidate="/$__nacre_part"
    else
      __nacre_candidate="$__nacre_current/$__nacre_part"
    fi
    if [[ -L "$__nacre_candidate" ]]; then
      if [[ "$__nacre_candidate" == "$__nacre_root" || "$__nacre_candidate" == "$__nacre_root/"* ]]; then
        return 1
      fi
      if [[ -d "$__nacre_candidate" ]]; then
        __nacre_current="$(cd -P -- "$__nacre_candidate" && pwd -P)" || return 1
        continue
      fi
    fi
    __nacre_current="$__nacre_candidate"
  done
}

__nacre_resolve_guarded_path() {
  local __nacre_path="$1"
  if [[ "$__nacre_path" != /* ]]; then
    __nacre_path="$PWD/$__nacre_path"
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

__nacre_checked_path() {
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
      if ! __nacre_reject_symlink_components "$__nacre_path" "$__nacre_root"; then
        printf 'nacre: denied %s path: %s\n' "$__nacre_access" "$__nacre_path" >&2
        return 126
      fi
      printf '%s\n' "$__nacre_resolved"
      return 0
    fi
  done
  printf 'nacre: denied %s path: %s\n' "$__nacre_access" "$__nacre_path" >&2
  return 126
}

__nacre_assert_path_in_roots() {
  __nacre_checked_path "$1" "$2" >/dev/null
}
"#,
    );
}

pub(super) fn program_needs_runtime(program: &Program) -> bool {
    program.statements().iter().any(statement_needs_runtime)
}

fn statement_needs_runtime(statement: &Statement) -> bool {
    match statement {
        Statement::Export(inner) => statement_needs_runtime(inner),
        Statement::Function { .. }
        | Statement::ExternalFunction { .. }
        | Statement::Impl { .. } => true,
        Statement::SumType { variants, .. } => {
            variants.iter().any(|variant| !variant.fields.is_empty())
        }
        Statement::Const { expr, .. }
        | Statement::Let { expr, .. }
        | Statement::Destructure { expr, .. }
        | Statement::Assign { expr, .. }
        | Statement::Expr(expr)
        | Statement::Return(expr)
        | Statement::TryResult(expr) => expr_needs_runtime(expr),
        Statement::Block { body } => program_needs_runtime(body),
        Statement::While { condition, body } => {
            expr_needs_runtime(condition) || program_needs_runtime(body)
        }
        Statement::For {
            binding,
            iterable,
            body,
        } => {
            for_binding_needs_runtime(binding)
                || expr_needs_runtime(iterable)
                || program_needs_runtime(body)
        }
        Statement::If {
            condition,
            then_branch,
            else_branch,
        } => {
            expr_needs_runtime(condition)
                || program_needs_runtime(then_branch)
                || else_branch.as_ref().is_some_and(program_needs_runtime)
        }
        _ => false,
    }
}

fn for_binding_needs_runtime(binding: &ForBinding) -> bool {
    matches!(
        binding,
        ForBinding::Pattern(
            BindingPattern::Tuple(_) | BindingPattern::Record(_) | BindingPattern::Array { .. }
        )
    )
}

fn expr_needs_runtime(expr: &Expr) -> bool {
    match expr {
        Expr::Array(values) => values.iter().any(|value| {
            matches!(value, Expr::Tuple(_) | Expr::Record(_) | Expr::Array(_))
                || expr_needs_runtime(value)
        }),
        Expr::Tuple(values) => values.iter().any(expr_needs_runtime),
        Expr::Record(fields) => fields.iter().any(|(_, value)| expr_needs_runtime(value)),
        Expr::Some(value)
        | Expr::Ok(value)
        | Expr::Err(value)
        | Expr::Async(value)
        | Expr::ResultOption(value)
        | Expr::TryResult(value)
        | Expr::PathExists(value)
        | Expr::IndexValue { value, .. }
        | Expr::ArraySliceValue { value, .. }
        | Expr::ArrayTakeValue { value, .. }
        | Expr::ArrayDropValue { value, .. }
        | Expr::TupleFieldValue { value, .. }
        | Expr::FieldValue { value, .. }
        | Expr::Cast { expr: value, .. }
        | Expr::NewtypeCtor { value, .. } => expr_needs_runtime(value),
        Expr::Default { value, fallback }
        | Expr::DefaultTry { value, fallback }
        | Expr::Binary {
            left: value,
            right: fallback,
            ..
        }
        | Expr::Range {
            start: value,
            end: fallback,
            ..
        } => expr_needs_runtime(value) || expr_needs_runtime(fallback),
        Expr::Call { args, .. } | Expr::AllowedCommand { args, .. } => {
            args.iter().any(expr_needs_runtime)
        }
        Expr::ArrayMap { mapper, .. }
        | Expr::ArrayFilter {
            predicate: mapper, ..
        }
        | Expr::ArrayFlatMap { mapper, .. }
        | Expr::ArrayFind {
            predicate: mapper, ..
        }
        | Expr::ArrayAny {
            predicate: mapper, ..
        }
        | Expr::ArrayAll {
            predicate: mapper, ..
        }
        | Expr::OptionMap { mapper, .. }
        | Expr::OptionFlatMap { mapper, .. }
        | Expr::ResultMap { mapper, .. }
        | Expr::ResultFlatMap { mapper, .. } => expr_needs_runtime(mapper),
        Expr::ArrayMapValue { value, mapper }
        | Expr::ArrayFlatMapValue { value, mapper }
        | Expr::OptionMapValue { value, mapper }
        | Expr::OptionFlatMapValue { value, mapper }
        | Expr::ResultMapValue { value, mapper }
        | Expr::ResultFlatMapValue { value, mapper } => {
            expr_needs_runtime(value) || expr_needs_runtime(mapper)
        }
        Expr::ArrayFilterValue { value, predicate }
        | Expr::ArrayFindValue { value, predicate }
        | Expr::ArrayAnyValue { value, predicate }
        | Expr::ArrayAllValue { value, predicate } => {
            expr_needs_runtime(value) || expr_needs_runtime(predicate)
        }
        Expr::ArrayFold {
            initial, reducer, ..
        } => expr_needs_runtime(initial) || expr_needs_runtime(reducer),
        Expr::ArrayFoldValue {
            value,
            initial,
            reducer,
        } => {
            expr_needs_runtime(value) || expr_needs_runtime(initial) || expr_needs_runtime(reducer)
        }
        Expr::IfElse {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_needs_runtime(condition)
                || expr_needs_runtime(then_expr)
                || expr_needs_runtime(else_expr)
        }
        Expr::Match { value, arms } => {
            expr_needs_runtime(value)
                || arms.iter().any(|arm| {
                    arm.pattern.as_ref().is_some_and(expr_needs_runtime)
                        || arm.guard.as_ref().is_some_and(expr_needs_runtime)
                        || expr_needs_runtime(&arm.expr)
                })
        }
        Expr::LetIn { value, body, .. } => expr_needs_runtime(value) || expr_needs_runtime(body),
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

__nacre_tuple_pack() {
  printf '__nacre_tuple:%s:' "$#"
  local __nacre_runtime_value
  for __nacre_runtime_value in "$@"; do
    printf '%s:%s' "${#__nacre_runtime_value}" "$__nacre_runtime_value"
  done
}

__nacre_tuple_field() {
  local __nacre_runtime_tuple_data="${1#__nacre_tuple:}"
  local __nacre_runtime_tuple_target="$2"
  local __nacre_runtime_tuple_count="${__nacre_runtime_tuple_data%%:*}"
  __nacre_runtime_tuple_data="${__nacre_runtime_tuple_data#*:}"
  local __nacre_runtime_tuple_length
  local __nacre_runtime_tuple_value
  local __nacre_runtime_tuple_index
  for ((__nacre_runtime_tuple_index = 1; __nacre_runtime_tuple_index <= __nacre_runtime_tuple_count; __nacre_runtime_tuple_index++)); do
    __nacre_runtime_tuple_length="${__nacre_runtime_tuple_data%%:*}"
    __nacre_runtime_tuple_data="${__nacre_runtime_tuple_data#*:}"
    __nacre_runtime_tuple_value="${__nacre_runtime_tuple_data:0:__nacre_runtime_tuple_length}"
    __nacre_runtime_tuple_data="${__nacre_runtime_tuple_data:__nacre_runtime_tuple_length}"
    if [[ "$__nacre_runtime_tuple_index" == "$__nacre_runtime_tuple_target" ]]; then
      printf '%s' "$__nacre_runtime_tuple_value"
      return
    fi
  done
}

__nacre_record_pack() {
  local __nacre_runtime_record_count=$(($# / 2))
  printf '__nacre_record:%s:' "$__nacre_runtime_record_count"
  local __nacre_runtime_record_key
  local __nacre_runtime_record_value
  while (($# > 0)); do
    __nacre_runtime_record_key="$1"
    __nacre_runtime_record_value="$2"
    shift 2
    printf '%s:%s%s:%s' "${#__nacre_runtime_record_key}" "$__nacre_runtime_record_key" "${#__nacre_runtime_record_value}" "$__nacre_runtime_record_value"
  done
}

__nacre_record_field() {
  local __nacre_runtime_record_data="${1#__nacre_record:}"
  local __nacre_runtime_record_target="$2"
  local __nacre_runtime_record_count="${__nacre_runtime_record_data%%:*}"
  __nacre_runtime_record_data="${__nacre_runtime_record_data#*:}"
  local __nacre_runtime_record_length
  local __nacre_runtime_record_key
  local __nacre_runtime_record_value
  local __nacre_runtime_record_index
  for ((__nacre_runtime_record_index = 0; __nacre_runtime_record_index < __nacre_runtime_record_count; __nacre_runtime_record_index++)); do
    __nacre_runtime_record_length="${__nacre_runtime_record_data%%:*}"
    __nacre_runtime_record_data="${__nacre_runtime_record_data#*:}"
    __nacre_runtime_record_key="${__nacre_runtime_record_data:0:__nacre_runtime_record_length}"
    __nacre_runtime_record_data="${__nacre_runtime_record_data:__nacre_runtime_record_length}"
    __nacre_runtime_record_length="${__nacre_runtime_record_data%%:*}"
    __nacre_runtime_record_data="${__nacre_runtime_record_data#*:}"
    __nacre_runtime_record_value="${__nacre_runtime_record_data:0:__nacre_runtime_record_length}"
    __nacre_runtime_record_data="${__nacre_runtime_record_data:__nacre_runtime_record_length}"
    if [[ "$__nacre_runtime_record_key" == "$__nacre_runtime_record_target" ]]; then
      printf '%s' "$__nacre_runtime_record_value"
      return
    fi
  done
}

__nacre_array_pack() {
  printf '__nacre_array:%s:' "$#"
  local __nacre_runtime_value
  for __nacre_runtime_value in "$@"; do
    printf '%s:%s' "${#__nacre_runtime_value}" "$__nacre_runtime_value"
  done
}

__nacre_array_field() {
  local __nacre_runtime_array_data="${1#__nacre_array:}"
  local __nacre_runtime_array_target="$2"
  local __nacre_runtime_array_count="${__nacre_runtime_array_data%%:*}"
  __nacre_runtime_array_data="${__nacre_runtime_array_data#*:}"
  local __nacre_runtime_array_length
  local __nacre_runtime_array_value
  local __nacre_runtime_array_index
  for ((__nacre_runtime_array_index = 0; __nacre_runtime_array_index < __nacre_runtime_array_count; __nacre_runtime_array_index++)); do
    __nacre_runtime_array_length="${__nacre_runtime_array_data%%:*}"
    __nacre_runtime_array_data="${__nacre_runtime_array_data#*:}"
    __nacre_runtime_array_value="${__nacre_runtime_array_data:0:__nacre_runtime_array_length}"
    __nacre_runtime_array_data="${__nacre_runtime_array_data:__nacre_runtime_array_length}"
    if [[ "$__nacre_runtime_array_index" == "$__nacre_runtime_array_target" ]]; then
      printf '%s' "$__nacre_runtime_array_value"
      return
    fi
  done
}

__nacre_array_rest_decl() {
  local __nacre_runtime_array_data="${1#__nacre_array:}"
  local __nacre_runtime_array_target="$2"
  local __nacre_runtime_array_start="$3"
  local __nacre_runtime_array_count="${__nacre_runtime_array_data%%:*}"
  __nacre_runtime_array_data="${__nacre_runtime_array_data#*:}"
  printf 'declare -a %s=(' "$__nacre_runtime_array_target"
  local __nacre_runtime_array_length
  local __nacre_runtime_array_value
  local __nacre_runtime_array_index
  for ((__nacre_runtime_array_index = 0; __nacre_runtime_array_index < __nacre_runtime_array_count; __nacre_runtime_array_index++)); do
    __nacre_runtime_array_length="${__nacre_runtime_array_data%%:*}"
    __nacre_runtime_array_data="${__nacre_runtime_array_data#*:}"
    __nacre_runtime_array_value="${__nacre_runtime_array_data:0:__nacre_runtime_array_length}"
    __nacre_runtime_array_data="${__nacre_runtime_array_data:__nacre_runtime_array_length}"
    if (( __nacre_runtime_array_index >= __nacre_runtime_array_start )); then
      printf ' %q' "$__nacre_runtime_array_value"
    fi
  done
  printf ')'
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
