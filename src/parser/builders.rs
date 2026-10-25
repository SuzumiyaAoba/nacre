use super::{parse_expr, BindingTarget, CollectionEntry, DoItem, FunctionHead, Postfix};
use crate::{
    AssignTarget, BinaryOp, DoStep, Expr, ImplMethod, Program, Statement, TraitMethod, Type,
};

pub(super) fn named_type(name: String) -> Type {
    match name.as_str() {
        "Int" => Type::Int,
        "Float" => Type::Float,
        "Bool" => Type::Bool,
        "String" => Type::String,
        "Path" => Type::Path,
        "ExitCode" => Type::ExitCode,
        "Unit" => Type::Unit,
        _ => Type::Named(name),
    }
}

pub(super) fn applied_type(name: String, args: Vec<Type>) -> Type {
    match (name.as_str(), args.as_slice()) {
        ("Future", [value]) => Type::Future(Box::new(value.clone())),
        ("Map", [key, value]) => Type::Map(Box::new(key.clone()), Box::new(value.clone())),
        _ => Type::Applied(name, args),
    }
}

pub(super) fn flatten_union(left: Type, right: Type) -> Type {
    let mut types = match left {
        Type::Union(types) => types,
        other => vec![other],
    };
    match right {
        Type::Union(right) => types.extend(right),
        other => types.push(other),
    }
    Type::Union(types)
}

pub(super) fn flatten_intersection(left: Type, right: Type) -> Type {
    let mut types = match left {
        Type::Intersection(types) => types,
        other => vec![other],
    };
    match right {
        Type::Intersection(right) => types.extend(right),
        other => types.push(other),
    }
    Type::Intersection(types)
}

pub(super) fn decode_escaped(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            result.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => result.push('\n'),
            Some('r') => result.push('\r'),
            Some('t') => result.push('\t'),
            Some(other) => result.push(other),
            None => result.push('\\'),
        }
    }
    result
}

pub(super) fn string_expr(value: String) -> Result<Expr, String> {
    if !value.contains("${") {
        return Ok(Expr::String(value));
    }

    let mut parts = Vec::new();
    let mut rest = value.as_str();
    while let Some(start) = rest.find("${") {
        if start > 0 {
            parts.push(Expr::String(rest[..start].to_string()));
        }
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find('}') else {
            return Ok(Expr::String(value));
        };
        let source = &after_start[..end];
        let expr =
            parse_expr(source, 1).map_err(|_| "string interpolation expression".to_string())?;
        parts.push(Expr::Cast {
            expr: Box::new(expr),
            ty: Type::String,
        });
        rest = &after_start[end + 1..];
    }
    if !rest.is_empty() {
        parts.push(Expr::String(rest.to_string()));
    }

    let mut parts = parts.into_iter();
    let Some(first) = parts.next() else {
        return Ok(Expr::String(String::new()));
    };
    Ok(parts.fold(first, |left, right| binary(left, BinaryOp::Concat, right)))
}

pub(super) fn binary(left: Expr, op: BinaryOp, right: Expr) -> Expr {
    Expr::Binary {
        left: Box::new(left),
        op,
        right: Box::new(right),
    }
}

pub(super) fn compound_assignment(name: String, op: BinaryOp, right: Expr) -> Statement {
    Statement::Assign {
        target: AssignTarget::Name(name.clone()),
        expr: binary(Expr::Ident(name), op, right),
    }
}

pub(super) fn default_expr(left: Expr, right: Expr) -> Expr {
    Expr::Default {
        value: Box::new(left),
        fallback: Box::new(right),
    }
}

pub(super) fn ident_name(value: &Expr) -> Option<&str> {
    if let Expr::Ident(name) = value {
        Some(name)
    } else {
        None
    }
}

