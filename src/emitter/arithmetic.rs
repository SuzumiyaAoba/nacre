use super::expressions::{emit_call_arg, emit_call_head, emit_expr};
use super::quoting::{emit_awk_string, emit_shell_word};
use super::values::{emit_closure, emit_index_expr};
use crate::Expr;

pub(super) fn emit_awk_numeric(out: &mut String, expr: &Expr) {
    emit_awk(out, expr, AwkMode::Numeric);
}

pub(super) fn emit_awk_bool(out: &mut String, expr: &Expr) {
    emit_awk(out, expr, AwkMode::BoolValue);
}

pub(super) fn emit_awk_condition(out: &mut String, expr: &Expr) {
    emit_awk(out, expr, AwkMode::Condition);
}

#[derive(Debug, Copy, Clone)]
enum AwkMode {
    Numeric,
    BoolValue,
    Condition,
}

fn emit_awk(out: &mut String, expr: &Expr, mode: AwkMode) {
    let mut vars = Vec::new();
    let mut awk_expr = String::new();
    emit_awk_expr(&mut awk_expr, expr, &mut vars);

    out.push_str("awk");
    for (name, value) in vars {
        out.push_str(" -v ");
        out.push_str(&name);
        out.push('=');
        out.push_str(&value);
    }
    out.push(' ');

    let program = match mode {
        AwkMode::Numeric => format!("BEGIN {{ print ({awk_expr}) }}"),
        AwkMode::BoolValue => format!("BEGIN {{ print (({awk_expr}) ? \"true\" : \"false\") }}"),
        AwkMode::Condition => format!("BEGIN {{ exit (({awk_expr}) ? 0 : 1) }}"),
    };
    emit_shell_word(out, &program);
}

