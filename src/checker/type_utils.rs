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