pub(super) fn call_expr(name: String, mut args: Vec<Expr>) -> Expr {
    if let Some((group, command, result)) = allowed_command_name(&name) {
        return Expr::AllowedCommand {
            group,
            command,
            args,
            result,
            program: None,
            read_args: Vec::new(),
            write_args: Vec::new(),
        };
    }
    match (name.as_str(), args.len()) {
        ("Some", 1) => Expr::Some(Box::new(args.remove(0))),
        ("Ok", 1) => Expr::Ok(Box::new(args.remove(0))),
        ("Err", 1) => Expr::Err(Box::new(args.remove(0))),
        ("hasCommand", 1) => match args.remove(0) {
            Expr::String(command) => Expr::HasCommand(command),
            value => Expr::Call {
                name,
                args: vec![value],
            },
        },
        ("pathExists", 1) => Expr::PathExists(Box::new(args.remove(0))),
        ("process.args", 0) => Expr::ProcessArgs,
        ("process.env", 1) => Expr::ProcessEnv {
            name: Box::new(args.remove(0)),
        },
        ("fs.isFile", 1) => Expr::FsIsFile {
            path: Box::new(args.remove(0)),
        },
        ("fs.isDir", 1) => Expr::FsIsDir {
            path: Box::new(args.remove(0)),
        },
        ("fs.size", 1) => Expr::FsSize {
            path: Box::new(args.remove(0)),
        },
        ("fs.readLines", 1) => Expr::FsReadLines {
            path: Box::new(args.remove(0)),
        },
        ("fs.list", 1) => Expr::FsList {
            path: Box::new(args.remove(0)),
        },
        ("fs.writeLines", 2) => Expr::FsWriteLines {
            path: Box::new(args.remove(0)),
            lines: Box::new(args.remove(0)),
        },
        ("fs.appendLines", 2) => Expr::FsAppendLines {
            path: Box::new(args.remove(0)),
            lines: Box::new(args.remove(0)),
        },
        ("cli.parse", 0) => Expr::CliParse,
        ("json.parse", 1) => Expr::JsonParse {
            value: Box::new(args.remove(0)),
        },
        ("json.stringify", 1) => match args.remove(0) {
            Expr::Ident(name) => Expr::JsonStringify { name },
            value => Expr::JsonStringifyValue {
                value: Box::new(value),
            },
        },
        _ if name
            .rsplit('.')
            .next()
            .is_some_and(|part| part.starts_with(|ch: char| ch.is_ascii_uppercase()))
            && args.len() == 1 =>
        {
            Expr::NewtypeCtor {
                name,
                value: Box::new(args.remove(0)),
            }
        }
        _ => Expr::Call { name, args },
    }
}

pub(super) fn allowed_command_name(name: &str) -> Option<(String, String, bool)> {
    let mut parts = name.split('.');
    if parts.next()? != "run" {
        return None;
    }
    let first = parts.next()?;
    let (group, result) = if first == "result" {
        (parts.next()?, true)
    } else {
        (first, false)
    };
    let command = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    Some((group.to_string(), command.to_string(), result))
}

pub(super) fn named_or_value<T>(
    receiver: Expr,
    named: impl FnOnce(String) -> T,
    value: impl FnOnce(Expr) -> T,
) -> T {
    match receiver {
        Expr::Ident(name) => named(name),
        other => value(other),
    }
}

pub(super) fn invalid_method_arity(
    receiver: Expr,
    method: &str,
    args: Vec<Expr>,
    message: &'static str,
) -> Result<Expr, &'static str> {
    if let Some(name) = receiver.qualified_name() {
        return Ok(call_expr(format!("{name}.{method}"), args));
    }
    Err(message)
}

