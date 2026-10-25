use super::type_utils::{
    command_result_type, option_element_type, result_types, substitute_generics,
};
use super::validation::match_pattern_mismatch;
use super::TypeChecker;
use crate::{CompileError, Expr, MatchArm, Type};
use std::collections::HashMap;

impl TypeChecker {
    pub(super) fn lower_match_pattern(
        &self,
        pattern: &Expr,
        value_ty: &Type,
        line: usize,
    ) -> Result<Expr, CompileError> {
        match pattern {
            Expr::Call { name, args } if self.variants.contains_key(name) => Ok(Expr::Variant {
                name: name.clone(),
                args: args
                    .iter()
                    .map(|arg| self.lower_match_pattern(arg, value_ty, line))
                    .collect::<Result<Vec<_>, _>>()?,
                field_types: self.instantiate_variant_fields_for_value(name, value_ty, line)?,
            }),
            Expr::NewtypeCtor { name, value } if self.variants.contains_key(name) => {
                Ok(Expr::Variant {
                    name: name.clone(),
                    args: vec![self.lower_match_pattern(value, value_ty, line)?],
                    field_types: self.instantiate_variant_fields_for_value(name, value_ty, line)?,
                })
            }
            Expr::Ident(name)
                if self
                    .variants
                    .get(name)
                    .is_some_and(|variant| variant.fields.is_empty()) =>
            {
                Ok(Expr::Variant {
                    name: name.clone(),
                    args: Vec::new(),
                    field_types: Vec::new(),
                })
            }
            Expr::Tuple(values) => Ok(Expr::Tuple(
                values
                    .iter()
                    .map(|value| self.lower_match_pattern(value, value_ty, line))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            Expr::ArrayPattern { patterns, rest } => Ok(Expr::ArrayPattern {
                patterns: patterns
                    .iter()
                    .map(|value| self.lower_match_pattern(value, value_ty, line))
                    .collect::<Result<Vec<_>, _>>()?,
                rest: rest.clone(),
            }),
            Expr::AliasPattern { pattern, alias } => Ok(Expr::AliasPattern {
                pattern: Box::new(self.lower_match_pattern(pattern, value_ty, line)?),
                alias: alias.clone(),
            }),
            Expr::RecordPattern(fields) => Ok(Expr::RecordPattern(
                fields
                    .iter()
                    .map(|(name, value)| {
                        Ok((
                            name.clone(),
                            value
                                .as_ref()
                                .map(|value| self.lower_match_pattern(value, value_ty, line))
                                .transpose()?,
                        ))
                    })
                    .collect::<Result<Vec<_>, CompileError>>()?,
            )),
            Expr::Some(value) => Ok(Expr::Some(Box::new(
                self.lower_match_pattern(value, value_ty, line)?,
            ))),
            Expr::Ok(value) => Ok(Expr::Ok(Box::new(
                self.lower_match_pattern(value, value_ty, line)?,
            ))),
            Expr::Err(value) => Ok(Expr::Err(Box::new(
                self.lower_match_pattern(value, value_ty, line)?,
            ))),
            other => Ok(other.clone()),
        }
    }

    pub(super) fn check_variant(
        &self,
        name: &str,
        args: &[Expr],
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(variant) = self.variants.get(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variant `{name}`"),
            ));
        };
        if args.len() != variant.fields.len() {
            return Err(CompileError::new(
                line,
                format!(
                    "variant `{name}` expects {} arguments, found {}",
                    variant.fields.len(),
                    args.len()
                ),
            ));
        }
        let mut inferred = HashMap::new();
        for (index, (arg, expected)) in args.iter().zip(&variant.fields).enumerate() {
            let actual = self.check_expr_expected(arg, expected, line)?;
            if !variant.type_params.is_empty() {
                self.infer_generic_type(expected, &actual, arg, &mut inferred)
                    .map_err(|message| {
                        CompileError::new(
                            line,
                            format!("argument {} for variant `{name}`: {message}", index + 1),
                        )
                    })?;
            }
            let concrete_expected = substitute_generics(expected, &inferred);
            if !self.is_assignable(&concrete_expected, &actual, arg) {
                return Err(CompileError::new(
                    line,
                    format!(
                        "argument {} for variant `{name}`: expected {}, found {}",
                        index + 1,
                        concrete_expected.name(),
                        actual.name()
                    ),
                ));
            }
        }
        if variant.type_params.is_empty() {
            return Ok(Type::Named(variant.sum_type.clone()));
        }
        let mut concrete_args = Vec::with_capacity(variant.type_params.len());
        for type_param in &variant.type_params {
            let Some(ty) = inferred.get(type_param).cloned() else {
                return Err(CompileError::new(
                    line,
                    format!("could not infer generic type `{type_param}` for variant `{name}`"),
                ));
            };
            concrete_args.push(ty);
        }
        Ok(Type::Applied(variant.sum_type.clone(), concrete_args))
    }

