use crate::Expr;

use super::quoting::{emit_awk_string, emit_shell_word};
use super::{emit_closure, emit_expr};

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
