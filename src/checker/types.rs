use super::names::applied_nominal_name;
use super::type_utils::{option_element_type, result_types, substitute_generics};
use super::TypeChecker;
use crate::{CompileError, Expr, Type};
use std::collections::{HashMap, HashSet};

impl TypeChecker {
    pub(super) fn resolve_type(&self, ty: &Type, line: usize) -> Result<Type, CompileError> {
        self.resolve_type_with_generics(ty, &HashSet::new(), line)
    }

    pub(super) fn resolve_type_with_generics(
        &self,
        ty: &Type,
        generics: &HashSet<String>,
        line: usize,
    ) -> Result<Type, CompileError> {
        match ty {
            Type::Named(name) if generics.contains(name) => Ok(Type::Generic(name.clone())),
            Type::Named(name) => self
                .types
                .get(name)
                .cloned()
                .ok_or_else(|| CompileError::new(line, format!("unknown type `{name}`"))),
            Type::Applied(name, args) if name == "Option" && args.len() == 1 => Ok(Type::Applied(
                name.clone(),
                vec![self.resolve_type_with_generics(&args[0], generics, line)?],
            )),
            Type::Applied(name, args) if name == "Result" && args.len() == 2 => Ok(Type::Applied(
                name.clone(),
                vec![
                    self.resolve_type_with_generics(&args[0], generics, line)?,
                    self.resolve_type_with_generics(&args[1], generics, line)?,
                ],
            )),
            Type::Applied(name, args) => {
                if let Some((type_params, body)) = self.generic_types.get(name) {
                    if args.len() != type_params.len() {
                        return Err(CompileError::new(
                            line,
                            format!(
                                "type `{name}` expects {} type arguments, found {}",
                                type_params.len(),
                                args.len()
                            ),
                        ));
                    }
                    let inferred = self.resolve_type_args(type_params, args, generics, line)?;
                    return Ok(substitute_generics(body, &inferred));
                }
                if let Some((type_params, base)) = self.generic_newtypes.get(name) {
                    if args.len() != type_params.len() {
                        return Err(CompileError::new(
                            line,
                            format!(
                                "type `{name}` expects {} type arguments, found {}",
                                type_params.len(),
                                args.len()
                            ),
                        ));
                    }
                    let inferred = self.resolve_type_args(type_params, args, generics, line)?;
                    return Ok(Type::Brand {
                        name: applied_nominal_name(name, type_params, &inferred),
                        base: Box::new(substitute_generics(base, &inferred)),
                    });
                }
                if let Some(type_params) = self.generic_sum_types.get(name) {
                    if args.len() != type_params.len() {
                        return Err(CompileError::new(
                            line,
                            format!(
                                "type `{name}` expects {} type arguments, found {}",
                                type_params.len(),
                                args.len()
                            ),
                        ));
                    }
                    let resolved_args = args
                        .iter()
                        .map(|arg| self.resolve_type_with_generics(arg, generics, line))
                        .collect::<Result<Vec<_>, _>>()?;
                    return Ok(Type::Applied(name.clone(), resolved_args));
                }
                Err(CompileError::new(line, format!("unknown type `{name}`")))
            }
            Type::Array(element) => Ok(Type::Array(Box::new(
                self.resolve_type_with_generics(element, generics, line)?,
            ))),
            Type::Future(value) => Ok(Type::Future(Box::new(
                self.resolve_type_with_generics(value, generics, line)?,
            ))),
            Type::Map(key, value) => Ok(Type::Map(
                Box::new(self.resolve_type_with_generics(key, generics, line)?),
                Box::new(self.resolve_type_with_generics(value, generics, line)?),
            )),
            Type::Record(fields) => {
                let mut resolved = Vec::new();
                for (name, ty) in fields {
                    resolved.push((
                        name.clone(),
                        self.resolve_type_with_generics(ty, generics, line)?,
                    ));
                }
                Ok(Type::Record(resolved))
            }
            Type::Tuple(elements) => {
                let mut resolved = Vec::new();
                for element in elements {
                    resolved.push(self.resolve_type_with_generics(element, generics, line)?);
                }
                Ok(Type::Tuple(resolved))
            }
            Type::Function(params, return_type) => {
                let mut resolved_params = Vec::new();
                for param in params {
                    resolved_params.push(self.resolve_type_with_generics(param, generics, line)?);
                }
                Ok(Type::Function(
                    resolved_params,
                    Box::new(self.resolve_type_with_generics(return_type, generics, line)?),
                ))
            }
            Type::Union(types) => {
                let mut resolved = Vec::new();
                for ty in types {
                    resolved.push(self.resolve_type_with_generics(ty, generics, line)?);
                }
                Ok(Type::Union(resolved))
            }
            Type::Intersection(types) => {
                let mut resolved = Vec::new();
                for ty in types {
                    resolved.push(self.resolve_type_with_generics(ty, generics, line)?);
                }
                Ok(Type::Intersection(resolved))
            }
            other => Ok(other.clone()),
        }
    }