    pub(super) fn check_variant_expected(
        &self,
        name: &str,
        args: &[Expr],
        expected: &Type,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(variant) = self.variants.get(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variant `{name}`"),
            ));
        };
        let Type::Applied(expected_name, expected_args) = expected else {
            return self.check_variant(name, args, line);
        };
        if expected_name != &variant.sum_type || expected_args.len() != variant.type_params.len() {
            return self.check_variant(name, args, line);
        }
        if args.len() != variant.fields.len() {
            return Err(CompileError::new(
                line,
                format!(
                    "variant `{name}` expects {} arguments, found {}",
                    variant.fields.len(),
                    args.len()
                ),
            ));
        }
        let inferred = variant
            .type_params
            .iter()
            .cloned()
            .zip(expected_args.iter().cloned())
            .collect::<HashMap<_, _>>();
        for (index, (arg, field)) in args.iter().zip(&variant.fields).enumerate() {
            let expected_field = substitute_generics(field, &inferred);
            let actual = self.check_expr_expected(arg, &expected_field, line)?;
            if !self.is_assignable(&expected_field, &actual, arg) {
                return Err(CompileError::new(
                    line,
                    format!(
                        "argument {} for variant `{name}`: expected {}, found {}",
                        index + 1,
                        expected_field.name(),
                        actual.name()
                    ),
                ));
            }
        }
        Ok(expected.clone())
    }

    pub(super) fn instantiate_variant_fields(
        &self,
        name: &str,
        args: &[Expr],
        line: usize,
    ) -> Result<Vec<Type>, CompileError> {
        let Some(variant) = self.variants.get(name) else {
            return Ok(Vec::new());
        };
        if variant.type_params.is_empty() {
            return Ok(variant.fields.clone());
        }
        let mut inferred = HashMap::new();
        for (arg, expected) in args.iter().zip(&variant.fields) {
            let actual = self.check_expr(arg, line)?;
            self.infer_generic_type(expected, &actual, arg, &mut inferred)
                .map_err(|message| {
                    CompileError::new(line, format!("variant `{name}`: {message}"))
                })?;
        }
        Ok(variant
            .fields
            .iter()
            .map(|field| substitute_generics(field, &inferred))
            .collect())
    }

    pub(super) fn instantiate_variant_fields_for_value(
        &self,
        name: &str,
        value_ty: &Type,
        line: usize,
    ) -> Result<Vec<Type>, CompileError> {
        let Some(variant) = self.variants.get(name) else {
            return Ok(Vec::new());
        };
        if variant.type_params.is_empty() {
            return Ok(variant.fields.clone());
        }
        let Type::Applied(actual_name, actual_args) = value_ty else {
            return Ok(variant.fields.clone());
        };
        if actual_name != &variant.sum_type || actual_args.len() != variant.type_params.len() {
            return Err(CompileError::new(
                line,
                format!(
                    "match variant `{name}` belongs to {}, found {}",
                    variant.sum_type,
                    value_ty.name()
                ),
            ));
        }
        let inferred = variant
            .type_params
            .iter()
            .cloned()
            .zip(actual_args.iter().cloned())
            .collect::<HashMap<_, _>>();
        Ok(variant
            .fields
            .iter()
            .map(|field| substitute_generics(field, &inferred))
            .collect())
    }

    pub(super) fn check_match(
        &self,
        value: &Expr,
        arms: &[MatchArm],
        line: usize,
    ) -> Result<Type, CompileError> {
        if arms.is_empty() {
            return Err(CompileError::new(
                line,
                "match expression requires at least one arm".to_string(),
            ));
        }
        let value_ty = if matches!(value, Expr::Command { .. } | Expr::Pipeline { .. }) {
            command_result_type()
        } else {
            self.check_expr(value, line)?
        };
        let mut first_ty = None;
        let mut guard_error = None;
        for arm in arms {
            let mut arm_checker = self.clone();
            if let Some(pattern) = &arm.pattern {
                for (name, ty) in self.check_match_pattern(pattern, value, &value_ty, line)? {
                    arm_checker.define(&name, ty, false, line)?;
                }
            }
            if let Some(guard) = &arm.guard {
                arm_checker.check_condition(guard, line)?;
                if let Expr::MatchGuardResult(value) = guard {
                    let ty = arm_checker.check_expr(value, line)?;
                    let (_, error) =
                        result_types(&ty).expect("MatchGuardResult requires Result value");
                    guard_error = Some(match guard_error {
                        None => error.clone(),
                        Some(Type::Unit) => error.clone(),
                        Some(existing) if *error == Type::Unit => existing,
                        Some(existing)
                            if self.is_assignable(&existing, error, value)
                                && self.is_assignable(error, &existing, value) =>
                        {
                            existing
                        }
                        Some(existing) => {
                            return Err(CompileError::new(
                                line,
                                format!(
                                    "match guard error types must match, found {} and {}",
                                    existing.name(),
                                    error.name()
                                ),
                            ));
                        }
                    });
                }
            }
            let arm_ty = arm_checker.check_expr(&arm.expr, line)?;
            if let Some(expected) = &first_ty {
                if self.is_assignable(expected, &arm_ty, &arm.expr) {
                    continue;
                }
                if self.is_assignable(&arm_ty, expected, &arm.expr) {
                    first_ty = Some(arm_ty);
                } else {
                    return Err(CompileError::new(
                        line,
                        format!(
                            "match arms must have matching types, found {} and {}",
                            expected.name(),
                            arm_ty.name()
                        ),
                    ));
                }
            } else {
                first_ty = Some(arm_ty);
            }
        }
        if !arms
            .iter()
            .any(|arm| arm.pattern.is_none() && arm.guard.is_none())
        {
            match self.missing_match_cases(&value_ty, arms) {
                Some(missing) if missing.is_empty() => {}
                Some(missing) => {
                    return Err(CompileError::new(
                        line,
                        format!(
                            "non-exhaustive match; missing cases: {}",
                            missing.join(", ")
                        ),
                    ));
                }
                None => {
                    return Err(CompileError::new(
                        line,
                        "match expression requires wildcard `_` arm".to_string(),
                    ));
                }
            }
        }
        let first_ty = first_ty.expect("match expression has at least one arm");
        if let Some(guard_error) = guard_error {
            let Some((ok, error)) = result_types(&first_ty) else {
                unreachable!("propagating match guards lift arm results");
            };
            if *error != Type::Unit
                && guard_error != Type::Unit
                && !self.is_assignable(error, &guard_error, value)
            {
                return Err(CompileError::new(
                    line,
                    format!(
                        "match guard error {} is not assignable to arm error {}",
                        guard_error.name(),
                        error.name()
                    ),
                ));
            }
            return Ok(Type::Applied(
                "Result".to_string(),
                vec![
                    ok.clone(),
                    if *error == Type::Unit {
                        guard_error
                    } else {
                        error.clone()
                    },
                ],
            ));
        }
        Ok(first_ty)
    }

    pub(super) fn check_match_pattern(
        &self,
        pattern: &Expr,
        _value: &Expr,
        value_ty: &Type,
        line: usize,
    ) -> Result<Vec<(String, Type)>, CompileError> {
        match pattern {
            Expr::Call { name, args } | Expr::Variant { name, args, .. }
                if self.variants.contains_key(name) =>
            {
                self.check_variant_pattern(name, args, value_ty, line)
            }
            Expr::Ident(name)
                if self
                    .variants
                    .get(name)
                    .is_some_and(|variant| variant.fields.is_empty()) =>
            {
                self.check_variant_pattern(name, &[], value_ty, line)
            }
            Expr::NewtypeCtor { name, value } if self.variants.contains_key(name) => self
                .check_variant_pattern(name, std::slice::from_ref(value.as_ref()), value_ty, line),
            Expr::Some(inner) => {
                let Some(element_ty) = option_element_type(value_ty) else {
                    return Err(match_pattern_mismatch(line, value_ty, pattern));
                };
                self.check_match_constructor_payload(inner, element_ty, line)
            }
            Expr::None => {
                if option_element_type(value_ty).is_some() {
                    Ok(Vec::new())
                } else {
                    Err(match_pattern_mismatch(line, value_ty, pattern))
                }
            }
            Expr::Ok(inner) => {
                let Some((ok_ty, _)) = result_types(value_ty) else {
                    return Err(match_pattern_mismatch(line, value_ty, pattern));
                };
                self.check_match_constructor_payload(inner, ok_ty, line)
            }
            Expr::Err(inner) => {
                let Some((_, err_ty)) = result_types(value_ty) else {
                    return Err(match_pattern_mismatch(line, value_ty, pattern));
                };
                self.check_match_constructor_payload(inner, err_ty, line)
            }
            Expr::Tuple(patterns) => {
                let Type::Tuple(elements) = value_ty else {
                    return Err(match_pattern_mismatch(line, value_ty, pattern));
                };
                if patterns.len() != elements.len() {
                    return Err(CompileError::new(
                        line,
                        format!(
                            "match pattern type mismatch: expected {}, found {}",
                            value_ty.name(),
                            Type::Tuple(
                                patterns
                                    .iter()
                                    .map(|pattern| self
                                        .check_expr(pattern, line)
                                        .unwrap_or(Type::Unit))
                                    .collect()
                            )
                            .name()
                        ),
                    ));
                }
                let mut bindings = Vec::new();
                for (pattern, expected_ty) in patterns.iter().zip(elements) {
                    if matches!(pattern, Expr::Ident(name) if name == "_") {
                        continue;
                    }
                    if let Expr::Ident(name) = pattern {
                        bindings.push((name.clone(), expected_ty.clone()));
                        continue;
                    }
                    let pattern_ty = self.check_expr(pattern, line)?;
                    if !self.is_comparable_by_equality(expected_ty, &pattern_ty)
                        && !self.is_assignable(expected_ty, &pattern_ty, pattern)
                    {
                        return Err(CompileError::new(
                            line,
                            format!(
                                "match pattern type mismatch: expected {}, found {}",
                                expected_ty.name(),
                                pattern_ty.name()
                            ),
                        ));
                    }
                }
                Ok(bindings)
            }
            Expr::ArrayPattern { patterns, rest } => {
                let Type::Array(element_ty) = value_ty else {
                    return Err(CompileError::new(
                        line,
                        format!("array pattern requires Array, found {}", value_ty.name()),
                    ));
                };
                let mut bindings = Vec::new();
                for pattern in patterns {
                    bindings
                        .extend(self.check_match_constructor_payload(pattern, element_ty, line)?);
                }
                if let Some(rest) = rest.as_ref().filter(|name| *name != "_") {
                    bindings.push((rest.clone(), value_ty.clone()));
                }
                Ok(bindings)
            }
            Expr::AliasPattern { pattern, alias } => {
                let mut bindings = self.check_match_pattern(pattern, pattern, value_ty, line)?;
                if alias != "_" {
                    bindings.push((alias.clone(), value_ty.clone()));
                }
                Ok(bindings)
            }
            Expr::RecordPattern(patterns) => {
                let Type::Record(fields) = value_ty else {
                    return Err(match_pattern_mismatch(line, value_ty, pattern));
                };
                let mut bindings = Vec::new();
                for (field, pattern) in patterns {
                    let Some((_, expected_ty)) =
                        fields.iter().find(|(candidate, _)| candidate == field)
                    else {
                        return Err(CompileError::new(
                            line,
                            format!("match record pattern field `{field}` is missing"),
                        ));
                    };
                    match pattern {
                        None => bindings.push((field.clone(), expected_ty.clone())),
                        Some(Expr::Ident(name)) if name == "_" => {}
                        Some(Expr::Ident(name)) => {
                            bindings.push((name.clone(), expected_ty.clone()));
                        }
                        Some(pattern) => {
                            bindings.extend(self.check_match_constructor_payload(
                                pattern,
                                expected_ty,
                                line,
                            )?);
                        }
                    }
                }
                Ok(bindings)
            }
            _ => {
                let pattern_ty = self.check_expr(pattern, line)?;
                if self.is_comparable_by_equality(value_ty, &pattern_ty) {
                    Ok(Vec::new())
                } else {
                    Err(CompileError::new(
                        line,
                        format!(
                            "match pattern type mismatch: expected {}, found {}",
                            value_ty.name(),
                            pattern_ty.name()
                        ),
                    ))
                }
            }
        }
    }

    pub(super) fn missing_match_cases(
        &self,
        value_ty: &Type,
        arms: &[MatchArm],
    ) -> Option<Vec<String>> {
        let covered = arms
            .iter()
            .filter(|arm| arm.guard.is_none())
            .filter_map(|arm| arm.pattern.as_ref())
            .collect::<Vec<_>>();
        let expected = match value_ty {
            Type::Bool => vec!["true".to_string(), "false".to_string()],
            Type::Applied(name, args) if name == "Option" && args.len() == 1 => {
                vec!["Some".to_string(), "None".to_string()]
            }
            Type::Applied(name, args) if name == "Result" && args.len() == 2 => {
                vec!["Ok".to_string(), "Err".to_string()]
            }
            Type::Applied(name, _) => self.sum_types.get(name)?.clone(),
            Type::Named(name) => self.sum_types.get(name)?.clone(),
            _ => return None,
        };
        Some(
            expected
                .into_iter()
                .filter(|case| {
                    !covered
                        .iter()
                        .any(|pattern| self.pattern_covers_case(pattern, case))
                })
                .collect(),
        )
    }

    pub(super) fn pattern_covers_case(&self, pattern: &Expr, case: &str) -> bool {
        match pattern {
            Expr::Bool(true) => case == "true",
            Expr::Bool(false) => case == "false",
            Expr::Some(_) => case == "Some",
            Expr::None => case == "None",
            Expr::Ok(_) => case == "Ok",
            Expr::Err(_) => case == "Err",
            Expr::Call { name, .. }
            | Expr::Variant { name, .. }
            | Expr::NewtypeCtor { name, .. } => name == case,
            Expr::Ident(name)
                if self
                    .variants
                    .get(name)
                    .is_some_and(|variant| variant.fields.is_empty()) =>
            {
                name == case
            }
            _ => false,
        }
    }

    pub(super) fn check_variant_pattern(
        &self,
        name: &str,
        args: &[Expr],
        value_ty: &Type,
        line: usize,
    ) -> Result<Vec<(String, Type)>, CompileError> {
        let Some(variant) = self.variants.get(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variant `{name}`"),
            ));
        };
        let mut inferred = HashMap::new();
        if variant.type_params.is_empty() {
            if value_ty != &Type::Named(variant.sum_type.clone()) {
                return Err(CompileError::new(
                    line,
                    format!(
                        "match variant `{name}` belongs to {}, found {}",
                        variant.sum_type,
                        value_ty.name()
                    ),
                ));
            }
        } else if let Type::Applied(actual_name, actual_args) = value_ty {
            if actual_name != &variant.sum_type || actual_args.len() != variant.type_params.len() {
                return Err(CompileError::new(
                    line,
                    format!(
                        "match variant `{name}` belongs to {}, found {}",
                        variant.sum_type,
                        value_ty.name()
                    ),
                ));
            }
            for (type_param, actual) in variant.type_params.iter().zip(actual_args) {
                inferred.insert(type_param.clone(), actual.clone());
            }
        } else {
            return Err(CompileError::new(
                line,
                format!(
                    "match variant `{name}` belongs to {}, found {}",
                    variant.sum_type,
                    value_ty.name()
                ),
            ));
        }
        if args.len() != variant.fields.len() {
            return Err(CompileError::new(
                line,
                format!(
                    "variant pattern `{name}` expects {} arguments, found {}",
                    variant.fields.len(),
                    args.len()
                ),
            ));
        }
        let mut bindings = Vec::new();
        for (pattern, expected) in args.iter().zip(&variant.fields) {
            let expected = substitute_generics(expected, &inferred);
            if matches!(pattern, Expr::Ident(candidate) if candidate == "_") {
                continue;
            }
            if let Expr::Ident(binding) = pattern {
                bindings.push((binding.clone(), expected.clone()));
                continue;
            }
            let actual = self.check_expr(pattern, line)?;
            if !self.is_comparable_by_equality(&expected, &actual)
                && !self.is_assignable(&expected, &actual, pattern)
            {
                return Err(CompileError::new(
                    line,
                    format!(
                        "match pattern type mismatch: expected {}, found {}",
                        expected.name(),
                        actual.name()
                    ),
                ));
            }
        }
        Ok(bindings)
    }

    pub(super) fn check_match_constructor_payload(
        &self,
        payload: &Expr,
        expected_ty: &Type,
        line: usize,
    ) -> Result<Vec<(String, Type)>, CompileError> {
        if let Expr::Ident(name) = payload {
            return if name == "_" {
                Ok(Vec::new())
            } else {
                Ok(vec![(name.clone(), expected_ty.clone())])
            };
        }
        if matches!(
            payload,
            Expr::Tuple(_)
                | Expr::RecordPattern(_)
                | Expr::ArrayPattern { .. }
                | Expr::AliasPattern { .. }
        ) {
            return self.check_match_pattern(payload, payload, expected_ty, line);
        }
        let payload_ty = self.check_expr(payload, line)?;
        if self.is_comparable_by_equality(expected_ty, &payload_ty)
            || self.is_assignable(expected_ty, &payload_ty, payload)
        {
            Ok(Vec::new())
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "match pattern type mismatch: expected {}, found {}",
                    expected_ty.name(),
                    payload_ty.name()
                ),
            ))
        }
    }
}