pub(super) fn emit_awk_expr(out: &mut String, expr: &Expr, vars: &mut Vec<(String, String)>) {
    match expr {
        Expr::MatchGuardResult(value) => emit_awk_expr(out, value, vars),
        Expr::Int(value) => out.push_str(&value.to_string()),
        Expr::Float(value) => out.push_str(value),
        Expr::Bool(true) => emit_awk_string(out, "true"),
        Expr::Bool(false) => emit_awk_string(out, "false"),
        Expr::Unit => emit_awk_string(out, ""),
        Expr::String(value) | Expr::RawString(value) => emit_awk_string(out, value),
        Expr::ProcessEnv { .. }
        | Expr::FsIsFile { .. }
        | Expr::FsIsDir { .. }
        | Expr::FsSize { .. }
        | Expr::FsReadLines { .. }
        | Expr::FsList { .. }
        | Expr::FsWriteLines { .. }
        | Expr::FsAppendLines { .. } => {
            let name = format!("__nacre_{}", vars.len());
            let mut value = String::new();
            emit_expr(&mut value, expr);
            vars.push((name.clone(), value));
            out.push_str(&name);
        }
        Expr::Not(expr) => {
            out.push_str("!(");
            emit_awk_bool_operand(out, expr, vars);
            out.push(')');
        }
        Expr::BitNot(_) => {
            let name = format!("__nacre_{}", vars.len());
            let mut value = String::new();
            emit_expr(&mut value, expr);
            vars.push((name.clone(), value));
            out.push_str(&name);
        }
        Expr::Binary { op, .. } if *op == crate::BinaryOp::Concat => {
            let name = format!("__nacre_{}", vars.len());
            let mut value = String::new();
            emit_expr(&mut value, expr);
            vars.push((name.clone(), value));
            out.push_str(&name);
        }
        Expr::Binary { op, .. } if op.is_bitwise() => {
            let name = format!("__nacre_{}", vars.len());
            let mut value = String::new();
            emit_expr(&mut value, expr);
            vars.push((name.clone(), value));
            out.push_str(&name);
        }
        Expr::Binary { left, op, right } if op.is_logical() => {
            out.push('(');
            emit_awk_bool_operand(out, left, vars);
            out.push(' ');
            out.push_str(op.bash());
            out.push(' ');
            emit_awk_bool_operand(out, right, vars);
            out.push(')');
        }
        Expr::Binary { left, op, right } => {
            out.push('(');
            emit_awk_expr(out, left, vars);
            out.push(' ');
            out.push_str(op.bash());
            out.push(' ');
            emit_awk_expr(out, right, vars);
            out.push(')');
        }
        Expr::NewtypeCtor { value, .. } => emit_awk_expr(out, value, vars),
        Expr::Variant { .. } => {
            let name = format!("__nacre_{}", vars.len());
            let mut value = String::new();
            emit_expr(&mut value, expr);
            vars.push((name.clone(), value));
            out.push_str(&name);
        }
        Expr::Cast { expr, .. } => emit_awk_expr(out, expr, vars),
        Expr::Call { .. } => {
            let name = format!("__nacre_{}", vars.len());
            let mut value = String::new();
            emit_expr(&mut value, expr);
            vars.push((name.clone(), value));
            out.push_str(&name);
        }
        Expr::Ident(_)
        | Expr::Some(_)
        | Expr::Ok(_)
        | Expr::Err(_)
        | Expr::ResultOption(_)
        | Expr::TryResult(_)
        | Expr::None
        | Expr::Default { .. }
        | Expr::ProcessArgs
        | Expr::CliParse
        | Expr::JsonParse { .. }
        | Expr::JsonStringify { .. }
        | Expr::JsonStringifyValue { .. }
        | Expr::AllowedCommand { .. }
        | Expr::Command { .. }
        | Expr::CommandResult { .. }
        | Expr::AsyncCommand(_)
        | Expr::Async(_)
        | Expr::Await(_)
        | Expr::Pipeline { .. }
        | Expr::TryPipeline { .. }
        | Expr::PipelineResult { .. }
        | Expr::IfElse { .. }
        | Expr::Match { .. }
        | Expr::HasCommand(_)
        | Expr::PathExists(_)
        | Expr::Array(_)
        | Expr::Range { .. }
        | Expr::Map(_)
        | Expr::Record(_)
        | Expr::RecordPattern(_)
        | Expr::Tuple(_)
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
        | Expr::ArrayFilter { .. }
        | Expr::ArrayFilterValue { .. }
        | Expr::ArrayFlatMap { .. }
        | Expr::ArrayFlatMapValue { .. }
        | Expr::ArrayFind { .. }
        | Expr::ArrayFindValue { .. }
        | Expr::ArrayAny { .. }
        | Expr::ArrayAnyValue { .. }
        | Expr::ArrayAll { .. }
        | Expr::ArrayAllValue { .. }
        | Expr::ArrayFold { .. }
        | Expr::ArrayFoldValue { .. }
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
        | Expr::OptionOrElseTry { .. }
        | Expr::DefaultTry { .. }
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
        | Expr::EnvDefault { .. } => {
            let name = format!("__nacre_{}", vars.len());
            let mut value = String::new();
            emit_expr(&mut value, expr);
            vars.push((name.clone(), value));
            out.push_str(&name);
        }
        Expr::LetIn { .. } => {
            let name = format!("__nacre_{}", vars.len());
            let mut value = String::new();
            emit_expr(&mut value, expr);
            vars.push((name.clone(), value));
            out.push_str(&name);
        }
        Expr::Do { .. } => unreachable!("do expressions are lowered before emission"),
        Expr::NamedArg { .. } => unreachable!("named arguments are lowered before emission"),
        Expr::Closure { name, captures } => emit_closure(out, name, captures),
        Expr::ArrayPattern { .. } | Expr::AliasPattern { .. } => {
            unreachable!("match patterns are only emitted by the match emitter")
        }
        Expr::Lambda { .. } => unreachable!("lambdas are lowered before emission"),
    }
}

fn emit_awk_bool_operand(out: &mut String, expr: &Expr, vars: &mut Vec<(String, String)>) {
    match expr {
        Expr::Bool(true) => out.push('1'),
        Expr::Bool(false) => out.push('0'),
        Expr::Not(_) => emit_awk_expr(out, expr, vars),
        Expr::Binary { op, .. } if !op.is_arithmetic() => emit_awk_expr(out, expr, vars),
        _ => {
            let name = format!("__nacre_{}", vars.len());
            let mut value = String::new();
            emit_expr(&mut value, expr);
            vars.push((name.clone(), value));
            out.push('(');
            out.push_str(&name);
            out.push_str(" == \"true\")");
        }
    }
}

pub(super) fn emit_bash_arithmetic(out: &mut String, expr: &Expr) {
    out.push_str("$((");
    emit_bash_arith_expr(out, expr);
    out.push_str("))");
}

