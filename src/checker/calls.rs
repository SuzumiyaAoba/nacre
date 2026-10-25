use super::names::{applied_nominal_name, method_call_parts};
use super::type_utils::substitute_generics;
use super::{FunctionSig, TypeChecker};
use crate::symbols::is_identifier;
use crate::{CompileError, Expr, Type};
use std::collections::{HashMap, HashSet};

impl TypeChecker {
    pub(super) fn normalize_call_args(
        &self,
        name: &str,
        args: &[Expr],
        sig: &FunctionSig,
        line: usize,
    ) -> Result<Vec<Expr>, CompileError> {
        let has_named = args.iter().any(|arg| matches!(arg, Expr::NamedArg { .. }));
        if !has_named {
            return Ok(args.to_vec());
        }

        let has_rest = sig.params.last().is_some_and(|param| param.variadic);
        let fixed_len = if has_rest {
            sig.params.len() - 1
        } else {
            sig.params.len()
        };
        let mut values = vec![None; fixed_len];
        let mut next_positional = 0;
        let mut seen_named = HashSet::new();
        let mut seen_any_named = false;

        for arg in args {
            match arg {
                Expr::NamedArg {
                    name: arg_name,
                    value,
                } => {
                    seen_any_named = true;
                    let Some(index) = sig.params[..fixed_len]
                        .iter()
                        .position(|param| param.name == *arg_name)
                    else {
                        return Err(CompileError::new(
                            line,
                            format!("function `{name}` has no parameter `{arg_name}`"),
                        ));
                    };
                    if !seen_named.insert(arg_name.clone()) || values[index].is_some() {
                        return Err(CompileError::new(
                            line,
                            format!("argument `{arg_name}` for function `{name}` was provided more than once"),
                        ));
                    }
                    values[index] = Some((**value).clone());
                }
                value => {
                    if seen_any_named {
                        return Err(CompileError::new(
                            line,
                            format!(
                                "positional arguments for function `{name}` must appear before named arguments"
                            ),
                        ));
                    }
                    while next_positional < fixed_len && values[next_positional].is_some() {
                        next_positional += 1;
                    }
                    if next_positional >= fixed_len {
                        if has_rest {
                            return Err(CompileError::new(
                                line,
                                format!(
                                    "positional rest arguments for function `{name}` must appear before named arguments"
                                ),
                            ));
                        }
                        return Err(CompileError::new(
                            line,
                            format!(
                                "function `{name}` expects at most {fixed_len} arguments before named arguments"
                            ),
                        ));
                    }
                    values[next_positional] = Some(value.clone());
                    next_positional += 1;
                }
            }
        }

        let mut normalized = Vec::with_capacity(fixed_len);
        for (index, value) in values.into_iter().enumerate() {
            if let Some(value) = value {
                normalized.push((value, true));
            } else if let Some(default) = &sig.params[index].default {
                normalized.push((default.clone(), false));
            } else {
                return Err(CompileError::new(
                    line,
                    format!(
                        "missing argument `{}` for function `{name}`",
                        sig.params[index].name
                    ),
                ));
            }
        }

        while normalized
            .last()
            .zip(sig.params.get(normalized.len().saturating_sub(1)))
            .is_some_and(|((_, explicit), param)| !explicit && param.default.is_some())
        {
            normalized.pop();
        }
        Ok(normalized.into_iter().map(|(value, _)| value).collect())
    }