pub(super) fn method_expr(
    receiver: Expr,
    method: String,
    mut args: Vec<Expr>,
) -> Result<Expr, &'static str> {
    let arity = args.len();
    let result = match (method.as_str(), arity) {
        ("wait", 0) => named_or_value(receiver, Expr::Await, |value| Expr::Call {
            name: "wait".into(),
            args: vec![value],
        }),
        ("value", 0) => named_or_value(receiver, Expr::Value, |value| Expr::FieldValue {
            value: Box::new(value),
            field: "value".into(),
        }),
        ("len", 0) => named_or_value(receiver, Expr::Len, |value| match value {
            Expr::Array(_) => Expr::ArrayLenValue(Box::new(value)),
            Expr::Map(_) => Expr::MapLenValue(Box::new(value)),
            _ => Expr::StringLenValue(Box::new(value)),
        }),
        ("isEmpty", 0) => named_or_value(receiver, Expr::IsEmpty, |value| match value {
            Expr::Array(_) => Expr::ArrayIsEmptyValue(Box::new(value)),
            Expr::Map(_) => Expr::MapIsEmptyValue(Box::new(value)),
            _ => Expr::StringIsEmptyValue(Box::new(value)),
        }),
        ("first", 0) => named_or_value(receiver, Expr::ArrayFirst, |value| {
            Expr::ArrayFirstValue(Box::new(value))
        }),
        ("last", 0) => named_or_value(receiver, Expr::ArrayLast, |value| {
            Expr::ArrayLastValue(Box::new(value))
        }),
        ("reverse", 0) => named_or_value(receiver, Expr::ArrayReverse, |value| {
            Expr::ArrayReverseValue(Box::new(value))
        }),
        ("sort", 0) => named_or_value(receiver, Expr::ArraySort, |value| {
            Expr::ArraySortValue(Box::new(value))
        }),
        ("unique", 0) => named_or_value(receiver, Expr::ArrayUnique, |value| {
            Expr::ArrayUniqueValue(Box::new(value))
        }),
        ("keys", 0) => named_or_value(receiver, Expr::MapKeys, |value| {
            Expr::MapKeysValue(Box::new(value))
        }),
        ("values", 0) => named_or_value(receiver, Expr::MapValues, |value| {
            Expr::MapValuesValue(Box::new(value))
        }),
        ("pop", 0) => {
            let Some(name) = ident_name(&receiver) else {
                return Err("pop requires a named array");
            };
            Expr::ArrayPop {
                name: name.to_string(),
            }
        }
        ("trim", 0) => named_or_value(receiver, Expr::StringTrim, |value| {
            Expr::StringTrimValue(Box::new(value))
        }),
        ("trimStart", 0) => named_or_value(receiver, Expr::StringTrimStart, |value| {
            Expr::StringTrimStartValue(Box::new(value))
        }),
        ("trimEnd", 0) => named_or_value(receiver, Expr::StringTrimEnd, |value| {
            Expr::StringTrimEndValue(Box::new(value))
        }),
        ("toUpper", 0) => named_or_value(receiver, Expr::StringToUpper, |value| {
            Expr::StringToUpperValue(Box::new(value))
        }),
        ("toLower", 0) => named_or_value(receiver, Expr::StringToLower, |value| {
            Expr::StringToLowerValue(Box::new(value))
        }),
        ("isAbsolute", 0) => named_or_value(receiver, Expr::PathIsAbsolute, |value| {
            Expr::PathIsAbsoluteValue(Box::new(value))
        }),
        ("basename", 0) => named_or_value(receiver, Expr::PathBasename, |value| {
            Expr::PathBasenameValue(Box::new(value))
        }),
        ("dirname", 0) => named_or_value(receiver, Expr::PathDirname, |value| {
            Expr::PathDirnameValue(Box::new(value))
        }),
        ("stem", 0) => named_or_value(receiver, Expr::PathStem, |value| {
            Expr::PathStemValue(Box::new(value))
        }),
        ("extname", 0) => named_or_value(receiver, Expr::PathExtname, |value| {
            Expr::PathExtnameValue(Box::new(value))
        }),
        ("map", 1) => {
            let mapper = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::ArrayMap {
                    name,
                    mapper: mapper.clone(),
                },
                |value| Expr::ArrayMapValue {
                    value: Box::new(value),
                    mapper: mapper.clone(),
                },
            )
        }
        ("flatMap", 1) => {
            let mapper = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::ArrayFlatMap {
                    name,
                    mapper: mapper.clone(),
                },
                |value| Expr::ArrayFlatMapValue {
                    value: Box::new(value),
                    mapper: mapper.clone(),
                },
            )
        }
        ("filter", 1) => {
            let predicate = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::ArrayFilter {
                    name,
                    predicate: predicate.clone(),
                },
                |value| Expr::ArrayFilterValue {
                    value: Box::new(value),
                    predicate: predicate.clone(),
                },
            )
        }
        ("find", 1) => {
            let predicate = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::ArrayFind {
                    name,
                    predicate: predicate.clone(),
                },
                |value| Expr::ArrayFindValue {
                    value: Box::new(value),
                    predicate: predicate.clone(),
                },
            )
        }
        ("any", 1) => {
            let predicate = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::ArrayAny {
                    name,
                    predicate: predicate.clone(),
                },
                |value| Expr::ArrayAnyValue {
                    value: Box::new(value),
                    predicate: predicate.clone(),
                },
            )
        }
        ("all", 1) => {
            let predicate = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::ArrayAll {
                    name,
                    predicate: predicate.clone(),
                },
                |value| Expr::ArrayAllValue {
                    value: Box::new(value),
                    predicate: predicate.clone(),
                },
            )
        }
        ("fold", 2) | ("reduce", 2) => {
            let initial = Box::new(args.remove(0));
            let reducer = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::ArrayFold {
                    name,
                    initial: initial.clone(),
                    reducer: reducer.clone(),
                },
                |value| Expr::ArrayFoldValue {
                    value: Box::new(value),
                    initial: initial.clone(),
                    reducer: reducer.clone(),
                },
            )
        }
        ("ap", 1) => {
            let value = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::OptionAp {
                    name,
                    value: value.clone(),
                },
                |function| Expr::OptionApValue {
                    function: Box::new(function),
                    value: value.clone(),
                },
            )
        }
        ("orElse", 1) => {
            let fallback = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::OptionOrElse {
                    name,
                    fallback: fallback.clone(),
                },
                |value| Expr::OptionOrElseValue {
                    value: Box::new(value),
                    fallback: fallback.clone(),
                },
            )
        }
        ("join", 1) => {
            let separator = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::Join {
                    name,
                    separator: separator.clone(),
                },
                |value| Expr::JoinValue {
                    value: Box::new(value),
                    separator: separator.clone(),
                },
            )
        }
        ("take", 1) => {
            let count = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::ArrayTake {
                    name,
                    count: count.clone(),
                },
                |value| Expr::ArrayTakeValue {
                    value: Box::new(value),
                    count: count.clone(),
                },
            )
        }
        ("drop", 1) => {
            let count = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::ArrayDrop {
                    name,
                    count: count.clone(),
                },
                |value| Expr::ArrayDropValue {
                    value: Box::new(value),
                    count: count.clone(),
                },
            )
        }
        ("push", 1) => {
            let Some(name) = ident_name(&receiver) else {
                return Err("push requires a named array");
            };
            Expr::ArrayPush {
                name: name.to_string(),
                value: Box::new(args.remove(0)),
            }
        }
        ("contains", 1) => {
            let item = args.remove(0);
            named_or_value(
                receiver,
                |name| Expr::StringContains {
                    name,
                    needle: Box::new(item.clone()),
                },
                |value| match value {
                    Expr::Array(_) => Expr::ArrayContainsValue {
                        value: Box::new(value),
                        item: Box::new(item.clone()),
                    },
                    _ => Expr::StringContainsValue {
                        value: Box::new(value),
                        needle: Box::new(item.clone()),
                    },
                },
            )
        }
        ("indexOf", 1) => {
            let item = args.remove(0);
            named_or_value(
                receiver,
                |name| Expr::ArrayIndexOf {
                    name,
                    value: Box::new(item.clone()),
                },
                |value| match value {
                    Expr::Array(_) => Expr::ArrayIndexOfValue {
                        value: Box::new(value),
                        item: Box::new(item.clone()),
                    },
                    _ => Expr::StringIndexOfValue {
                        value: Box::new(value),
                        needle: Box::new(item.clone()),
                    },
                },
            )
        }
        ("startsWith", 1) => {
            let prefix = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::StringStartsWith {
                    name,
                    prefix: prefix.clone(),
                },
                |value| Expr::StringStartsWithValue {
                    value: Box::new(value),
                    prefix: prefix.clone(),
                },
            )
        }
        ("endsWith", 1) => {
            let suffix = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::StringEndsWith {
                    name,
                    suffix: suffix.clone(),
                },
                |value| Expr::StringEndsWithValue {
                    value: Box::new(value),
                    suffix: suffix.clone(),
                },
            )
        }
        ("repeat", 1) => {
            let count = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::StringRepeat {
                    name,
                    count: count.clone(),
                },
                |value| Expr::StringRepeatValue {
                    value: Box::new(value),
                    count: count.clone(),
                },
            )
        }
        ("split", 1) => {
            let separator = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::StringSplit {
                    name,
                    separator: separator.clone(),
                },
                |value| Expr::StringSplitValue {
                    value: Box::new(value),
                    separator: separator.clone(),
                },
            )
        }
        ("has", 1) => {
            let key = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::MapHas {
                    name,
                    key: key.clone(),
                },
                |value| Expr::MapHasValue {
                    value: Box::new(value),
                    key: key.clone(),
                },
            )
        }
        ("remove", 1) => {
            let Some(name) = ident_name(&receiver) else {
                return Err("remove requires a named map");
            };
            Expr::MapRemove {
                name: name.to_string(),
                key: Box::new(args.remove(0)),
            }
        }
        ("slice", 2) => {
            let start = Box::new(args.remove(0));
            let end = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::Slice {
                    name,
                    start: start.clone(),
                    end: end.clone(),
                },
                |value| match value {
                    Expr::Array(_) => Expr::ArraySliceValue {
                        value: Box::new(value),
                        start: start.clone(),
                        end: end.clone(),
                    },
                    _ => Expr::StringSliceValue {
                        value: Box::new(value),
                        start: start.clone(),
                        end: end.clone(),
                    },
                },
            )
        }
        ("set", 2) => {
            let Some(name) = ident_name(&receiver) else {
                return Err("set requires a named map");
            };
            Expr::MapSet {
                name: name.to_string(),
                key: Box::new(args.remove(0)),
                value: Box::new(args.remove(0)),
            }
        }
        ("replace", 2) => {
            let from = Box::new(args.remove(0));
            let to = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::StringReplace {
                    name,
                    from: from.clone(),
                    to: to.clone(),
                },
                |value| Expr::StringReplaceValue {
                    value: Box::new(value),
                    from: from.clone(),
                    to: to.clone(),
                },
            )
        }
        ("abs" | "floor" | "ceil" | "round", 0) => Expr::Call {
            name: format!("numeric_{method}"),
            args: vec![receiver],
        },
        ("min" | "max", 1) => Expr::Call {
            name: format!("numeric_{method}"),
            args: vec![receiver, args.remove(0)],
        },
        ("clamp", 2) => {
            let min = args.remove(0);
            let max = args.remove(0);
            Expr::Call {
                name: "numeric_clamp".into(),
                args: vec![receiver, min, max],
            }
        }
        ("len" | "isEmpty" | "trim" | "trimStart" | "trimEnd" | "toUpper" | "toLower", _) => {
            let message = match method.as_str() {
                "len" => "len expects no arguments",
                "isEmpty" => "isEmpty expects no arguments",
                "trim" => "trim expects no arguments",
                "trimStart" => "trimStart expects no arguments",
                "trimEnd" => "trimEnd expects no arguments",
                "toUpper" => "toUpper expects no arguments",
                _ => "toLower expects no arguments",
            };
            return invalid_method_arity(receiver, &method, args, message);
        }
        ("basename", _) => {
            return invalid_method_arity(receiver, &method, args, "basename expects no arguments");
        }
        ("contains", _) => {
            return invalid_method_arity(
                receiver,
                &method,
                args,
                "contains expects one needle argument",
            );
        }
        ("indexOf", _) => {
            return invalid_method_arity(
                receiver,
                &method,
                args,
                "indexOf expects one needle argument",
            );
        }
        ("startsWith", _) => {
            return invalid_method_arity(
                receiver,
                &method,
                args,
                "startsWith expects one prefix argument",
            );
        }
        ("endsWith", _) => {
            return invalid_method_arity(
                receiver,
                &method,
                args,
                "endsWith expects one suffix argument",
            );
        }
        ("repeat", _) => {
            return invalid_method_arity(
                receiver,
                &method,
                args,
                "repeat expects one count argument",
            );
        }
        ("slice", _) => {
            return invalid_method_arity(
                receiver,
                &method,
                args,
                "slice expects start and end arguments",
            );
        }
        ("replace", _) => {
            return invalid_method_arity(
                receiver,
                &method,
                args,
                "replace expects search and replacement arguments",
            );
        }
        _ => {
            if let Some(name) = receiver.qualified_name() {
                return Ok(call_expr(format!("{name}.{method}"), args));
            }
            let mut source_args = Vec::with_capacity(args.len() + 1);
            source_args.push(receiver);
            source_args.extend(args);
            return Ok(call_expr(method, source_args));
        }
    };
    Ok(result)
}

