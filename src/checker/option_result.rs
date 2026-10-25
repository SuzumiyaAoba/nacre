use super::type_utils::{default_success_type, option_element_type, result_types};
use super::TypeChecker;
use crate::{CompileError, Expr, Type};

impl TypeChecker {
    pub(super) fn check_option_map(
        &self,
        name: &str,
        mapper: &Expr,
        line: usize,
    ) -> Result<(Type, Type), CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        let Some(element) = option_element_type(&binding.ty) else {
            return Err(CompileError::new(
                line,
                format!("type {} has no map method", binding.ty.name()),
            ));
        };
        if *element == Type::Unit {
            return Err(CompileError::new(
                line,
                "map on None requires a typed Option".to_string(),
            ));
        }
        let mapped = self.check_map_mapper(mapper, element, "map", line)?;
        Ok((element.clone(), mapped))
    }

    pub(super) fn check_option_map_value(
        &self,
        value: &Expr,
        mapper: &Expr,
        line: usize,
    ) -> Result<(Type, Type), CompileError> {
        let value_ty = self.check_expr(value, line)?;
        let Some(element) = option_element_type(&value_ty) else {
            return Err(CompileError::new(
                line,
                format!("type {} has no map method", value_ty.name()),
            ));
        };
        if *element == Type::Unit {
            return Err(CompileError::new(
                line,
                "map on None requires a typed Option".to_string(),
            ));
        }
        let mapped = self.check_map_mapper(mapper, element, "map", line)?;
        Ok((element.clone(), mapped))
    }

    pub(super) fn check_result_map(
        &self,
        name: &str,
        mapper: &Expr,
        line: usize,
    ) -> Result<(Type, Type, Type), CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        let Some((element, error)) = result_types(&binding.ty) else {
            return Err(CompileError::new(
                line,
                format!("type {} has no map method", binding.ty.name()),
            ));
        };
        if *element == Type::Unit {
            return Err(CompileError::new(
                line,
                "map on Err requires a typed Result".to_string(),
            ));
        }
        let mapped = self.check_map_mapper(mapper, element, "map", line)?;
        Ok((element.clone(), error.clone(), mapped))
    }

    pub(super) fn check_result_map_value(
        &self,
        value: &Expr,
        mapper: &Expr,
        line: usize,
    ) -> Result<(Type, Type, Type), CompileError> {
        let value_ty = self.check_expr(value, line)?;
        let Some((element, error)) = result_types(&value_ty) else {
            return Err(CompileError::new(
                line,
                format!("type {} has no map method", value_ty.name()),
            ));
        };
        if *element == Type::Unit {
            return Err(CompileError::new(
                line,
                "map on Err requires a typed Result".to_string(),
            ));
        }
        let mapped = self.check_map_mapper(mapper, element, "map", line)?;
        Ok((element.clone(), error.clone(), mapped))
    }

    pub(super) fn check_option_flat_map(
        &self,
        name: &str,
        mapper: &Expr,
        line: usize,
    ) -> Result<(Type, Type), CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        let Some(element) = option_element_type(&binding.ty) else {
            return Err(CompileError::new(
                line,
                format!("type {} has no flatMap method", binding.ty.name()),
            ));
        };
        if *element == Type::Unit {
            return Err(CompileError::new(
                line,
                "flatMap on None requires a typed Option".to_string(),
            ));
        }
        let mapped = self.check_map_mapper(mapper, element, "flatMap", line)?;
        self.check_flat_map_result(element, mapped, line)
    }

    pub(super) fn check_option_flat_map_value(
        &self,
        value: &Expr,
        mapper: &Expr,
        line: usize,
    ) -> Result<(Type, Type), CompileError> {
        let value_ty = self.check_expr(value, line)?;
        let Some(element) = option_element_type(&value_ty) else {
            return Err(CompileError::new(
                line,
                format!("type {} has no flatMap method", value_ty.name()),
            ));
        };
        if *element == Type::Unit {
            return Err(CompileError::new(
                line,
                "flatMap on None requires a typed Option".to_string(),
            ));
        }
        let mapped = self.check_map_mapper(mapper, element, "flatMap", line)?;
        self.check_flat_map_result(element, mapped, line)
    }

    pub(super) fn check_result_flat_map(
        &self,
        name: &str,
        mapper: &Expr,
        line: usize,
    ) -> Result<(Type, Type, Type), CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        let Some((element, error)) = result_types(&binding.ty) else {
            return Err(CompileError::new(
                line,
                format!("type {} has no flatMap method", binding.ty.name()),
            ));
        };
        if *element == Type::Unit {
            return Err(CompileError::new(
                line,
                "flatMap on Err requires a typed Result".to_string(),
            ));
        }
        let mapped = self.check_map_mapper(mapper, element, "flatMap", line)?;
        self.check_result_flat_map_result(element, error, mapped, mapper, line)
    }

    pub(super) fn check_result_flat_map_value(
        &self,
        value: &Expr,
        mapper: &Expr,
        line: usize,
    ) -> Result<(Type, Type, Type), CompileError> {
        let value_ty = self.check_expr(value, line)?;
        let Some((element, error)) = result_types(&value_ty) else {
            return Err(CompileError::new(
                line,
                format!("type {} has no flatMap method", value_ty.name()),
            ));
        };
        if *element == Type::Unit {
            return Err(CompileError::new(
                line,
                "flatMap on Err requires a typed Result".to_string(),
            ));
        }
        let mapped = self.check_map_mapper(mapper, element, "flatMap", line)?;
        self.check_result_flat_map_result(element, error, mapped, mapper, line)
    }

    pub(super) fn check_result_flat_map_result(
        &self,
        element: &Type,
        error: &Type,
        mapped: Type,
        mapper: &Expr,
        line: usize,
    ) -> Result<(Type, Type, Type), CompileError> {
        let Some((mapped_element, mapped_error)) = result_types(&mapped) else {
            return Err(CompileError::new(
                line,
                format!("flatMap mapper must return Result, found {}", mapped.name()),
            ));
        };
        let mapped_element = mapped_element.clone();
        let result_error = if *error == Type::Unit {
            mapped_error.clone()
        } else if *mapped_error == Type::Unit || self.is_assignable(error, mapped_error, mapper) {
            error.clone()
        } else {
            return Err(CompileError::new(
                line,
                format!(
                    "flatMap mapper error must be assignable to {}, found {}",
                    error.name(),
                    mapped_error.name()
                ),
            ));
        };
        Ok((
            element.clone(),
            mapped,
            Type::Applied("Result".to_string(), vec![mapped_element, result_error]),
        ))
    }

    pub(super) fn check_option_ap(
        &self,
        name: &str,
        value: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        let value_ty = self.check_expr(value, line)?;
        self.check_option_ap_types(&binding.ty, &value_ty, value, line)
    }

    pub(super) fn check_option_ap_value(
        &self,
        function: &Expr,
        value: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let function_ty = self.check_expr(function, line)?;
        let value_ty = self.check_expr(value, line)?;
        self.check_option_ap_types(&function_ty, &value_ty, value, line)
    }

    pub(super) fn check_option_ap_types(
        &self,
        function_ty: &Type,
        value_ty: &Type,
        value: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(function) = option_element_type(function_ty) else {
            return Err(CompileError::new(
                line,
                format!("type {} has no ap method", function_ty.name()),
            ));
        };
        if *function == Type::Unit {
            return Err(CompileError::new(
                line,
                "ap on None requires a typed Option function".to_string(),
            ));
        }
        let Type::Function(params, return_type) = function else {
            return Err(CompileError::new(
                line,
                format!(
                    "ap receiver must contain a function, found {}",
                    function.name()
                ),
            ));
        };
        if params.len() != 1 {
            return Err(CompileError::new(
                line,
                format!("ap function expects 1 parameter, found {}", params.len()),
            ));
        }
        let Some(element) = option_element_type(value_ty) else {
            return Err(CompileError::new(
                line,
                format!("ap argument must be Option, found {}", value_ty.name()),
            ));
        };
        if *element != Type::Unit && !self.is_assignable(&params[0], element, value) {
            return Err(CompileError::new(
                line,
                format!(
                    "ap argument must contain {}, found {}",
                    params[0].name(),
                    element.name()
                ),
            ));
        }
        Ok(Type::Applied(
            "Option".to_string(),
            vec![(**return_type).clone()],
        ))
    }

    pub(super) fn check_result_ap(
        &self,
        name: &str,
        value: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        let value_ty = self.check_expr(value, line)?;
        self.check_result_ap_types(&binding.ty, &value_ty, value, line)
    }

    pub(super) fn check_result_ap_value(
        &self,
        function: &Expr,
        value: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let function_ty = self.check_expr(function, line)?;
        let value_ty = self.check_expr(value, line)?;
        self.check_result_ap_types(&function_ty, &value_ty, value, line)
    }

    pub(super) fn check_result_ap_types(
        &self,
        function_ty: &Type,
        value_ty: &Type,
        value: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some((function, function_error)) = result_types(function_ty) else {
            return Err(CompileError::new(
                line,
                format!("type {} has no ap method", function_ty.name()),
            ));
        };
        if *function == Type::Unit {
            return Err(CompileError::new(
                line,
                "ap on Err requires a typed Result function".to_string(),
            ));
        }
        let Type::Function(params, return_type) = function else {
            return Err(CompileError::new(
                line,
                format!(
                    "ap receiver must contain a function, found {}",
                    function.name()
                ),
            ));
        };
        if params.len() != 1 {
            return Err(CompileError::new(
                line,
                format!("ap function expects 1 parameter, found {}", params.len()),
            ));
        }
        let Some((element, value_error)) = result_types(value_ty) else {
            return Err(CompileError::new(
                line,
                format!("ap argument must be Result, found {}", value_ty.name()),
            ));
        };
        if *element != Type::Unit && !self.is_assignable(&params[0], element, value) {
            return Err(CompileError::new(
                line,
                format!(
                    "ap argument must contain {}, found {}",
                    params[0].name(),
                    element.name()
                ),
            ));
        }
        let error = if *function_error == Type::Unit {
            value_error.clone()
        } else if *value_error == Type::Unit
            || self.is_assignable(function_error, value_error, value)
        {
            function_error.clone()
        } else {
            return Err(CompileError::new(
                line,
                format!(
                    "ap argument error must be assignable to {}, found {}",
                    function_error.name(),
                    value_error.name()
                ),
            ));
        };
        Ok(Type::Applied(
            "Result".to_string(),
            vec![(**return_type).clone(), error],
        ))
    }

    pub(super) fn check_option_or_else(
        &self,
        name: &str,
        fallback: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        let fallback_ty = self.check_expr(fallback, line)?;
        self.check_option_or_else_types(&binding.ty, &fallback_ty, fallback, line)
    }

    pub(super) fn check_option_or_else_value(
        &self,
        value: &Expr,
        fallback: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, line)?;
        let fallback_ty = self.check_expr(fallback, line)?;
        self.check_option_or_else_types(&value_ty, &fallback_ty, fallback, line)
    }

    pub(super) fn check_option_or_else_try(
        &self,
        value: &Expr,
        fallback: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, line)?;
        let Some(value_element) = option_element_type(&value_ty) else {
            return Err(CompileError::new(
                line,
                format!("type {} has no orElse method", value_ty.name()),
            ));
        };
        let fallback_ty = self.check_expr(fallback, line)?;
        let Some((fallback_ok, fallback_error)) = result_types(&fallback_ty) else {
            return Err(CompileError::new(
                line,
                format!(
                    "propagating orElse fallback must return Result, found {}",
                    fallback_ty.name()
                ),
            ));
        };
        let Some(fallback_element) = option_element_type(fallback_ok) else {
            return Err(CompileError::new(
                line,
                format!(
                    "propagating orElse fallback must return Option, found {}",
                    fallback_ok.name()
                ),
            ));
        };
        if *value_element != Type::Unit
            && *fallback_element != Type::Unit
            && !self.is_assignable(value_element, fallback_element, fallback)
        {
            return Err(CompileError::new(
                line,
                format!(
                    "orElse fallback element mismatch: expected {}, found {}",
                    value_element.name(),
                    fallback_element.name()
                ),
            ));
        }
        let element = if *value_element == Type::Unit {
            fallback_element.clone()
        } else {
            value_element.clone()
        };
        Ok(Type::Applied(
            "Result".to_string(),
            vec![
                Type::Applied("Option".to_string(), vec![element]),
                fallback_error.clone(),
            ],
        ))
    }

    pub(super) fn check_option_or_else_types(
        &self,
        value_ty: &Type,
        fallback_ty: &Type,
        fallback: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(value_element) = option_element_type(value_ty) else {
            return Err(CompileError::new(
                line,
                format!("type {} has no orElse method", value_ty.name()),
            ));
        };
        let Some(fallback_element) = option_element_type(fallback_ty) else {
            return Err(CompileError::new(
                line,
                format!(
                    "orElse fallback must be Option, found {}",
                    fallback_ty.name()
                ),
            ));
        };
        if *value_element == Type::Unit && *fallback_element == Type::Unit {
            return Err(CompileError::new(
                line,
                "orElse with two None values requires a typed Option".to_string(),
            ));
        }
        if *value_element == Type::Unit {
            return Ok(fallback_ty.clone());
        }
        if *fallback_element == Type::Unit
            || self.is_assignable(value_element, fallback_element, fallback)
        {
            return Ok(value_ty.clone());
        }
        Err(CompileError::new(
            line,
            format!(
                "orElse fallback mismatch: expected {}, found {}",
                value_ty.name(),
                fallback_ty.name()
            ),
        ))
    }

    pub(super) fn check_flat_map_result(
        &self,
        element: &Type,
        mapped: Type,
        line: usize,
    ) -> Result<(Type, Type), CompileError> {
        let Some(mapped_element) = option_element_type(&mapped) else {
            return Err(CompileError::new(
                line,
                format!("flatMap mapper must return Option, found {}", mapped.name()),
            ));
        };
        if *mapped_element == Type::Unit {
            return Err(CompileError::new(
                line,
                "flatMap mapper returning None requires a typed Option".to_string(),
            ));
        }
        Ok((element.clone(), mapped))
    }

    pub(super) fn check_default(
        &self,
        value: &Expr,
        fallback: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, line)?;
        if matches!(value, Expr::Command { .. } | Expr::Pipeline { .. }) {
            let fallback_ty = self.check_expr(fallback, line)?;
            return if self.is_assignable(&Type::String, &fallback_ty, fallback) {
                Ok(Type::String)
            } else {
                Err(CompileError::new(
                    line,
                    format!(
                        "operator `??` fallback mismatch: expected String, found {}",
                        fallback_ty.name()
                    ),
                ))
            };
        }
        let Some(element_ty) = default_success_type(&value_ty) else {
            return Err(CompileError::new(
                line,
                format!(
                    "operator `??` requires Option or Result value, found {}",
                    value_ty.name()
                ),
            ));
        };
        let fallback_ty = self.check_expr(fallback, line)?;
        if self.is_assignable(element_ty, &fallback_ty, fallback) {
            Ok(element_ty.clone())
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "operator `??` fallback mismatch: expected {}, found {}",
                    element_ty.name(),
                    fallback_ty.name()
                ),
            ))
        }
    }

    pub(super) fn check_default_try(
        &self,
        value: &Expr,
        fallback: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let success_ty = if matches!(value, Expr::Command { .. } | Expr::Pipeline { .. }) {
            Type::String
        } else {
            let value_ty = self.check_expr(value, line)?;
            let Some(success_ty) = default_success_type(&value_ty) else {
                return Err(CompileError::new(
                    line,
                    format!(
                        "operator `??` requires Option or Result value, found {}",
                        value_ty.name()
                    ),
                ));
            };
            success_ty.clone()
        };
        let fallback_ty = self.check_expr(fallback, line)?;
        let Some((fallback_ok, fallback_error)) = result_types(&fallback_ty) else {
            return Err(CompileError::new(
                line,
                format!(
                    "propagating `??` fallback must return Result, found {}",
                    fallback_ty.name()
                ),
            ));
        };
        if !self.is_assignable(&success_ty, fallback_ok, fallback) {
            return Err(CompileError::new(
                line,
                format!(
                    "operator `??` fallback mismatch: expected {}, found {}",
                    success_ty.name(),
                    fallback_ok.name()
                ),
            ));
        }
        Ok(Type::Applied(
            "Result".to_string(),
            vec![success_ty, fallback_error.clone()],
        ))
    }

    pub(super) fn check_result_option(
        &self,
        value: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        if matches!(value, Expr::Command { .. } | Expr::Pipeline { .. }) {
            return Ok(Type::Applied("Option".to_string(), vec![Type::String]));
        }
        let value_ty = self.check_expr(value, line)?;
        let Some((ok_ty, _)) = result_types(&value_ty) else {
            return Err(CompileError::new(
                line,
                format!(
                    "operator `?` requires Result value, found {}",
                    value_ty.name()
                ),
            ));
        };
        Ok(Type::Applied("Option".to_string(), vec![ok_ty.clone()]))
    }

    pub(super) fn check_value_access(&self, name: &str, line: usize) -> Result<Type, CompileError> {
        let Some(binding) = self.bindings.get(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Brand { base, .. } => Ok((**base).clone()),
            other => Err(CompileError::new(
                line,
                format!(
                    "cannot access `.value` on `{name}` of type {}",
                    other.name()
                ),
            )),
        }
    }
}