    pub(super) fn resolve_method_name(
        &self,
        receiver: &str,
        method: &str,
        line: usize,
    ) -> Result<String, CompileError> {
        let receiver_ty = self.check_expr(&Expr::Ident(receiver.to_string()), line)?;
        if let Some(name) = self
            .inherent_methods
            .get(&(method.to_string(), receiver_ty.name()))
        {
            return Ok(name.clone());
        }
        if let Some(candidates) = self
            .method_impls
            .get(&(method.to_string(), receiver_ty.name()))
        {
            if candidates.len() == 1 {
                return Ok(candidates[0].1.clone());
            }
            let traits = candidates
                .iter()
                .map(|(trait_name, _)| trait_name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(CompileError::new(
                line,
                format!(
                    "ambiguous method `{method}` for {}; use one of: {traits}",
                    receiver_ty.name()
                ),
            ));
        }
        if self.functions.contains_key(method) {
            return Ok(method.to_string());
        }
        Err(CompileError::new(
            line,
            format!("undefined function `{method}`"),
        ))
    }

    pub(super) fn resolve_unscoped_method_call(
        &self,
        method: &str,
        args: &[Expr],
        line: usize,
    ) -> Result<Option<String>, CompileError> {
        if self.functions.contains_key(method) {
            return Ok(Some(method.to_string()));
        }
        let Some(receiver) = args.first() else {
            return Ok(None);
        };
        let receiver_ty = self.check_expr(receiver, line)?;
        if let Some(name) = self
            .inherent_methods
            .get(&(method.to_string(), receiver_ty.name()))
        {
            return Ok(Some(name.clone()));
        }
        let Some(candidates) = self
            .method_impls
            .get(&(method.to_string(), receiver_ty.name()))
        else {
            return Ok(None);
        };
        if candidates.len() == 1 {
            return Ok(Some(candidates[0].1.clone()));
        }
        let traits = candidates
            .iter()
            .map(|(trait_name, _)| trait_name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        Err(CompileError::new(
            line,
            format!(
                "ambiguous method `{method}` for {}; use one of: {traits}",
                receiver_ty.name()
            ),
        ))
    }

    pub(super) fn resolve_scoped_method_name(
        &self,
        trait_name: &str,
        method: &str,
        args: &[Expr],
        line: usize,
    ) -> Result<String, CompileError> {
        let Some(receiver) = args.first() else {
            return Err(CompileError::new(
                line,
                format!("trait method `{trait_name}.{method}` requires a receiver argument"),
            ));
        };
        let receiver_ty = self.check_expr(receiver, line)?;
        let Some(candidates) = self
            .method_impls
            .get(&(method.to_string(), receiver_ty.name()))
        else {
            return Err(CompileError::new(
                line,
                format!(
                    "type {} does not implement trait `{trait_name}` method `{method}`",
                    receiver_ty.name()
                ),
            ));
        };
        candidates
            .iter()
            .find(|(candidate_trait, _)| candidate_trait == trait_name)
            .map(|(_, lowered_name)| lowered_name.clone())
            .ok_or_else(|| {
                CompileError::new(
                    line,
                    format!(
                        "type {} does not implement trait `{trait_name}` method `{method}`",
                        receiver_ty.name()
                    ),
                )
            })
    }

    pub(super) fn scoped_trait_method_parts<'a>(
        &self,
        name: &'a str,
    ) -> Option<(&'a str, &'a str)> {
        let (trait_name, method) = name.rsplit_once('.')?;
        if self.traits.contains_key(trait_name) && is_identifier(method) {
            Some((trait_name, method))
        } else {
            None
        }
    }

    pub(super) fn check_newtype_ctor(
        &self,
        name: &str,
        value: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        if self.constructor_overrides.contains(name) {
            return self.check_call(name, std::slice::from_ref(value), line);
        }
        let Some(ty) = self.types.get(name) else {
            let Some((type_params, base)) = self.generic_newtypes.get(name) else {
                return Err(CompileError::new(line, format!("unknown type `{name}`")));
            };
            let actual = self.check_expr(value, line)?;
            let mut inferred = HashMap::new();
            self.infer_generic_type(base, &actual, value, &mut inferred)
                .map_err(|message| {
                    CompileError::new(
                        line,
                        format!("newtype constructor `{name}` expected {message}"),
                    )
                })?;
            let mut args = Vec::with_capacity(type_params.len());
            for type_param in type_params {
                let Some(ty) = inferred.get(type_param).cloned() else {
                    return Err(CompileError::new(
                        line,
                        format!("could not infer generic type `{type_param}` for newtype `{name}`"),
                    ));
                };
                args.push(ty);
            }
            let mut concrete = HashMap::new();
            for (type_param, ty) in type_params.iter().zip(&args) {
                concrete.insert(type_param.clone(), ty.clone());
            }
            return Ok(Type::Brand {
                name: applied_nominal_name(name, type_params, &concrete),
                base: Box::new(substitute_generics(base, &concrete)),
            });
        };
        let Type::Brand { base, .. } = ty else {
            return Err(CompileError::new(
                line,
                format!("type `{name}` is not a newtype"),
            ));
        };
        let value_ty = self.check_expr(value, line)?;
        if self.is_assignable(base, &value_ty, value) {
            Ok(ty.clone())
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "newtype constructor `{name}` expected {}, found {}",
                    base.name(),
                    value_ty.name()
                ),
            ))
        }
    }

    pub(super) fn check_newtype_ctor_expected(
        &self,
        name: &str,
        value: &Expr,
        expected: &Type,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Type::Brand {
            name: expected_name,
            base,
        } = expected
        else {
            return self.check_newtype_ctor(name, value, line);
        };
        if !expected_name.starts_with(&format!("{name}[")) {
            return self.check_newtype_ctor(name, value, line);
        }
        let actual = self.check_expr_expected(value, base, line)?;
        if self.is_assignable(base, &actual, value) {
            Ok(expected.clone())
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "newtype constructor `{name}` expected {}, found {}",
                    base.name(),
                    actual.name()
                ),
            ))
        }
    }

    pub(super) fn check_call(
        &self,
        name: &str,
        args: &[Expr],
        line: usize,
    ) -> Result<Type, CompileError> {
        if let Some(ty) = self.check_numeric_builtin_call(name, args, line)? {
            return Ok(ty);
        }
        if let Some((trait_name, method)) = self.scoped_trait_method_parts(name) {
            let lowered_name = self.resolve_scoped_method_name(trait_name, method, args, line)?;
            return self.check_call(&lowered_name, args, line);
        }
        if let Some((receiver, method)) = method_call_parts(name) {
            if !self.functions.contains_key(name) {
                let lowered_name = self.resolve_method_name(receiver, method, line)?;
                let mut lowered_args = Vec::with_capacity(args.len() + 1);
                lowered_args.push(Expr::Ident(receiver.to_string()));
                lowered_args.extend_from_slice(args);
                return self.check_call(&lowered_name, &lowered_args, line);
            }
        }
        let Some(sig) = self.functions.get(name) else {
            if let Some(lowered_name) = self.resolve_unscoped_method_call(name, args, line)? {
                return self.check_call(&lowered_name, args, line);
            }
            return self.check_function_value_call(name, args, line);
        };
        let args = self.normalize_call_args(name, args, sig, line)?;
        if !sig.type_params.is_empty() {
            return self.check_generic_call(name, sig, &args, line);
        }
        let min_args = sig
            .params
            .iter()
            .filter(|param| !param.variadic)
            .filter(|param| param.default.is_none())
            .count();
        let has_rest = sig.params.last().is_some_and(|param| param.variadic);
        let max_args = if has_rest {
            usize::MAX
        } else {
            sig.params.len()
        };
        if args.len() < min_args || args.len() > max_args {
            let expected = if has_rest {
                format!("{min_args}..")
            } else {
                format!("{min_args}..{}", sig.params.len())
            };
            return Err(CompileError::new(
                line,
                format!(
                    "function `{name}` expects {expected} arguments, found {}",
                    args.len()
                ),
            ));
        }
        for (arg, param) in args
            .iter()
            .zip(sig.params.iter().filter(|param| !param.variadic))
        {
            let actual = self.check_expr_expected(arg, &param.ty, line)?;
            if !self.is_assignable(&param.ty, &actual, arg) {
                return Err(CompileError::new(
                    line,
                    format!(
                        "argument `{}` for function `{name}`: expected {}, found {}",
                        param.name,
                        param.ty.name(),
                        actual.name()
                    ),
                ));
            }
        }
        if let Some(rest_param) = sig.params.last().filter(|param| param.variadic) {
            let Type::Array(element_ty) = &rest_param.ty else {
                return Err(CompileError::new(
                    line,
                    format!("rest parameter `{}` must have Array type", rest_param.name),
                ));
            };
            let fixed_len = sig.params.len() - 1;
            for arg in args.iter().skip(fixed_len) {
                let actual = self.check_expr(arg, line)?;
                if !self.is_assignable(element_ty, &actual, arg) {
                    return Err(CompileError::new(
                        line,
                        format!(
                            "rest argument for function `{name}`: expected {}, found {}",
                            element_ty.name(),
                            actual.name()
                        ),
                    ));
                }
            }
        }
        Ok(sig.return_type.clone())
    }

    pub(super) fn check_numeric_builtin_call(
        &self,
        name: &str,
        args: &[Expr],
        line: usize,
    ) -> Result<Option<Type>, CompileError> {
        let expected = match name {
            "Int.parse" | "Int.tryParse" | "Float.parse" | "Float.tryParse" | "parseInt"
            | "tryParseInt" | "parseFloat" | "tryParseFloat" | "numeric_abs" | "numeric_floor"
            | "numeric_ceil" | "numeric_round" => 1,
            "numeric_min" | "numeric_max" => 2,
            "numeric_clamp" => 3,
            _ => return Ok(None),
        };
        if args.len() != expected {
            return Err(CompileError::new(
                line,
                format!(
                    "function `{name}` expects {expected} arguments, found {}",
                    args.len()
                ),
            ));
        }
        match name {
            "Int.parse" | "parseInt" => {
                self.check_string_arg(name, &args[0], line)?;
                Ok(Some(Type::Int))
            }
            "Int.tryParse" | "tryParseInt" => {
                self.check_string_arg(name, &args[0], line)?;
                Ok(Some(Type::Applied("Option".to_string(), vec![Type::Int])))
            }
            "Float.parse" | "parseFloat" => {
                self.check_string_arg(name, &args[0], line)?;
                Ok(Some(Type::Float))
            }
            "Float.tryParse" | "tryParseFloat" => {
                self.check_string_arg(name, &args[0], line)?;
                Ok(Some(Type::Applied("Option".to_string(), vec![Type::Float])))
            }
            "numeric_abs" => {
                let ty = self.check_numeric_arg(name, &args[0], line)?;
                Ok(Some(if ty == Type::Float {
                    Type::Float
                } else {
                    Type::Int
                }))
            }
            "numeric_floor" | "numeric_ceil" | "numeric_round" => {
                self.check_numeric_arg(name, &args[0], line)?;
                Ok(Some(Type::Int))
            }
            "numeric_min" | "numeric_max" => {
                let left = self.check_numeric_arg(name, &args[0], line)?;
                let right = self.check_numeric_arg(name, &args[1], line)?;
                Ok(Some(if left == Type::Float || right == Type::Float {
                    Type::Float
                } else {
                    Type::Int
                }))
            }
            "numeric_clamp" => {
                let value = self.check_numeric_arg(name, &args[0], line)?;
                let min = self.check_numeric_arg(name, &args[1], line)?;
                let max = self.check_numeric_arg(name, &args[2], line)?;
                Ok(Some(
                    if value == Type::Float || min == Type::Float || max == Type::Float {
                        Type::Float
                    } else {
                        Type::Int
                    },
                ))
            }
            _ => Ok(None),
        }
    }

    pub(super) fn check_string_arg(
        &self,
        name: &str,
        arg: &Expr,
        line: usize,
    ) -> Result<(), CompileError> {
        let ty = self.check_expr(arg, line)?;
        if self.is_string_like(&ty) {
            Ok(())
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "function `{name}` expects String argument, found {}",
                    ty.name()
                ),
            ))
        }
    }

    pub(super) fn check_numeric_arg(
        &self,
        name: &str,
        arg: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let ty = self.check_expr(arg, line)?;
        if self.is_numeric(&ty) {
            Ok(if ty == Type::ExitCode { Type::Int } else { ty })
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "function `{name}` expects numeric argument, found {}",
                    ty.name()
                ),
            ))
        }
    }

    pub(super) fn check_generic_call(
        &self,
        name: &str,
        sig: &FunctionSig,
        args: &[Expr],
        line: usize,
    ) -> Result<Type, CompileError> {
        let has_rest = sig.params.last().is_some_and(|param| param.variadic);
        let fixed_len = if has_rest {
            sig.params.len() - 1
        } else {
            sig.params.len()
        };
        if (!has_rest && args.len() != sig.params.len()) || (has_rest && args.len() < fixed_len) {
            let expected = if has_rest {
                format!("{fixed_len}..")
            } else {
                sig.params.len().to_string()
            };
            return Err(CompileError::new(
                line,
                format!(
                    "generic function `{name}` expects {expected} arguments, found {}",
                    args.len()
                ),
            ));
        }
        let mut inferred = HashMap::new();
        for (arg, param) in args.iter().take(fixed_len).zip(&sig.params[..fixed_len]) {
            let actual = self.check_expr(arg, line)?;
            self.infer_generic_type(&param.ty, &actual, arg, &mut inferred)
                .map_err(|message| {
                    CompileError::new(
                        line,
                        format!("argument `{}` for function `{name}`: {message}", param.name),
                    )
                })?;
        }
        if let Some(rest_param) = sig.params.last().filter(|param| param.variadic) {
            let Type::Array(element_ty) = &rest_param.ty else {
                return Err(CompileError::new(
                    line,
                    format!("rest parameter `{}` must have Array type", rest_param.name),
                ));
            };
            for arg in args.iter().skip(fixed_len) {
                let actual = self.check_expr(arg, line)?;
                self.infer_generic_type(element_ty, &actual, arg, &mut inferred)
                    .map_err(|message| {
                        CompileError::new(
                            line,
                            format!(
                                "rest argument for function `{name}` parameter `{}`: {message}",
                                rest_param.name
                            ),
                        )
                    })?;
            }
        }
        for type_param in &sig.type_params {
            let Some(inferred_ty) = inferred.get(&type_param.name) else {
                return Err(CompileError::new(
                    line,
                    format!(
                        "could not infer generic type `{}` for function `{name}`",
                        type_param.name
                    ),
                ));
            };
            for bound in &type_param.bounds {
                if !self
                    .trait_impls
                    .contains(&(bound.clone(), inferred_ty.name()))
                {
                    return Err(CompileError::new(
                        line,
                        format!(
                            "type {} does not implement trait `{bound}`",
                            inferred_ty.name()
                        ),
                    ));
                }
            }
        }
        Ok(substitute_generics(&sig.return_type, &inferred))
    }

    pub(super) fn infer_generic_type(
        &self,
        expected: &Type,
        actual: &Type,
        expr: &Expr,
        inferred: &mut HashMap<String, Type>,
    ) -> Result<(), String> {
        match expected {
            Type::Generic(name) => {
                if let Some(existing) = inferred.get(name) {
                    if self.is_assignable(existing, actual, expr)
                        && self.is_assignable(actual, existing, expr)
                    {
                        Ok(())
                    } else {
                        Err(format!(
                            "generic type `{name}` expected {}, found {}",
                            existing.name(),
                            actual.name()
                        ))
                    }
                } else {
                    inferred.insert(name.clone(), actual.clone());
                    Ok(())
                }
            }
            Type::Array(expected) => {
                let Type::Array(actual) = actual else {
                    return Err(format!(
                        "expected {}, found {}",
                        expected.name(),
                        actual.name()
                    ));
                };
                self.infer_generic_type(expected, actual, expr, inferred)
            }
            Type::Future(expected) => {
                let Type::Future(actual) = actual else {
                    return Err(format!(
                        "expected {}, found {}",
                        expected.name(),
                        actual.name()
                    ));
                };
                self.infer_generic_type(expected, actual, expr, inferred)
            }
            Type::Map(expected_key, expected_value) => {
                let Type::Map(actual_key, actual_value) = actual else {
                    return Err(format!(
                        "expected {}, found {}",
                        Type::Map(expected_key.clone(), expected_value.clone()).name(),
                        actual.name()
                    ));
                };
                self.infer_generic_type(expected_key, actual_key, expr, inferred)?;
                self.infer_generic_type(expected_value, actual_value, expr, inferred)
            }
            Type::Tuple(expected) => {
                let Type::Tuple(actual) = actual else {
                    return Err(format!(
                        "expected {}, found {}",
                        Type::Tuple(expected.clone()).name(),
                        actual.name()
                    ));
                };
                if expected.len() != actual.len() {
                    return Err(format!(
                        "expected {}, found {}",
                        Type::Tuple(expected.clone()).name(),
                        Type::Tuple(actual.clone()).name()
                    ));
                }
                for (expected, actual) in expected.iter().zip(actual) {
                    self.infer_generic_type(expected, actual, expr, inferred)?;
                }
                Ok(())
            }
            Type::Record(expected) => {
                let Type::Record(actual) = actual else {
                    return Err(format!(
                        "expected {}, found {}",
                        Type::Record(expected.clone()).name(),
                        actual.name()
                    ));
                };
                for (field, expected_ty) in expected {
                    let Some((_, actual_ty)) = actual
                        .iter()
                        .find(|(actual_field, _)| actual_field == field)
                    else {
                        return Err(format!("record field `{field}` is missing"));
                    };
                    self.infer_generic_type(expected_ty, actual_ty, expr, inferred)?;
                }
                Ok(())
            }
            Type::Function(expected_params, expected_return) => {
                let Type::Function(actual_params, actual_return) = actual else {
                    return Err(format!(
                        "expected {}, found {}",
                        expected.name(),
                        actual.name()
                    ));
                };
                if expected_params.len() != actual_params.len() {
                    return Err(format!(
                        "expected {}, found {}",
                        expected.name(),
                        actual.name()
                    ));
                }
                for (expected, actual) in expected_params.iter().zip(actual_params) {
                    self.infer_generic_type(expected, actual, expr, inferred)?;
                }
                self.infer_generic_type(expected_return, actual_return, expr, inferred)
            }
            Type::Applied(expected_name, expected_args) => {
                let Type::Applied(actual_name, actual_args) = actual else {
                    return Err(format!(
                        "expected {}, found {}",
                        expected.name(),
                        actual.name()
                    ));
                };
                if expected_name != actual_name || expected_args.len() != actual_args.len() {
                    return Err(format!(
                        "expected {}, found {}",
                        expected.name(),
                        actual.name()
                    ));
                }
                for (expected, actual) in expected_args.iter().zip(actual_args) {
                    self.infer_generic_type(expected, actual, expr, inferred)?;
                }
                Ok(())
            }
            Type::Union(expected_types) => {
                for expected in expected_types {
                    let mut candidate = inferred.clone();
                    if self
                        .infer_generic_type(expected, actual, expr, &mut candidate)
                        .is_ok()
                    {
                        *inferred = candidate;
                        return Ok(());
                    }
                }
                Err(format!(
                    "expected {}, found {}",
                    expected.name(),
                    actual.name()
                ))
            }
            Type::Intersection(expected_types) => {
                for expected in expected_types {
                    self.infer_generic_type(expected, actual, expr, inferred)?;
                }
                Ok(())
            }
            _ if self.is_assignable(expected, actual, expr) => Ok(()),
            _ => Err(format!(
                "expected {}, found {}",
                expected.name(),
                actual.name()
            )),
        }
    }

    pub(super) fn check_function_value_call(
        &self,
        name: &str,
        args: &[Expr],
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined function `{name}`"),
            ));
        };
        let Type::Function(params, return_type) = &binding.ty else {
            return Err(CompileError::new(line, format!("`{name}` is not callable")));
        };
        if args.len() != params.len() {
            return Err(CompileError::new(
                line,
                format!(
                    "function value `{name}` expects {} arguments, found {}",
                    params.len(),
                    args.len()
                ),
            ));
        }
        for (index, (arg, expected)) in args.iter().zip(params).enumerate() {
            let actual = self.check_expr_expected(arg, expected, line)?;
            if !self.is_assignable(expected, &actual, arg) {
                return Err(CompileError::new(
                    line,
                    format!(
                        "argument {} for function value `{name}`: expected {}, found {}",
                        index + 1,
                        expected.name(),
                        actual.name()
                    ),
                ));
            }
        }
        Ok((**return_type).clone())
    }
}