pub(super) fn apply_postfix(mut value: Expr, suffixes: Vec<Postfix>) -> Result<Expr, &'static str> {
    for suffix in suffixes {
        value = match suffix {
            Postfix::Call(args) => {
                let Some(name) = value.qualified_name() else {
                    return Err("only named functions can be called");
                };
                call_expr(name, args)
            }
            Postfix::Method(name, args) => method_expr(value, name, args)?,
            Postfix::Index(index) => match value {
                Expr::Ident(name) => Expr::Index {
                    name,
                    index: Box::new(index),
                },
                value => Expr::IndexValue {
                    value: Box::new(value),
                    index: Box::new(index),
                },
            },
            Postfix::Field(field) => match value {
                Expr::Ident(name) if field == "value" => Expr::Value(name),
                Expr::Ident(name) => Expr::Field { name, field },
                value => Expr::FieldValue {
                    value: Box::new(value),
                    field,
                },
            },
            Postfix::TupleField(field) => match value {
                Expr::Ident(name) => Expr::TupleField { name, field },
                value => Expr::TupleFieldValue {
                    value: Box::new(value),
                    field,
                },
            },
            Postfix::ResultOption => Expr::ResultOption(Box::new(value)),
            Postfix::TryResult => Expr::TryResult(Box::new(value)),
        };
    }
    Ok(value)
}

