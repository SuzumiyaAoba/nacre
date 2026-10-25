use super::names::{associated_const_name, associated_method_name, impl_method_name};
use super::type_utils::substitute_generics;
use super::{FunctionDecl, TraitSig, TypeChecker, VariantSig};
use crate::{CompileError, Expr, ImplConst, ImplMethod, Param, TraitMethod, Type, VariantDecl};
use std::collections::{HashMap, HashSet};

impl TypeChecker {
    pub(super) fn define_newtype(
        &mut self,
        name: &str,
        type_params: &[String],
        base: &Type,
        line: usize,
    ) -> Result<(), CompileError> {
        let generic_names = type_params.iter().cloned().collect::<HashSet<_>>();
        let resolved_base = self.resolve_type_with_generics(base, &generic_names, line)?;
        if self.type_name_exists(name) {
            return Err(CompileError::new(
                line,
                format!("type `{name}` is already defined"),
            ));
        }
        if type_params.is_empty() {
            self.types.insert(
                name.to_string(),
                Type::Brand {
                    name: name.to_string(),
                    base: Box::new(resolved_base),
                },
            );
        } else {
            self.generic_newtypes
                .insert(name.to_string(), (type_params.to_vec(), resolved_base));
        }
        Ok(())
    }

    pub(super) fn define_sum_type(
        &mut self,
        name: &str,
        type_params: &[String],
        variants: &[VariantDecl],
        line: usize,
    ) -> Result<(), CompileError> {
        if self.type_name_exists(name) {
            return Err(CompileError::new(
                line,
                format!("type `{name}` is already defined"),
            ));
        }
        if variants.is_empty() {
            return Err(CompileError::new(
                line,
                format!("sum type `{name}` requires at least one variant"),
            ));
        }
        let mut names = Vec::new();
        let mut seen = HashSet::new();
        let mut resolved_variants = Vec::new();
        let generic_names = type_params.iter().cloned().collect::<HashSet<_>>();
        for variant in variants {
            if !seen.insert(variant.name.clone()) {
                return Err(CompileError::new(
                    line,
                    format!("variant `{}` is already defined", variant.name),
                ));
            }
            if self.variants.contains_key(&variant.name)
                || self.functions.contains_key(&variant.name)
                || self.bindings.contains_key(&variant.name)
            {
                return Err(CompileError::new(
                    line,
                    format!(
                        "variant `{}` conflicts with an existing declaration",
                        variant.name
                    ),
                ));
            }
            let fields = variant
                .fields
                .iter()
                .map(|field| self.resolve_type_with_generics(field, &generic_names, line))
                .collect::<Result<Vec<_>, _>>()?;
            names.push(variant.name.clone());
            resolved_variants.push((variant.name.clone(), fields));
        }
        if type_params.is_empty() {
            self.types
                .insert(name.to_string(), Type::Named(name.to_string()));
        } else {
            self.generic_sum_types
                .insert(name.to_string(), type_params.to_vec());
        }
        self.sum_types.insert(name.to_string(), names);
        for (variant, fields) in resolved_variants {
            self.variants.insert(
                variant,
                VariantSig {
                    sum_type: name.to_string(),
                    type_params: type_params.to_vec(),
                    fields,
                },
            );
        }
        Ok(())
    }

    pub(super) fn type_name_exists(&self, name: &str) -> bool {
        self.types.contains_key(name)
            || self.generic_types.contains_key(name)
            || self.generic_newtypes.contains_key(name)
            || self.generic_sum_types.contains_key(name)
    }

    pub(super) fn define_trait(
        &mut self,
        name: &str,
        type_param: &str,
        methods: &[TraitMethod],
        line: usize,
    ) -> Result<(), CompileError> {
        if self.traits.contains_key(name) {
            return Err(CompileError::new(
                line,
                format!("trait `{name}` is already defined"),
            ));
        }
        let mut seen_methods = HashSet::new();
        let generic_names = HashSet::from([type_param.to_string()]);
        let mut resolved_methods = Vec::new();
        for method in methods {
            if !seen_methods.insert(method.name.clone()) {
                return Err(CompileError::new(
                    line,
                    format!("trait method `{}` is already defined", method.name),
                ));
            }
            if method.params.is_empty() {
                return Err(CompileError::new(
                    line,
                    format!(
                        "trait method `{}` requires a receiver parameter",
                        method.name
                    ),
                ));
            }
            let receiver_ty =
                self.resolve_type_with_generics(&method.params[0].ty, &generic_names, line)?;
            if receiver_ty != Type::Generic(type_param.to_string()) {
                return Err(CompileError::new(
                    line,
                    format!(
                        "trait method `{}` receiver must be `{type_param}`",
                        method.name
                    ),
                ));
            }
            for param in &method.params {
                if param.variadic && param.name != method.params.last().unwrap().name {
                    return Err(CompileError::new(
                        line,
                        format!("trait method `{}` rest parameter must be last", method.name),
                    ));
                }
                if param.default.is_some() {
                    return Err(CompileError::new(
                        line,
                        format!(
                            "trait method `{}` cannot use default parameters",
                            method.name
                        ),
                    ));
                }
                self.resolve_type_with_generics(&param.ty, &generic_names, line)?;
            }
            self.resolve_type_with_generics(&method.return_type, &generic_names, line)?;
            let params = method
                .params
                .iter()
                .map(|param| {
                    Ok(Param {
                        name: param.name.clone(),
                        ty: self.resolve_type_with_generics(&param.ty, &generic_names, line)?,
                        default: None,
                        variadic: param.variadic,
                        capture_name: param.capture_name.clone(),
                    })
                })
                .collect::<Result<Vec<_>, CompileError>>()?;
            let return_type =
                self.resolve_type_with_generics(&method.return_type, &generic_names, line)?;
            resolved_methods.push(TraitMethod {
                name: method.name.clone(),
                params,
                return_type,
            });
        }
        self.traits.insert(
            name.to_string(),
            TraitSig {
                type_param: type_param.to_string(),
                methods: resolved_methods,
            },
        );
        Ok(())
    }

