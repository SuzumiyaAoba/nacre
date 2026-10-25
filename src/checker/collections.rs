use super::names::{associated_const_name, module_binding_name};
use super::TypeChecker;
use crate::{CompileError, Expr, Type};

impl TypeChecker {
    pub(super) fn check_array(&self, values: &[Expr], line: usize) -> Result<Type, CompileError> {
        let Some(first) = values.first() else {
            return Ok(Type::Array(Box::new(Type::Unit)));
        };
        let element_ty = self.check_expr(first, line)?;
        for value in &values[1..] {
            let value_ty = self.check_expr(value, line)?;
            if !self.is_assignable(&element_ty, &value_ty, value)
                || !self.is_assignable(&value_ty, &element_ty, first)
            {
                return Err(CompileError::new(
                    line,
                    format!(
                        "array elements must have matching types, found {} and {}",
                        element_ty.name(),
                        value_ty.name()
                    ),
                ));
            }
        }
        Ok(Type::Array(Box::new(element_ty)))
    }

    pub(super) fn check_json_parse(&self, value: &Expr, line: usize) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, line)?;
        if matches!(value_ty, Type::String | Type::Path) {
            Ok(Type::Map(Box::new(Type::String), Box::new(Type::String)))
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "json.parse value must be String or Path, found {}",
                    value_ty.name()
                ),
            ))
        }
    }

    pub(super) fn check_json_stringify(
        &self,
        name: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Map(key, value) if **key == Type::String && **value == Type::String => {
                Ok(Type::String)
            }
            other => Err(CompileError::new(
                line,
                format!(
                    "json.stringify expects Map[String, String], found {}",
                    other.name()
                ),
            )),
        }
    }

    pub(super) fn check_json_stringify_value(
        &self,
        value: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        if !matches!(value, Expr::Map(_) | Expr::JsonParse { .. }) {
            return Err(CompileError::new(
                line,
                "json.stringify value must be a map literal or json.parse result".to_string(),
            ));
        }
        match self.check_expr(value, line)? {
            Type::Map(key, value) if *key == Type::String && *value == Type::String => {
                Ok(Type::String)
            }
            other => Err(CompileError::new(
                line,
                format!(
                    "json.stringify expects Map[String, String], found {}",
                    other.name()
                ),
            )),
        }
    }

    pub(super) fn check_map(
        &self,
        entries: &[(Expr, Expr)],
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some((first_key, first_value)) = entries.first() else {
            return Ok(Type::Map(Box::new(Type::Unit), Box::new(Type::Unit)));
        };
        let key_ty = self.check_expr(first_key, line)?;
        let value_ty = self.check_expr(first_value, line)?;
        for (key, value) in &entries[1..] {
            let next_key_ty = self.check_expr(key, line)?;
            let next_value_ty = self.check_expr(value, line)?;
            if !self.is_assignable(&key_ty, &next_key_ty, key)
                || !self.is_assignable(&next_key_ty, &key_ty, first_key)
            {
                return Err(CompileError::new(
                    line,
                    format!(
                        "map keys must have matching types, found {} and {}",
                        key_ty.name(),
                        next_key_ty.name()
                    ),
                ));
            }
            if !self.is_assignable(&value_ty, &next_value_ty, value)
                || !self.is_assignable(&next_value_ty, &value_ty, first_value)
            {
                return Err(CompileError::new(
                    line,
                    format!(
                        "map values must have matching types, found {} and {}",
                        value_ty.name(),
                        next_value_ty.name()
                    ),
                ));
            }
        }
        Ok(Type::Map(Box::new(key_ty), Box::new(value_ty)))
    }

    pub(super) fn check_record(
        &self,
        fields: &[(String, Expr)],
        line: usize,
    ) -> Result<Type, CompileError> {
        let mut typed_fields = Vec::new();
        for (name, value) in fields {
            if typed_fields.iter().any(|(existing, _)| existing == name) {
                return Err(CompileError::new(
                    line,
                    format!("record field `{name}` is already defined"),
                ));
            }
            typed_fields.push((name.clone(), self.check_expr(value, line)?));
        }
        Ok(Type::Record(typed_fields))
    }

    pub(super) fn check_tuple(&self, values: &[Expr], line: usize) -> Result<Type, CompileError> {
        if values.len() < 2 {
            return Err(CompileError::new(
                line,
                "tuple literal requires at least two elements".to_string(),
            ));
        }
        let mut elements = Vec::new();
        for value in values {
            elements.push(self.check_expr(value, line)?);
        }
        Ok(Type::Tuple(elements))
    }

    pub(super) fn check_index(
        &self,
        name: &str,
        index: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let index_ty = self.check_expr(index, line)?;
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Array(element) => {
                if self.is_integer_numeric(&index_ty) {
                    Ok((**element).clone())
                } else {
                    Err(CompileError::new(
                        line,
                        format!("array index must be Int, found {}", index_ty.name()),
                    ))
                }
            }
            Type::Map(key, value) => {
                if self.is_assignable(key, &index_ty, index) {
                    Ok((**value).clone())
                } else {
                    Err(CompileError::new(
                        line,
                        format!("map key must be {}, found {}", key.name(), index_ty.name()),
                    ))
                }
            }
            other => Err(CompileError::new(
                line,
                format!("cannot index `{name}` of type {}", other.name()),
            )),
        }
    }

    pub(super) fn check_index_value(
        &self,
        value: &Expr,
        index: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let index_ty = self.check_expr(index, line)?;
        let value_ty = self.check_expr(value, line)?;
        match value_ty {
            Type::Array(element) => {
                if !matches!(value, Expr::Array(_)) {
                    return Err(CompileError::new(
                        line,
                        "index value must be an array literal or named array".to_string(),
                    ));
                }
                if self.is_integer_numeric(&index_ty) {
                    Ok(*element)
                } else {
                    Err(CompileError::new(
                        line,
                        format!("array index must be Int, found {}", index_ty.name()),
                    ))
                }
            }
            Type::Map(key, map_value) => {
                if !matches!(value, Expr::Map(_)) {
                    return Err(CompileError::new(
                        line,
                        "index value must be a map literal or named map".to_string(),
                    ));
                }
                if self.is_assignable(&key, &index_ty, index) {
                    Ok(*map_value)
                } else {
                    Err(CompileError::new(
                        line,
                        format!("map key must be {}, found {}", key.name(), index_ty.name()),
                    ))
                }
            }
            other => Err(CompileError::new(
                line,
                format!("cannot index value of type {}", other.name()),
            )),
        }
    }

    pub(super) fn check_slice(
        &self,
        name: &str,
        start: &Expr,
        end: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let start_ty = self.check_expr(start, line)?;
        if !self.is_integer_numeric(&start_ty) {
            return Err(CompileError::new(
                line,
                format!("slice start must be Int, found {}", start_ty.name()),
            ));
        }
        let end_ty = self.check_expr(end, line)?;
        if !self.is_integer_numeric(&end_ty) {
            return Err(CompileError::new(
                line,
                format!("slice end must be Int, found {}", end_ty.name()),
            ));
        }
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Array(element) => Ok(Type::Array(element.clone())),
            ty if self.is_string_like(ty) => Ok(Type::String),
            other => Err(CompileError::new(
                line,
                format!("type {} has no slice method", other.name()),
            )),
        }
    }

    pub(super) fn check_string_slice_value(
        &self,
        value: &Expr,
        start: &Expr,
        end: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let start_ty = self.check_expr(start, line)?;
        if !self.is_integer_numeric(&start_ty) {
            return Err(CompileError::new(
                line,
                format!("slice start must be Int, found {}", start_ty.name()),
            ));
        }
        let end_ty = self.check_expr(end, line)?;
        if !self.is_integer_numeric(&end_ty) {
            return Err(CompileError::new(
                line,
                format!("slice end must be Int, found {}", end_ty.name()),
            ));
        }
        let value_ty = self.check_expr(value, line)?;
        if self.is_string_like(&value_ty) {
            Ok(Type::String)
        } else {
            Err(CompileError::new(
                line,
                format!("type {} has no slice method", value_ty.name()),
            ))
        }
    }

    pub(super) fn check_array_slice_value(
        &self,
        value: &Expr,
        start: &Expr,
        end: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let start_ty = self.check_expr(start, line)?;
        if !self.is_integer_numeric(&start_ty) {
            return Err(CompileError::new(
                line,
                format!("slice start must be Int, found {}", start_ty.name()),
            ));
        }
        let end_ty = self.check_expr(end, line)?;
        if !self.is_integer_numeric(&end_ty) {
            return Err(CompileError::new(
                line,
                format!("slice end must be Int, found {}", end_ty.name()),
            ));
        }
        self.check_array_value(value, "slice", line)
    }

    pub(super) fn lower_len(&self, name: &str) -> Expr {
        self.lookup_binding(name)
            .filter(|binding| self.is_string_like(&binding.ty))
            .map(|_| Expr::StringLen(name.to_string()))
            .unwrap_or_else(|| Expr::Len(name.to_string()))
    }

    pub(super) fn lower_is_empty(&self, name: &str) -> Expr {
        self.lookup_binding(name)
            .filter(|binding| self.is_string_like(&binding.ty))
            .map(|_| Expr::StringIsEmpty(name.to_string()))
            .unwrap_or_else(|| Expr::IsEmpty(name.to_string()))
    }

    pub(super) fn check_len(&self, name: &str, line: usize) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Array(_) | Type::Map(_, _) => Ok(Type::Int),
            ty if self.is_string_like(ty) => Ok(Type::Int),
            _ => Err(CompileError::new(
                line,
                format!("type {} has no len method", binding.ty.name()),
            )),
        }
    }

    pub(super) fn check_is_empty(&self, name: &str, line: usize) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Array(_) | Type::Map(_, _) => Ok(Type::Bool),
            ty if self.is_string_like(ty) => Ok(Type::Bool),
            _ => Err(CompileError::new(
                line,
                format!("type {} has no isEmpty method", binding.ty.name()),
            )),
        }
    }

    pub(super) fn check_array_edge(
        &self,
        name: &str,
        method: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Array(element) => Ok((**element).clone()),
            _ => Err(CompileError::new(
                line,
                format!("type {} has no {method} method", binding.ty.name()),
            )),
        }
    }

    pub(super) fn check_array_edge_value(
        &self,
        value: &Expr,
        method: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, line)?;
        let Type::Array(element) = value_ty else {
            return Err(CompileError::new(
                line,
                format!("type {} has no {method} method", value_ty.name()),
            ));
        };
        if *element == Type::Unit {
            return Err(CompileError::new(
                line,
                format!("{method} requires a non-empty array literal"),
            ));
        }
        if !matches!(value, Expr::Array(_)) {
            return Err(CompileError::new(
                line,
                format!("{method} value must be an array literal or named array"),
            ));
        }
        Ok(*element)
    }

    pub(super) fn check_array_reverse(
        &self,
        name: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Array(element) => Ok(Type::Array(element.clone())),
            _ => Err(CompileError::new(
                line,
                format!("type {} has no reverse method", binding.ty.name()),
            )),
        }
    }

    pub(super) fn check_array_sort(&self, name: &str, line: usize) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Array(element) if self.is_string_like(element) => {
                Ok(Type::Array(element.clone()))
            }
            Type::Array(element) => Err(CompileError::new(
                line,
                format!(
                    "sort array elements must be String or Path, found {}",
                    element.name()
                ),
            )),
            _ => Err(CompileError::new(
                line,
                format!("type {} has no sort method", binding.ty.name()),
            )),
        }
    }

    pub(super) fn check_array_unique(&self, name: &str, line: usize) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Array(element) => Ok(Type::Array(element.clone())),
            _ => Err(CompileError::new(
                line,
                format!("type {} has no unique method", binding.ty.name()),
            )),
        }
    }

    pub(super) fn check_array_transform_value(
        &self,
        value: &Expr,
        method: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, line)?;
        let Type::Array(element) = value_ty else {
            return Err(CompileError::new(
                line,
                format!("type {} has no {method} method", value_ty.name()),
            ));
        };
        if !matches!(value, Expr::Array(_)) {
            return Err(CompileError::new(
                line,
                format!("{method} value must be an array literal or named array"),
            ));
        }
        Ok(Type::Array(element))
    }

    pub(super) fn check_array_sort_value(
        &self,
        value: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_array_transform_value(value, "sort", line)?;
        match value_ty {
            Type::Array(element) if self.is_string_like(&element) => Ok(Type::Array(element)),
            Type::Array(element) => Err(CompileError::new(
                line,
                format!(
                    "sort array elements must be String or Path, found {}",
                    element.name()
                ),
            )),
            other => Err(CompileError::new(
                line,
                format!("type {} has no sort method", other.name()),
            )),
        }
    }

    pub(super) fn check_array_map(
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
        let element = match binding.ty {
            Type::Array(element) => element,
            other => {
                return Err(CompileError::new(
                    line,
                    format!("type {} has no map method", other.name()),
                ))
            }
        };
        let mapped = self.check_map_mapper(mapper, &element, "map", line)?;
        Ok((*element, mapped))
    }

    pub(super) fn check_array_map_value(
        &self,
        value: &Expr,
        mapper: &Expr,
        line: usize,
    ) -> Result<(Type, Type), CompileError> {
        let value_ty = self.check_expr(value, line)?;
        let Type::Array(element) = value_ty else {
            return Err(CompileError::new(
                line,
                format!("type {} has no map method", value_ty.name()),
            ));
        };
        if *element == Type::Unit {
            return Err(CompileError::new(
                line,
                "map requires a non-empty array literal or typed array".to_string(),
            ));
        }
        if !matches!(value, Expr::Array(_)) {
            return Err(CompileError::new(
                line,
                "map value must be an array literal or named array".to_string(),
            ));
        }
        let mapped = self.check_map_mapper(mapper, &element, "map", line)?;
        Ok((*element, mapped))
    }

    pub(super) fn check_array_predicate(
        &self,
        name: &str,
        predicate: &Expr,
        method: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        let Type::Array(element) = binding.ty else {
            return Err(CompileError::new(
                line,
                format!("type {} has no {method} method", binding.ty.name()),
            ));
        };
        self.check_predicate_mapper(predicate, &element, method, line)?;
        Ok(*element)
    }

    pub(super) fn check_array_predicate_value(
        &self,
        value: &Expr,
        predicate: &Expr,
        method: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, line)?;
        let Type::Array(element) = value_ty else {
            return Err(CompileError::new(
                line,
                format!("type {} has no {method} method", value_ty.name()),
            ));
        };
        if *element == Type::Unit {
            return Err(CompileError::new(
                line,
                format!("{method} requires a non-empty array literal or typed array"),
            ));
        }
        self.check_predicate_mapper(predicate, &element, method, line)?;
        Ok(*element)
    }

    pub(super) fn check_array_flat_map(
        &self,
        name: &str,
        mapper: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        let Type::Array(element) = binding.ty else {
            return Err(CompileError::new(
                line,
                format!("type {} has no flatMap method", binding.ty.name()),
            ));
        };
        self.check_array_flat_map_mapper(mapper, &element, line)
    }

    pub(super) fn check_array_flat_map_value(
        &self,
        value: &Expr,
        mapper: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, line)?;
        let Type::Array(element) = value_ty else {
            return Err(CompileError::new(
                line,
                format!("type {} has no flatMap method", value_ty.name()),
            ));
        };
        if *element == Type::Unit {
            return Err(CompileError::new(
                line,
                "flatMap requires a non-empty array literal or typed array".to_string(),
            ));
        }
        self.check_array_flat_map_mapper(mapper, &element, line)
    }

    pub(super) fn check_array_fold(
        &self,
        name: &str,
        initial: &Expr,
        reducer: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        let Type::Array(element) = binding.ty else {
            return Err(CompileError::new(
                line,
                format!("type {} has no fold method", binding.ty.name()),
            ));
        };
        let accumulator = self.check_expr(initial, line)?;
        self.check_fold_reducer(reducer, &accumulator, &element, line)?;
        Ok(accumulator)
    }

    pub(super) fn check_array_fold_value(
        &self,
        value: &Expr,
        initial: &Expr,
        reducer: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, line)?;
        let Type::Array(element) = value_ty else {
            return Err(CompileError::new(
                line,
                format!("type {} has no fold method", value_ty.name()),
            ));
        };
        if *element == Type::Unit {
            return Err(CompileError::new(
                line,
                "fold requires a non-empty array literal or typed array".to_string(),
            ));
        }
        let accumulator = self.check_expr(initial, line)?;
        self.check_fold_reducer(reducer, &accumulator, &element, line)?;
        Ok(accumulator)
    }

    pub(super) fn check_map_mapper(
        &self,
        mapper: &Expr,
        element: &Type,
        method: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        if let Expr::Lambda { params, body } = mapper {
            if params.len() != 1 {
                return Err(CompileError::new(
                    line,
                    format!(
                        "{method} lambda expects 1 parameter, found {}",
                        params.len()
                    ),
                ));
            }
            let (mut checker, _) =
                self.lambda_checker(params, std::slice::from_ref(element), line)?;
            let mut body = (**body).clone();
            if let Some(lifted) = checker.lift_lazy_try_branch(&mut body) {
                return checker.check_expr(&lifted, line);
            }
            return checker.check_expr(&body, line);
        }
        let mapper_ty = self.check_expr(mapper, line)?;
        let Type::Function(params, return_type) = mapper_ty else {
            return Err(CompileError::new(
                line,
                format!(
                    "{method} mapper must be a function, found {}",
                    mapper_ty.name()
                ),
            ));
        };
        if params.len() != 1 {
            return Err(CompileError::new(
                line,
                format!(
                    "{method} mapper expects 1 parameter, found {}",
                    params.len()
                ),
            ));
        }
        if !self.is_assignable(&params[0], element, mapper) {
            return Err(CompileError::new(
                line,
                format!(
                    "{method} mapper parameter must accept {}, found {}",
                    element.name(),
                    params[0].name()
                ),
            ));
        }
        Ok(*return_type)
    }

    pub(super) fn check_predicate_mapper(
        &self,
        predicate: &Expr,
        element: &Type,
        method: &str,
        line: usize,
    ) -> Result<(), CompileError> {
        let result = self.check_map_mapper(predicate, element, method, line)?;
        if result == Type::Bool {
            Ok(())
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "{method} predicate must return Bool, found {}",
                    result.name()
                ),
            ))
        }
    }

    pub(super) fn check_array_flat_map_mapper(
        &self,
        mapper: &Expr,
        element: &Type,
        line: usize,
    ) -> Result<Type, CompileError> {
        let result = self.check_map_mapper(mapper, element, "flatMap", line)?;
        let Type::Array(mapped) = result else {
            return Err(CompileError::new(
                line,
                format!(
                    "flatMap mapper must return an array, found {}",
                    result.name()
                ),
            ));
        };
        if *mapped == Type::Unit {
            return Err(CompileError::new(
                line,
                "flatMap mapper must return a typed or non-empty array".to_string(),
            ));
        }
        Ok(*mapped)
    }

    pub(super) fn check_fold_reducer(
        &self,
        reducer: &Expr,
        accumulator: &Type,
        element: &Type,
        line: usize,
    ) -> Result<(), CompileError> {
        let result = if let Expr::Lambda { params, body } = reducer {
            if params.len() != 2 {
                return Err(CompileError::new(
                    line,
                    format!("fold reducer expects 2 parameters, found {}", params.len()),
                ));
            }
            let (checker, _) =
                self.lambda_checker(params, &[accumulator.clone(), element.clone()], line)?;
            checker.check_expr(body, line)?
        } else {
            let reducer_ty = self.check_expr(reducer, line)?;
            let Type::Function(params, return_type) = reducer_ty else {
                return Err(CompileError::new(
                    line,
                    format!(
                        "fold reducer must be a function, found {}",
                        reducer_ty.name()
                    ),
                ));
            };
            if params.len() != 2 {
                return Err(CompileError::new(
                    line,
                    format!("fold reducer expects 2 parameters, found {}", params.len()),
                ));
            }
            if !self.is_assignable(&params[0], accumulator, reducer)
                || !self.is_assignable(&params[1], element, reducer)
            {
                return Err(CompileError::new(
                    line,
                    format!(
                        "fold reducer must accept ({}, {}), found ({}, {})",
                        accumulator.name(),
                        element.name(),
                        params[0].name(),
                        params[1].name()
                    ),
                ));
            }
            *return_type
        };
        if self.is_assignable(accumulator, &result, reducer) {
            Ok(())
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "fold reducer must return {}, found {}",
                    accumulator.name(),
                    result.name()
                ),
            ))
        }
    }

    pub(super) fn lower_map_mapper(
        &self,
        mapper: &Expr,
        element: &Type,
        mapped: &Type,
        line: usize,
    ) -> Result<Expr, CompileError> {
        if matches!(mapper, Expr::Lambda { .. }) {
            return self.lower_expr_expected(
                mapper,
                &Type::Function(vec![element.clone()], Box::new(mapped.clone())),
                line,
            );
        }
        self.lower_expr(mapper, line)
    }

    pub(super) fn lower_fold_reducer(
        &self,
        reducer: &Expr,
        accumulator: &Type,
        element: &Type,
        line: usize,
    ) -> Result<Expr, CompileError> {
        if matches!(reducer, Expr::Lambda { .. }) {
            return self.lower_expr_expected(
                reducer,
                &Type::Function(
                    vec![accumulator.clone(), element.clone()],
                    Box::new(accumulator.clone()),
                ),
                line,
            );
        }
        self.lower_expr(reducer, line)
    }

    pub(super) fn check_array_count(
        &self,
        name: &str,
        count: &Expr,
        method: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        let count_ty = self.check_expr(count, line)?;
        if count_ty != Type::Int {
            return Err(CompileError::new(
                line,
                format!("{method} count must be Int, found {}", count_ty.name()),
            ));
        }
        match &binding.ty {
            Type::Array(element) => Ok(Type::Array(element.clone())),
            _ => Err(CompileError::new(
                line,
                format!("type {} has no {method} method", binding.ty.name()),
            )),
        }
    }

    pub(super) fn check_array_count_value(
        &self,
        value: &Expr,
        count: &Expr,
        method: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let count_ty = self.check_expr(count, line)?;
        if count_ty != Type::Int {
            return Err(CompileError::new(
                line,
                format!("{method} count must be Int, found {}", count_ty.name()),
            ));
        }
        self.check_array_value(value, method, line)
    }

    pub(super) fn check_array_value(
        &self,
        value: &Expr,
        method: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, line)?;
        let Type::Array(element) = value_ty else {
            return Err(CompileError::new(
                line,
                format!("type {} has no {method} method", value_ty.name()),
            ));
        };
        if !matches!(value, Expr::Array(_)) {
            return Err(CompileError::new(
                line,
                format!("{method} value must be an array literal or named array"),
            ));
        }
        Ok(Type::Array(element))
    }

    pub(super) fn check_map_value(
        &self,
        value: &Expr,
        method: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, line)?;
        let Type::Map(key, value) = value_ty else {
            return Err(CompileError::new(
                line,
                format!("type {} has no {method} method", value_ty.name()),
            ));
        };
        Ok(Type::Map(key, value))
    }

    pub(super) fn check_join(
        &self,
        name: &str,
        separator: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        let separator_ty = self.check_expr(separator, line)?;
        if !self.is_string_like(&separator_ty) {
            return Err(CompileError::new(
                line,
                format!(
                    "join separator must be String or Path, found {}",
                    separator_ty.name()
                ),
            ));
        }
        match &binding.ty {
            Type::Array(element) if self.is_string_like(element) => Ok(Type::String),
            Type::Array(element) => Err(CompileError::new(
                line,
                format!(
                    "join array elements must be String or Path, found {}",
                    element.name()
                ),
            )),
            _ => Err(CompileError::new(
                line,
                format!("type {} has no join method", binding.ty.name()),
            )),
        }
    }

    pub(super) fn check_join_value(
        &self,
        value: &Expr,
        separator: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        if !matches!(value, Expr::Array(_)) {
            return Err(CompileError::new(
                line,
                "join value must be an array literal or named array".to_string(),
            ));
        }
        let separator_ty = self.check_expr(separator, line)?;
        if !self.is_string_like(&separator_ty) {
            return Err(CompileError::new(
                line,
                format!(
                    "join separator must be String or Path, found {}",
                    separator_ty.name()
                ),
            ));
        }
        match self.check_expr(value, line)? {
            Type::Array(element) if self.is_string_like(&element) => Ok(Type::String),
            Type::Array(element) => Err(CompileError::new(
                line,
                format!(
                    "join array elements must be String or Path, found {}",
                    element.name()
                ),
            )),
            other => Err(CompileError::new(
                line,
                format!("type {} has no join method", other.name()),
            )),
        }
    }

    pub(super) fn check_array_push(
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
        if !binding.mutable {
            return Err(CompileError::new(
                line,
                format!("cannot push to const array `{name}`"),
            ));
        }
        let Type::Array(element) = &binding.ty else {
            return Err(CompileError::new(
                line,
                format!("type {} has no push method", binding.ty.name()),
            ));
        };
        let value_ty = self.check_expr(value, line)?;
        if self.is_assignable(element, &value_ty, value) {
            Ok(Type::Unit)
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "push value type mismatch: expected {}, found {}",
                    element.name(),
                    value_ty.name()
                ),
            ))
        }
    }

    pub(super) fn check_array_pop(&self, name: &str, line: usize) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        if !binding.mutable {
            return Err(CompileError::new(
                line,
                format!("cannot pop from const array `{name}`"),
            ));
        }
        let Type::Array(_) = &binding.ty else {
            return Err(CompileError::new(
                line,
                format!("type {} has no pop method", binding.ty.name()),
            ));
        };
        Ok(Type::Unit)
    }

    pub(super) fn check_array_contains(
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
        let Type::Array(element) = &binding.ty else {
            return Err(CompileError::new(
                line,
                format!("type {} has no contains method", binding.ty.name()),
            ));
        };
        let value_ty = self.check_expr(value, line)?;
        if self.is_assignable(element, &value_ty, value) {
            Ok(Type::Bool)
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "contains value type mismatch: expected {}, found {}",
                    element.name(),
                    value_ty.name()
                ),
            ))
        }
    }

    pub(super) fn check_array_contains_value(
        &self,
        value: &Expr,
        item: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        self.check_array_value_item(value, item, "contains", line)
            .map(|_| Type::Bool)
    }

    pub(super) fn check_index_of(
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
        match &binding.ty {
            Type::Array(element) => {
                let value_ty = self.check_expr(value, line)?;
                if self.is_assignable(element, &value_ty, value) {
                    Ok(Type::Int)
                } else {
                    Err(CompileError::new(
                        line,
                        format!(
                            "indexOf value type mismatch: expected {}, found {}",
                            element.name(),
                            value_ty.name()
                        ),
                    ))
                }
            }
            ty if self.is_string_like(ty) => self.check_string_index_of(name, value, line),
            _ => Err(CompileError::new(
                line,
                format!("type {} has no indexOf method", binding.ty.name()),
            )),
        }
    }

    pub(super) fn check_array_index_of_value(
        &self,
        value: &Expr,
        item: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        self.check_array_value_item(value, item, "indexOf", line)
            .map(|_| Type::Int)
    }

    pub(super) fn check_array_value_item(
        &self,
        value: &Expr,
        item: &Expr,
        method: &str,
        line: usize,
    ) -> Result<(), CompileError> {
        let value_ty = self.check_expr(value, line)?;
        let Type::Array(element) = value_ty else {
            return Err(CompileError::new(
                line,
                format!("type {} has no {method} method", value_ty.name()),
            ));
        };
        if !matches!(value, Expr::Array(_)) {
            return Err(CompileError::new(
                line,
                format!("{method} value must be an array literal or named array"),
            ));
        }
        let item_ty = self.check_expr(item, line)?;
        if self.is_assignable(&element, &item_ty, item) {
            Ok(())
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "{method} value type mismatch: expected {}, found {}",
                    element.name(),
                    item_ty.name()
                ),
            ))
        }
    }

    pub(super) fn check_map_keys(&self, name: &str, line: usize) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Map(key, _) => Ok(Type::Array(key.clone())),
            _ => Err(CompileError::new(
                line,
                format!("type {} has no keys method", binding.ty.name()),
            )),
        }
    }

    pub(super) fn check_map_values(&self, name: &str, line: usize) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Map(_, value) => Ok(Type::Array(value.clone())),
            _ => Err(CompileError::new(
                line,
                format!("type {} has no values method", binding.ty.name()),
            )),
        }
    }

    pub(super) fn check_map_has(
        &self,
        name: &str,
        key: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let key_ty = self.check_expr(key, line)?;
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Map(expected, _) if self.is_assignable(expected, &key_ty, key) => Ok(Type::Bool),
            Type::Map(expected, _) => Err(CompileError::new(
                line,
                format!(
                    "map key must be {}, found {}",
                    expected.name(),
                    key_ty.name()
                ),
            )),
            _ => Err(CompileError::new(
                line,
                format!("type {} has no has method", binding.ty.name()),
            )),
        }
    }

    pub(super) fn check_map_keys_value(
        &self,
        value: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        match self.check_expr(value, line)? {
            Type::Map(key, _) => Ok(Type::Array(key)),
            other => Err(CompileError::new(
                line,
                format!("type {} has no keys method", other.name()),
            )),
        }
    }

    pub(super) fn check_map_values_value(
        &self,
        value: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        match self.check_expr(value, line)? {
            Type::Map(_, value) => Ok(Type::Array(value)),
            other => Err(CompileError::new(
                line,
                format!("type {} has no values method", other.name()),
            )),
        }
    }

    pub(super) fn check_map_has_value(
        &self,
        value: &Expr,
        key: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let key_ty = self.check_expr(key, line)?;
        match self.check_expr(value, line)? {
            Type::Map(expected, _) if self.is_assignable(&expected, &key_ty, key) => Ok(Type::Bool),
            Type::Map(expected, _) => Err(CompileError::new(
                line,
                format!(
                    "map key must be {}, found {}",
                    expected.name(),
                    key_ty.name()
                ),
            )),
            other => Err(CompileError::new(
                line,
                format!("type {} has no has method", other.name()),
            )),
        }
    }

    pub(super) fn check_map_set(
        &self,
        name: &str,
        key: &Expr,
        value: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        if !binding.mutable {
            return Err(CompileError::new(
                line,
                format!("cannot set const map `{name}`"),
            ));
        }
        let Type::Map(expected_key, expected_value) = &binding.ty else {
            return Err(CompileError::new(
                line,
                format!("type {} has no set method", binding.ty.name()),
            ));
        };
        let key_ty = self.check_expr(key, line)?;
        if !self.is_assignable(expected_key, &key_ty, key) {
            return Err(CompileError::new(
                line,
                format!(
                    "map key must be {}, found {}",
                    expected_key.name(),
                    key_ty.name()
                ),
            ));
        }
        let value_ty = self.check_expr(value, line)?;
        if !self.is_assignable(expected_value, &value_ty, value) {
            return Err(CompileError::new(
                line,
                format!(
                    "map value must be {}, found {}",
                    expected_value.name(),
                    value_ty.name()
                ),
            ));
        }
        Ok(Type::Unit)
    }

    pub(super) fn check_map_remove(
        &self,
        name: &str,
        key: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        if !binding.mutable {
            return Err(CompileError::new(
                line,
                format!("cannot remove from const map `{name}`"),
            ));
        }
        let Type::Map(expected_key, _) = &binding.ty else {
            return Err(CompileError::new(
                line,
                format!("type {} has no remove method", binding.ty.name()),
            ));
        };
        let key_ty = self.check_expr(key, line)?;
        if self.is_assignable(expected_key, &key_ty, key) {
            Ok(Type::Unit)
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "map key must be {}, found {}",
                    expected_key.name(),
                    key_ty.name()
                ),
            ))
        }
    }

    pub(super) fn check_tuple_field(
        &self,
        name: &str,
        field: usize,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Tuple(elements) => elements.get(field - 1).cloned().ok_or_else(|| {
                CompileError::new(
                    line,
                    format!(
                        "tuple `{name}` has no field _{field}; it has {} fields",
                        elements.len()
                    ),
                )
            }),
            other => Err(CompileError::new(
                line,
                format!(
                    "cannot access tuple field on `{name}` of type {}",
                    other.name()
                ),
            )),
        }
    }

    pub(super) fn check_tuple_field_value(
        &self,
        value: &Expr,
        field: usize,
        line: usize,
    ) -> Result<Type, CompileError> {
        match self.check_expr(value, line)? {
            Type::Tuple(elements) => elements.get(field - 1).cloned().ok_or_else(|| {
                CompileError::new(
                    line,
                    format!(
                        "tuple value has no field _{field}; it has {} fields",
                        elements.len()
                    ),
                )
            }),
            other => Err(CompileError::new(
                line,
                format!(
                    "cannot access tuple field on value of type {}",
                    other.name()
                ),
            )),
        }
    }

    pub(super) fn check_field(
        &self,
        name: &str,
        field: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            let qualified = format!("{name}.{field}");
            if let Some(sig) = self.functions.get(&qualified) {
                return Ok(sig.function_type());
            }
            if let Some(binding) = self.lookup_binding(&associated_const_name(name, field)) {
                return Ok(binding.ty);
            }
            if let Some(binding) = self.lookup_binding(&module_binding_name(name, field)) {
                return Ok(binding.ty);
            }
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        match &binding.ty {
            Type::Record(fields) => fields
                .iter()
                .find(|(candidate, _)| candidate == field)
                .map(|(_, ty)| ty.clone())
                .ok_or_else(|| {
                    CompileError::new(line, format!("record `{name}` has no field `{field}`"))
                }),
            other => Err(CompileError::new(
                line,
                format!(
                    "cannot access field `{field}` on `{name}` of type {}",
                    other.name()
                ),
            )),
        }
    }

    pub(super) fn check_field_value(
        &self,
        value: &Expr,
        field: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        if !matches!(value, Expr::Record(_)) {
            if let Some(name) = value.qualified_name() {
                if let Some(binding) = self.lookup_binding(&associated_const_name(&name, field)) {
                    return Ok(binding.ty);
                }
            }
            let ty = self.check_expr(value, line)?;
            return Err(CompileError::new(
                line,
                format!(
                    "cannot access field `{field}` on value of type {}; use a named record or record literal",
                    ty.name()
                ),
            ));
        }
        match self.check_expr(value, line)? {
            Type::Record(fields) => fields
                .into_iter()
                .find(|(candidate, _)| candidate == field)
                .map(|(_, ty)| ty)
                .ok_or_else(|| {
                    CompileError::new(line, format!("record value has no field `{field}`"))
                }),
            other => Err(CompileError::new(
                line,
                format!(
                    "cannot access field `{field}` on value of type {}",
                    other.name()
                ),
            )),
        }
    }
}