pub(super) fn collection(entries: Vec<CollectionEntry>) -> Result<Expr, &'static str> {
    if entries.is_empty() {
        return Ok(Expr::Map(Vec::new()));
    }
    if entries
        .iter()
        .all(|entry| matches!(entry, CollectionEntry::Record(_, _)))
    {
        return Ok(Expr::Record(
            entries
                .into_iter()
                .map(|entry| match entry {
                    CollectionEntry::Record(name, value) => (name, value),
                    CollectionEntry::Map(_, _) => unreachable!(),
                })
                .collect(),
        ));
    }
    if entries
        .iter()
        .all(|entry| matches!(entry, CollectionEntry::Map(_, _)))
    {
        return Ok(Expr::Map(
            entries
                .into_iter()
                .map(|entry| match entry {
                    CollectionEntry::Map(key, value) => (key, value),
                    CollectionEntry::Record(_, _) => unreachable!(),
                })
                .collect(),
        ));
    }
    Err("cannot mix record and map entries")
}

pub(super) fn do_expression(items: Vec<DoItem>) -> Result<Expr, &'static str> {
    let Some((last, preceding)) = items.split_last() else {
        return Err("do expression requires a result expression");
    };
    let DoItem::Expr(result) = last else {
        return Err("do expression must end with a result expression");
    };
    let mut steps = Vec::with_capacity(preceding.len());
    for item in preceding {
        match item {
            DoItem::Bind(name, expr) => steps.push(DoStep::Bind {
                name: name.clone(),
                expr: expr.clone(),
            }),
            DoItem::Let(name, annotation, expr) => steps.push(DoStep::Let {
                name: name.clone(),
                annotation: annotation.clone(),
                expr: expr.clone(),
            }),
            DoItem::Expr(_) => return Err("only the final do item may be an expression"),
        }
    }
    Ok(Expr::Do {
        steps,
        result: Box::new(result.clone()),
    })
}