    pub(super) fn resolve_type_args(
        &self,
        type_params: &[String],
        args: &[Type],
        generics: &HashSet<String>,
        line: usize,
    ) -> Result<HashMap<String, Type>, CompileError> {
        let mut inferred = HashMap::new();
        for (type_param, arg) in type_params.iter().zip(args) {
            inferred.insert(
                type_param.clone(),
                self.resolve_type_with_generics(arg, generics, line)?,
            );
        }
        Ok(inferred)
    }

    pub(super) fn is_assignable(&self, expected: &Type, actual: &Type, expr: &Expr) -> bool {
        if expected == actual {
            return true;
        }
        match (expected, actual) {
            (Type::Union(types), actual) => types
                .iter()
                .any(|expected| self.is_assignable(expected, actual, expr)),
            (expected, Type::Union(types)) => types
                .iter()
                .all(|actual| self.is_assignable(expected, actual, expr)),
            (Type::Intersection(types), actual) => types
                .iter()
                .all(|expected| self.is_assignable(expected, actual, expr)),
            (expected, Type::Intersection(types)) => types
                .iter()
                .any(|actual| self.is_assignable(expected, actual, expr)),
            (Type::Float, Type::Int) => true,
            (Type::Path, Type::String) => true,
            (Type::String, Type::Path) => true,
            (Type::ExitCode, Type::Int) => {
                matches!(expr, Expr::Int(value) if (0..=255).contains(value))
            }
            (Type::Array(expected), Type::Array(actual)) if **actual == Type::Unit => {
                !matches!(**expected, Type::Unit)
            }
            (Type::Array(expected), Type::Array(actual)) => {
                self.is_assignable(expected, actual, expr)
            }
            (expected, Type::Applied(name, args))
                if name == "Option" && args.len() == 1 && args[0] == Type::Unit =>
            {
                option_element_type(expected).is_some()
            }
            (expected, Type::Applied(name, args))
                if name == "Result" && args.len() == 2 && args[1] == Type::Unit =>
            {
                result_types(expected).is_some_and(|(ok, _)| self.is_assignable(ok, &args[0], expr))
            }
            (expected, Type::Applied(name, args))
                if name == "Result" && args.len() == 2 && args[0] == Type::Unit =>
            {
                result_types(expected)
                    .is_some_and(|(_, err)| self.is_assignable(err, &args[1], expr))
            }
            (
                Type::Applied(expected_name, expected_args),
                Type::Applied(actual_name, actual_args),
            ) if expected_name == "Option"
                && actual_name == "Option"
                && expected_args.len() == 1
                && actual_args.len() == 1 =>
            {
                self.is_assignable(&expected_args[0], &actual_args[0], expr)
            }
            (
                Type::Applied(expected_name, expected_args),
                Type::Applied(actual_name, actual_args),
            ) if expected_name == "Result"
                && actual_name == "Result"
                && expected_args.len() == 2
                && actual_args.len() == 2 =>
            {
                self.is_assignable(&expected_args[0], &actual_args[0], expr)
                    && self.is_assignable(&expected_args[1], &actual_args[1], expr)
            }
            (
                Type::Applied(expected_name, expected_args),
                Type::Applied(actual_name, actual_args),
            ) if expected_name == actual_name && expected_args.len() == actual_args.len() => {
                expected_args
                    .iter()
                    .zip(actual_args)
                    .all(|(expected, actual)| self.is_assignable(expected, actual, expr))
            }
            (Type::Future(expected), Type::Future(actual)) => {
                self.is_assignable(expected, actual, expr)
            }
            (Type::Map(expected_key, expected_value), Type::Map(actual_key, actual_value))
                if **actual_key == Type::Unit && **actual_value == Type::Unit =>
            {
                !matches!(**expected_key, Type::Unit) && !matches!(**expected_value, Type::Unit)
            }
            (Type::Map(expected_key, expected_value), Type::Map(actual_key, actual_value)) => {
                self.is_assignable(expected_key, actual_key, expr)
                    && self.is_assignable(expected_value, actual_value, expr)
            }
            (Type::Record(expected), Type::Record(actual)) if expected.len() == actual.len() => {
                expected.iter().all(|(name, expected_ty)| {
                    actual
                        .iter()
                        .find(|(actual_name, _)| actual_name == name)
                        .is_some_and(|(_, actual_ty)| {
                            self.is_assignable(expected_ty, actual_ty, expr)
                        })
                })
            }
            (Type::Tuple(expected), Type::Tuple(actual)) if expected.len() == actual.len() => {
                expected
                    .iter()
                    .zip(actual)
                    .all(|(expected, actual)| self.is_assignable(expected, actual, expr))
            }
            (
                Type::Function(expected_params, expected_return),
                Type::Function(actual_params, actual_return),
            ) if expected_params.len() == actual_params.len() => {
                expected_params
                    .iter()
                    .zip(actual_params)
                    .all(|(expected, actual)| self.is_assignable(expected, actual, expr))
                    && self.is_assignable(expected_return, actual_return, expr)
            }
            (Type::Brand { name: expected, .. }, Type::Brand { name: actual, .. }) => {
                expected == actual
            }
            _ => false,
        }
    }

