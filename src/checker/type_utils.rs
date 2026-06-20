use std::collections::HashMap;

use crate::Type;

pub(super) fn substitute_generics(ty: &Type, inferred: &HashMap<String, Type>) -> Type {
    match ty {
        Type::Generic(name) => inferred.get(name).cloned().unwrap_or_else(|| ty.clone()),
        Type::Array(element) => Type::Array(Box::new(substitute_generics(element, inferred))),
        Type::Future(value) => Type::Future(Box::new(substitute_generics(value, inferred))),
        Type::Map(key, value) => Type::Map(
            Box::new(substitute_generics(key, inferred)),
            Box::new(substitute_generics(value, inferred)),
        ),
        Type::Record(fields) => Type::Record(
            fields
                .iter()
                .map(|(name, ty)| (name.clone(), substitute_generics(ty, inferred)))
                .collect(),
        ),
        Type::Tuple(elements) => Type::Tuple(
            elements
                .iter()
                .map(|element| substitute_generics(element, inferred))
                .collect(),
        ),
        Type::Function(params, return_type) => Type::Function(
            params
                .iter()
                .map(|param| substitute_generics(param, inferred))
                .collect(),
            Box::new(substitute_generics(return_type, inferred)),
        ),
        Type::Union(types) => Type::Union(
            types
                .iter()
                .map(|ty| substitute_generics(ty, inferred))
                .collect(),
        ),
        Type::Intersection(types) => Type::Intersection(
            types
                .iter()
                .map(|ty| substitute_generics(ty, inferred))
                .collect(),
        ),
        Type::Applied(name, args) => Type::Applied(
            name.clone(),
            args.iter()
                .map(|arg| substitute_generics(arg, inferred))
                .collect(),
        ),
        other => other.clone(),
    }
}

pub(super) fn option_element_type(ty: &Type) -> Option<&Type> {
    match ty {
        Type::Applied(name, args) if name == "Option" && args.len() == 1 => Some(&args[0]),
        _ => None,
    }
}

pub(super) fn is_scalar_backed_type(ty: &Type) -> bool {
    match ty {
        Type::Array(_) | Type::Map(_, _) | Type::Record(_) | Type::Tuple(_) => false,
        Type::Applied(_, args) | Type::Union(args) | Type::Intersection(args) => {
            args.iter().all(is_scalar_backed_type)
        }
        Type::Brand { base, .. } => is_scalar_backed_type(base),
        Type::Generic(_) | Type::Named(_) => false,
        Type::Int
        | Type::Float
        | Type::Bool
        | Type::String
        | Type::Path
        | Type::ExitCode
        | Type::Unit
        | Type::Future(_)
        | Type::Function(_, _) => true,
    }
}

pub(super) fn capture_suffixes(ty: &Type) -> Vec<String> {
    fn append(prefix: &str, ty: &Type, suffixes: &mut Vec<String>) {
        match ty {
            Type::Record(fields) => {
                for (field, ty) in fields {
                    append(&format!("{prefix}_{field}"), ty, suffixes);
                }
            }
            Type::Tuple(fields) => {
                for (index, ty) in fields.iter().enumerate() {
                    append(&format!("{prefix}_{}", index + 1), ty, suffixes);
                }
            }
            Type::Applied(name, args) if name == "Option" || name == "Result" => {
                suffixes.push(prefix.to_string());
                for ty in args {
                    if !is_scalar_backed_type(ty) {
                        append(prefix, ty, suffixes);
                    }
                }
            }
            Type::Brand { base, .. } => append(prefix, base, suffixes),
            Type::Union(types) | Type::Intersection(types) => {
                for ty in types {
                    append(prefix, ty, suffixes);
                }
            }
            _ => suffixes.push(prefix.to_string()),
        }
    }

    let mut suffixes = Vec::new();
    append("", ty, &mut suffixes);
    suffixes.sort();
    suffixes.dedup();
    suffixes
}

pub(super) fn result_types(ty: &Type) -> Option<(&Type, &Type)> {
    match ty {
        Type::Applied(name, args) if name == "Result" && args.len() == 2 => {
            Some((&args[0], &args[1]))
        }
        _ => None,
    }
}

pub(super) fn default_success_type(ty: &Type) -> Option<&Type> {
    option_element_type(ty).or_else(|| result_types(ty).map(|(ok, _)| ok))
}

pub(super) fn command_result_type() -> Type {
    Type::Applied("Result".to_string(), vec![Type::String, cmd_error_type()])
}

pub(super) fn command_output_type() -> Type {
    Type::Record(vec![
        ("stdout".to_string(), Type::String),
        ("stderr".to_string(), Type::String),
        ("status".to_string(), Type::ExitCode),
        ("success".to_string(), Type::Bool),
    ])
}

pub(super) fn cmd_error_type() -> Type {
    Type::Record(vec![
        ("code".to_string(), Type::ExitCode),
        ("stderr".to_string(), Type::String),
    ])
}