pub(super) fn mark_checked_command(expr: &mut Expr) -> bool {
    match expr {
        Expr::Command { checked, .. } => {
            *checked = true;
            true
        }
        Expr::Pipeline { input, commands } => {
            *expr = Expr::TryPipeline {
                input: input.take(),
                commands: std::mem::take(commands),
            };
            true
        }
        Expr::StringContainsValue { value, .. }
        | Expr::StringIndexOfValue { value, .. }
        | Expr::StringStartsWithValue { value, .. }
        | Expr::StringEndsWithValue { value, .. }
        | Expr::StringSliceValue { value, .. }
        | Expr::StringRepeatValue { value, .. }
        | Expr::StringSplitValue { value, .. }
        | Expr::StringReplaceValue { value, .. }
        | Expr::StringLenValue(value)
        | Expr::StringIsEmptyValue(value)
        | Expr::StringTrimValue(value)
        | Expr::StringTrimStartValue(value)
        | Expr::StringTrimEndValue(value)
        | Expr::StringToUpperValue(value)
        | Expr::StringToLowerValue(value)
        | Expr::PathBasenameValue(value)
        | Expr::PathDirnameValue(value)
        | Expr::PathStemValue(value)
        | Expr::PathExtnameValue(value)
        | Expr::PathIsAbsoluteValue(value)
        | Expr::FieldValue { value, .. } => mark_checked_command(value),
        _ => false,
    }
}