    pub(super) fn is_numeric(&self, ty: &Type) -> bool {
        matches!(ty, Type::Int | Type::Float | Type::ExitCode)
    }

    pub(super) fn is_integer_numeric(&self, ty: &Type) -> bool {
        matches!(ty, Type::Int | Type::ExitCode)
    }

    pub(super) fn is_string_like(&self, ty: &Type) -> bool {
        matches!(ty, Type::String | Type::Path)
    }

    pub(super) fn is_castable(&self, target: &Type, actual: &Type, expr: &Expr) -> bool {
        if self.is_assignable(target, actual, expr) || self.is_assignable(actual, target, expr) {
            return true;
        }
        match (target, actual) {
            (Type::String, actual) => matches!(
                actual,
                Type::Int
                    | Type::Float
                    | Type::Bool
                    | Type::String
                    | Type::Path
                    | Type::ExitCode
                    | Type::Unit
                    | Type::Generic(_)
            ),
            (Type::Brand { base, .. }, _) => self.is_assignable(base, actual, expr),
            (_, Type::Brand { base, .. }) => self.is_assignable(target, base, expr),
            _ => false,
        }
    }

    pub(super) fn is_comparable_by_equality(&self, left: &Type, right: &Type) -> bool {
        left == right
            || (self.is_numeric(left) && self.is_numeric(right))
            || matches!(
                (left, right),
                (Type::String, Type::Path) | (Type::Path, Type::String)
            )
    }
}