pub(super) fn emit_bash_arith_expr(out: &mut String, expr: &Expr) {
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

pub(super) fn emit_numeric_call(out: &mut String, name: &str, args: &[Expr]) -> bool {
    if !is_numeric_builtin(name) {
        return false;
    }
    out.push_str("\"$(");
    emit_numeric_call_command(out, name, args);
    out.push_str(")\"");
    true
}

pub(super) fn emit_numeric_call_command(out: &mut String, name: &str, args: &[Expr]) -> bool {
    if !is_numeric_builtin(name) {
        return false;
    }
    match name {
        "Int.parse" | "parseInt" => emit_numeric_parse_command(out, &args[0], false, true),
        "Int.tryParse" | "tryParseInt" => emit_numeric_parse_command(out, &args[0], false, false),
        "Float.parse" | "parseFloat" => emit_numeric_parse_command(out, &args[0], true, true),
        "Float.tryParse" | "tryParseFloat" => {
            emit_numeric_parse_command(out, &args[0], true, false)
        }
        "numeric_abs" => {
            out.push_str("awk -v v=");
            emit_call_arg(out, &args[0]);
            out.push_str(" 'BEGIN { if (v < 0) v = -v; print v }'");
        }
        "numeric_floor" => emit_numeric_unary_awk(out, &args[0], "int(v)"),
        "numeric_ceil" => {
            emit_numeric_unary_awk(out, &args[0], "int(v) == v ? v : int(v) + (v > 0 ? 1 : 0)")
        }
        "numeric_round" => {
            emit_numeric_unary_awk(out, &args[0], "v < 0 ? int(v - 0.5) : int(v + 0.5)")
        }
        "numeric_min" => emit_numeric_binary_awk(out, &args[0], &args[1], "a < b ? a : b"),
        "numeric_max" => emit_numeric_binary_awk(out, &args[0], &args[1], "a > b ? a : b"),
        "numeric_clamp" => {
            out.push_str("awk -v v=");
            emit_call_arg(out, &args[0]);
            out.push_str(" -v lo=");
            emit_call_arg(out, &args[1]);
            out.push_str(" -v hi=");
            emit_call_arg(out, &args[2]);
            out.push_str(" 'BEGIN { print (v < lo ? lo : (v > hi ? hi : v)) }'");
        }
        _ => return false,
    }
    true
}

pub(super) fn is_numeric_builtin(name: &str) -> bool {
    matches!(
        name,
        "Int.parse"
            | "Int.tryParse"
            | "Float.parse"
            | "Float.tryParse"
            | "parseInt"
            | "tryParseInt"
            | "parseFloat"
            | "tryParseFloat"
            | "numeric_abs"
            | "numeric_floor"
            | "numeric_ceil"
            | "numeric_round"
            | "numeric_min"
            | "numeric_max"
            | "numeric_clamp"
    )
}

pub(super) fn emit_numeric_parse_command(
    out: &mut String,
    value: &Expr,
    float: bool,
    checked: bool,
) {
    out.push_str("awk -v v=");
    emit_call_arg(out, value);
    let pattern = if checked && float {
        r#"'BEGIN { if (v ~ /^[-+]?([0-9]+([.][0-9]*)?|[.][0-9]+)$/) { print v } else { exit 2 } }'"#
    } else if checked {
        r#"'BEGIN { if (v ~ /^[-+]?[0-9]+$/) { print int(v) } else { exit 2 } }'"#
    } else if float {
        r#"'BEGIN { if (v ~ /^[-+]?([0-9]+([.][0-9]*)?|[.][0-9]+)$/) { printf "1%s\n", v } else { exit 2 } }'"#
    } else {
        r#"'BEGIN { if (v ~ /^[-+]?[0-9]+$/) { printf "1%s\n", int(v) } else { exit 2 } }'"#
    };
    out.push(' ');
    out.push_str(pattern);
    if checked {
        out.push_str(" || exit $?");
    } else {
        out.push_str(" && exit 0 || { status=$?; if [ \"$status\" -eq 2 ]; then printf 0; else exit \"$status\"; fi; }");
    }
}

pub(super) fn emit_numeric_unary_awk(out: &mut String, value: &Expr, body: &str) {
    out.push_str("awk -v v=");
    emit_call_arg(out, value);
    out.push_str(" 'BEGIN { print ");
    out.push('(');
    out.push_str(body);
    out.push(')');
    out.push_str(" }'");
}

pub(super) fn emit_numeric_binary_awk(out: &mut String, left: &Expr, right: &Expr, body: &str) {
    out.push_str("awk -v a=");
    emit_call_arg(out, left);
    out.push_str(" -v b=");
    emit_call_arg(out, right);
    out.push_str(" 'BEGIN { print ");
    out.push('(');
    out.push_str(body);
    out.push(')');
    out.push_str(" }'");
}