    pub(super) fn define_trait_impl(
        &mut self,
        trait_name: &str,
        for_type: &Type,
        methods: &[ImplMethod],
        line: usize,
    ) -> Result<Vec<ImplMethod>, CompileError> {
        let Some(trait_sig) = self.traits.get(trait_name).cloned() else {
            return Err(CompileError::new(
                line,
                format!("unknown trait `{trait_name}`"),
            ));
        };
        let resolved = self.resolve_type(for_type, line)?;
        let key = (trait_name.to_string(), resolved.name());
        if !self.trait_impls.insert(key) {
            return Err(CompileError::new(
                line,
                format!(
                    "trait `{trait_name}` is already implemented for {}",
                    resolved.name()
                ),
            ));
        }
        self.check_impl_methods(trait_name, &trait_sig, &resolved, methods, line)
    }

    pub(super) fn define_inherent_impl(
        &mut self,
        for_type: &Type,
        consts: &[ImplConst],
        methods: &[ImplMethod],
        line: usize,
    ) -> Result<(Vec<ImplConst>, Vec<ImplMethod>), CompileError> {
        let resolved = self.resolve_type(for_type, line)?;
        let type_name = resolved.name();
        let mut seen_consts = HashSet::new();
        let mut seen_methods = HashSet::new();
        let mut lowered_consts = Vec::new();
        for value in consts {
            if !seen_consts.insert(value.name.clone()) {
                return Err(CompileError::new(
                    line,
                    format!("associated const `{}` is already defined", value.name),
                ));
            }
            let lowered_name = associated_const_name(&type_name, &value.name);
            if self.bindings.contains_key(&lowered_name)
                || self.functions.contains_key(&lowered_name)
            {
                return Err(CompileError::new(
                    line,
                    format!(
                        "associated const `{}` conflicts with an existing declaration",
                        value.name
                    ),
                ));
            }
            let actual = self.binding_expr_type(value.annotation.as_ref(), &value.expr, line)?;
            let binding_ty =
                self.check_annotation(value.annotation.clone(), actual, &value.expr, line)?;
            let expr = self.lower_expr_expected(&value.expr, &binding_ty, line)?;
            self.define(&lowered_name, binding_ty, false, line)?;
            lowered_consts.push(ImplConst {
                name: lowered_name,
                annotation: value.annotation.clone(),
                expr,
            });
        }
        let mut lowered_methods = Vec::new();
        for method in methods {
            if !seen_methods.insert(method.name.clone()) {
                return Err(CompileError::new(
                    line,
                    format!("inherent method `{}` is already defined", method.name),
                ));
            }
            let lowered_name = associated_method_name(&type_name, &method.name);
            if self.functions.contains_key(&lowered_name)
                || self.bindings.contains_key(&lowered_name)
            {
                return Err(CompileError::new(
                    line,
                    format!(
                        "associated function `{}` conflicts with an existing declaration",
                        method.name
                    ),
                ));
            }
            self.check_function(
                FunctionDecl {
                    name: &lowered_name,
                    override_constructor: false,
                    type_params: &[],
                    params: &method.params,
                    return_type: &method.return_type,
                    body: &method.body,
                },
                line,
            )?;
            if let Some(receiver) = method.params.first() {
                let receiver_ty = self.resolve_type(&receiver.ty, line)?;
                if self.is_assignable(&resolved, &receiver_ty, &Expr::Unit)
                    || self.is_assignable(&receiver_ty, &resolved, &Expr::Unit)
                {
                    self.inherent_methods
                        .insert((method.name.clone(), resolved.name()), lowered_name.clone());
                }
            }
            let body = self.lower_function_body(
                &lowered_name,
                &[],
                &method.params,
                &method.return_type,
                &method.body,
                line,
            )?;
            lowered_methods.push(ImplMethod {
                name: lowered_name,
                params: method.params.clone(),
                return_type: method.return_type.clone(),
                body,
            });
        }
        Ok((lowered_consts, lowered_methods))
    }