pub(super) fn pipeline(mut first: Expr, commands: Vec<String>, checked: bool) -> Expr {
    if commands.is_empty() {
        if !checked {
            return first;
        }
        if mark_checked_command(&mut first) {
            return first;
        }
        return Expr::TryResult(Box::new(first));
    }

    let (input, mut stages) = match first {
        Expr::Command { command, .. } => (None, vec![command]),
        other => (Some(Box::new(other)), Vec::new()),
    };
    stages.extend(commands);
    if checked {
        Expr::TryPipeline {
            input,
            commands: stages,
        }
    } else {
        Expr::Pipeline {
            input,
            commands: stages,
        }
    }
}

pub(super) fn match_constructor(name: String, mut args: Vec<Expr>) -> Result<Expr, &'static str> {
    match (name.as_str(), args.len()) {
        ("Some", 1) => Ok(Expr::Some(Box::new(args.remove(0)))),
        ("Ok", 1) => Ok(Expr::Ok(Box::new(args.remove(0)))),
        ("Err", 1) => Ok(Expr::Err(Box::new(args.remove(0)))),
        ("Some" | "Ok" | "Err", _) => Err("constructor pattern arity"),
        _ => Ok(Expr::Call { name, args }),
    }
}

pub(super) fn trait_method(head: FunctionHead) -> Result<TraitMethod, &'static str> {
    if !head.type_params.is_empty() {
        return Err("trait methods cannot declare type parameters");
    }
    if head.params.iter().any(|param| param.default.is_some()) {
        return Err("trait methods cannot declare default parameters");
    }
    Ok(TraitMethod {
        name: head.name,
        params: head.params,
        return_type: head.return_type,
    })
}

pub(super) fn impl_method(head: FunctionHead, body: Program) -> Result<ImplMethod, &'static str> {
    if !head.type_params.is_empty() {
        return Err("impl methods cannot declare type parameters");
    }
    Ok(ImplMethod {
        name: head.name,
        params: head.params,
        return_type: head.return_type,
        body,
    })
}

pub(super) fn expression_statement(expr: Expr) -> Statement {
    match expr {
        Expr::Command {
            command,
            checked: true,
        } => Statement::TryCommand(command),
        Expr::Command {
            command,
            checked: false,
        } => Statement::Command(command),
        Expr::TryPipeline { input, commands } => Statement::TryPipeline { input, commands },
        Expr::Pipeline {
            input: None,
            commands,
        } => Statement::Command(commands.join(" | ")),
        Expr::Pipeline { input, commands } => Statement::Expr(Expr::Pipeline { input, commands }),
        Expr::TryResult(value) => Statement::TryResult(*value),
        other => Statement::Expr(other),
    }
}

pub(super) fn binding_statement(mutable: bool, target: BindingTarget, expr: Expr) -> Statement {
    match target {
        BindingTarget::Pattern(pattern) => Statement::Destructure {
            mutable,
            pattern,
            expr,
        },
        BindingTarget::Name(name, annotation) if mutable => Statement::Let {
            name,
            annotation,
            expr,
        },
        BindingTarget::Name(name, annotation) => Statement::Const {
            name,
            annotation,
            expr,
        },
    }
}