    pub(super) fn check_impl_methods(
        &mut self,
        trait_name: &str,
        trait_sig: &TraitSig,
        for_type: &Type,
        methods: &[ImplMethod],
        line: usize,
    ) -> Result<Vec<ImplMethod>, CompileError> {
        let mut seen_methods = HashSet::new();
        for method in methods {
            if !seen_methods.insert(method.name.clone()) {
                return Err(CompileError::new(
                    line,
                    format!("impl method `{}` is already defined", method.name),
                ));
            }
            let Some(expected) = trait_sig
                .methods
                .iter()
                .find(|candidate| candidate.name == method.name)
            else {
                return Err(CompileError::new(
                    line,
                    format!(
                        "impl method `{}` is not declared by trait `{trait_name}`",
                        method.name
                    ),
                ));
            };
            self.check_impl_method_signature(
                expected,
                method,
                &trait_sig.type_param,
                for_type,
                line,
            )?;
        }
        for expected in &trait_sig.methods {
            if !seen_methods.contains(&expected.name) {
                return Err(CompileError::new(
                    line,
                    format!(
                        "impl for trait `{trait_name}` is missing method `{}`",
                        expected.name
                    ),
                ));
            }
        }
        for method in methods {
            let lowered_name = impl_method_name(trait_name, for_type, &method.name);
            let key = (method.name.clone(), for_type.name());
            self.method_impls
                .entry(key)
                .or_default()
                .push((trait_name.to_string(), lowered_name.clone()));
            let params = &method.params;
            let return_type = &method.return_type;
            let body = &method.body;
            self.check_function(
                FunctionDecl {
                    name: &lowered_name,
                    override_constructor: false,
                    type_params: &[],
                    params,
                    return_type,
                    body,
                },
                line,
            )?;
        }
        methods
            .iter()
            .map(|method| {
                let lowered_name = impl_method_name(trait_name, for_type, &method.name);
                let params = &method.params;
                let return_type = &method.return_type;
                let method_body = &method.body;
                let body = self.lower_function_body(
                    &lowered_name,
                    &[],
                    params,
                    return_type,
                    method_body,
                    line,
                )?;
                Ok(ImplMethod {
                    name: lowered_name,
                    params: method.params.clone(),
                    return_type: method.return_type.clone(),
                    body,
                })
            })
            .collect()
    }

    pub(super) fn check_impl_method_signature(
        &self,
        expected: &TraitMethod,
        actual: &ImplMethod,
        type_param: &str,
        for_type: &Type,
        line: usize,
    ) -> Result<(), CompileError> {
        if expected.params.len() != actual.params.len() {
            return Err(CompileError::new(
                line,
                format!(
                    "impl method `{}` parameter count mismatch: expected {}, found {}",
                    actual.name,
                    expected.params.len(),
                    actual.params.len()
                ),
            ));
        }
        let inferred = HashMap::from([(type_param.to_string(), for_type.clone())]);
        for (expected, actual_param) in expected.params.iter().zip(&actual.params) {
            if expected.variadic != actual_param.variadic {
                return Err(CompileError::new(
                    line,
                    format!(
                        "impl method `{}` parameter `{}` rest modifier mismatch",
                        actual.name, actual_param.name
                    ),
                ));
            }
            let expected_ty = substitute_generics(&expected.ty, &inferred);
            let actual_ty = self.resolve_type(&actual_param.ty, line)?;
            if expected_ty != actual_ty {
                return Err(CompileError::new(
                    line,
                    format!(
                        "impl method `{}` parameter `{}` type mismatch: expected {}, found {}",
                        actual.name,
                        actual_param.name,
                        expected_ty.name(),
                        actual_ty.name()
                    ),
                ));
            }
        }
        let expected_return = substitute_generics(&expected.return_type, &inferred);
        let actual_return = self.resolve_type(&actual.return_type, line)?;
        if expected_return != actual_return {
            return Err(CompileError::new(
                line,
                format!(
                    "impl method `{}` return type mismatch: expected {}, found {}",
                    actual.name,
                    expected_return.name(),
                    actual_return.name()
                ),
            ));
        }
        Ok(())
    }

    pub(super) fn define_type_alias(
        &mut self,
        name: &str,
        type_params: &[String],
        ty: &Type,
        line: usize,
    ) -> Result<(), CompileError> {
        let generic_names = type_params.iter().cloned().collect::<HashSet<_>>();
        let resolved_ty = self.resolve_type_with_generics(ty, &generic_names, line)?;
        if self.type_name_exists(name) {
            return Err(CompileError::new(
                line,
                format!("type `{name}` is already defined"),
            ));
        }
        if type_params.is_empty() {
            self.types.insert(name.to_string(), resolved_ty);
        } else {
            self.generic_types
                .insert(name.to_string(), (type_params.to_vec(), resolved_ty));
        }
        Ok(())
    }
}
