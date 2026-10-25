use super::type_utils::{command_result_type, result_types};
use super::{Binding, TypeChecker};
use crate::{AssignTarget, BindingPattern, CompileError, Expr, ForBinding, Type};

impl TypeChecker {
    pub(super) fn define(
        &mut self,
        name: &str,
        ty: Type,
        mutable: bool,
        line: usize,
    ) -> Result<(), CompileError> {
        if name == "_" {
            return Ok(());
        }
        if name == "args" {
            return Err(CompileError::new(
                line,
                "`args` is reserved for process arguments".to_string(),
            ));
        }
        if self.bindings.contains_key(name) {
            return Err(CompileError::new(
                line,
                format!("variable `{name}` is already defined"),
            ));
        }
        if self.captured_bindings.is_some() {
            self.capture_params.insert(name.to_string());
        }
        self.bindings
            .insert(name.to_string(), Binding { ty, mutable });
        Ok(())
    }

    pub(super) fn lookup_binding(&self, name: &str) -> Option<Binding> {
        let binding = self.bindings.get(name).cloned().or_else(|| {
            (name == "args").then(|| Binding {
                ty: Type::Array(Box::new(Type::String)),
                mutable: false,
            })
        });
        if binding.is_some() {
            self.record_capture(name);
        }
        binding
    }

    pub(super) fn record_capture(&self, name: &str) {
        if !self.capture_params.contains(name) {
            if let Some(captures) = &self.captured_bindings {
                captures.borrow_mut().insert(name.to_string());
            }
        }
    }

    pub(super) fn is_qualified_function_call(&self, receiver: &str, method: &str) -> bool {
        self.lookup_binding(receiver).is_none()
            && self.functions.contains_key(&format!("{receiver}.{method}"))
    }

    pub(super) fn define_destructure(
        &mut self,
        pattern: &BindingPattern,
        ty: &Type,
        mutable: bool,
        line: usize,
    ) -> Result<(), CompileError> {
        match (pattern, ty) {
            (BindingPattern::Name(name), ty) => self.define(name, ty.clone(), mutable, line),
            (BindingPattern::Array { patterns, rest }, Type::Array(element)) => {
                for pattern in patterns {
                    self.define_destructure(pattern, element, mutable, line)?;
                }
                if let Some(rest) = rest {
                    self.define(rest, Type::Array(element.clone()), mutable, line)?;
                }
                Ok(())
            }
            (BindingPattern::Array { .. }, other) => Err(CompileError::new(
                line,
                format!(
                    "array destructuring requires array value, found {}",
                    other.name()
                ),
            )),
            (BindingPattern::Tuple(patterns), Type::Tuple(elements)) => {
                if patterns.len() != elements.len() {
                    return Err(CompileError::new(
                        line,
                        format!(
                            "tuple destructuring expected {} values, found {}",
                            patterns.len(),
                            elements.len()
                        ),
                    ));
                }
                for (pattern, ty) in patterns.iter().zip(elements) {
                    self.define_destructure(pattern, ty, mutable, line)?;
                }
                Ok(())
            }
            (BindingPattern::Tuple(_), other) => Err(CompileError::new(
                line,
                format!(
                    "tuple destructuring requires tuple value, found {}",
                    other.name()
                ),
            )),
            (BindingPattern::Record(bindings), Type::Record(fields)) => {
                for (field, pattern) in bindings {
                    let Some((_, ty)) = fields.iter().find(|(candidate, _)| candidate == field)
                    else {
                        return Err(CompileError::new(
                            line,
                            format!("record destructuring field `{field}` is missing"),
                        ));
                    };
                    self.define_destructure(pattern, ty, mutable, line)?;
                }
                Ok(())
            }
            (BindingPattern::Record(_), other) => Err(CompileError::new(
                line,
                format!(
                    "record destructuring requires record value, found {}",
                    other.name()
                ),
            )),
        }
    }

    pub(super) fn define_for_binding(
        &mut self,
        binding: &ForBinding,
        element_ty: &Type,
        line: usize,
    ) -> Result<(), CompileError> {
        match binding {
            ForBinding::Name(name) => self.define(name, element_ty.clone(), false, line),
            ForBinding::Pattern(pattern) => {
                self.define_destructure(pattern, element_ty, false, line)
            }
        }
    }

    pub(super) fn ensure_destructurable_source(
        &self,
        expr: &Expr,
        line: usize,
    ) -> Result<(), CompileError> {
        match expr {
            Expr::Array(_)
            | Expr::Tuple(_)
            | Expr::Record(_)
            | Expr::Ident(_)
            | Expr::ProcessArgs => Ok(()),
            _ => Err(CompileError::new(
                line,
                "destructuring requires an array, tuple, or record literal or variable".to_string(),
            )),
        }
    }

    pub(super) fn check_assignment(
        &mut self,
        target: &AssignTarget,
        expr: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        if matches!(target, AssignTarget::Name(name) if name == "_") {
            return self.check_expr(expr, line);
        }
        let expected = self.assignment_target_type(target, line)?;
        let expr_ty = self.check_expr_expected(expr, &expected, line)?;
        if !self.is_assignable(&expected, &expr_ty, expr) {
            return Err(CompileError::new(
                line,
                format!(
                    "assignment type mismatch: expected {}, found {}",
                    expected.name(),
                    expr_ty.name()
                ),
            ));
        }
        Ok(expected)
    }

    pub(super) fn lower_assign_target(
        &self,
        target: &AssignTarget,
        line: usize,
    ) -> Result<AssignTarget, CompileError> {
        match target {
            AssignTarget::Name(name) => Ok(AssignTarget::Name(name.clone())),
            AssignTarget::Index { name, index } => Ok(AssignTarget::Index {
                name: name.clone(),
                index: self.lower_expr(index, line)?,
            }),
            AssignTarget::FieldIndex { name, field, index } => Ok(AssignTarget::FieldIndex {
                name: name.clone(),
                field: field.clone(),
                index: self.lower_expr(index, line)?,
            }),
            AssignTarget::Field { name, field } => Ok(AssignTarget::Field {
                name: name.clone(),
                field: field.clone(),
            }),
            AssignTarget::TupleField { name, field } => Ok(AssignTarget::TupleField {
                name: name.clone(),
                field: *field,
            }),
        }
    }

    pub(super) fn assignment_target_type(
        &self,
        target: &AssignTarget,
        line: usize,
    ) -> Result<Type, CompileError> {
        match target {
            AssignTarget::Name(name) => {
                if name == "_" {
                    return Ok(Type::Unit);
                }
                let binding = self.assignment_base_binding(name, line)?;
                Ok(binding.ty.clone())
            }
            AssignTarget::Index { name, index } => {
                let binding = self.assignment_base_binding(name, line)?;
                let index_ty = self.check_expr(index, line)?;
                self.index_assignment_target_type(name, &binding.ty, index, &index_ty, line)
            }
            AssignTarget::FieldIndex { name, field, index } => {
                let binding = self.assignment_base_binding(name, line)?;
                let Type::Record(fields) = &binding.ty else {
                    return Err(CompileError::new(
                        line,
                        format!(
                            "cannot assign field `{field}` on `{name}` of type {}",
                            binding.ty.name()
                        ),
                    ));
                };
                let Some((_, field_ty)) = fields.iter().find(|(candidate, _)| candidate == field)
                else {
                    return Err(CompileError::new(
                        line,
                        format!("record `{name}` has no field `{field}`"),
                    ));
                };
                let index_ty = self.check_expr(index, line)?;
                self.index_assignment_target_type(
                    &format!("{name}.{field}"),
                    field_ty,
                    index,
                    &index_ty,
                    line,
                )
            }
            AssignTarget::Field { name, field } => {
                let binding = self.assignment_base_binding(name, line)?;
                let Type::Record(fields) = &binding.ty else {
                    return Err(CompileError::new(
                        line,
                        format!(
                            "cannot assign field `{field}` on `{name}` of type {}",
                            binding.ty.name()
                        ),
                    ));
                };
                fields
                    .iter()
                    .find(|(candidate, _)| candidate == field)
                    .map(|(_, ty)| ty.clone())
                    .ok_or_else(|| {
                        CompileError::new(line, format!("record `{name}` has no field `{field}`"))
                    })
            }
            AssignTarget::TupleField { name, .. } => {
                self.assignment_base_binding(name, line)?;
                Err(CompileError::new(
                    line,
                    "tuple fields are immutable; assign a rebuilt tuple instead".to_string(),
                ))
            }
        }
    }

    pub(super) fn index_assignment_target_type(
        &self,
        name: &str,
        target_ty: &Type,
        index: &Expr,
        index_ty: &Type,
        line: usize,
    ) -> Result<Type, CompileError> {
        match target_ty {
            Type::Array(element) => {
                if index_ty != &Type::Int {
                    return Err(CompileError::new(
                        line,
                        format!("array index must be Int, found {}", index_ty.name()),
                    ));
                }
                Ok((**element).clone())
            }
            Type::Map(key, value) => {
                if self.is_assignable(key, index_ty, index) {
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
                format!("cannot index-assign into `{name}` of type {}", other.name()),
            )),
        }
    }

    pub(super) fn assignment_base_binding(
        &self,
        name: &str,
        line: usize,
    ) -> Result<Binding, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("cannot assign to undefined variable `{name}`"),
            ));
        };
        if !binding.mutable {
            return Err(CompileError::new(
                line,
                format!("cannot assign to const `{name}`"),
            ));
        }
        Ok(binding)
    }

    pub(super) fn check_annotation(
        &self,
        annotation: Option<Type>,
        actual: Type,
        expr: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(expected) = annotation else {
            return Ok(actual);
        };
        let expected = self.resolve_type(&expected, line)?;
        if self.is_assignable(&expected, &actual, expr) {
            Ok(expected)
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "type annotation mismatch: expected {}, found {}",
                    expected.name(),
                    actual.name()
                ),
            ))
        }
    }

    pub(super) fn binding_expr_type(
        &self,
        annotation: Option<&Type>,
        expr: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        if let Expr::TryResult(value) = expr {
            return self.check_try_result_expr(value, line);
        }
        if let Some(annotation) = annotation {
            let expected = self.resolve_type(annotation, line)?;
            if matches!(
                expr,
                Expr::Command { .. } | Expr::Pipeline { .. } | Expr::TryPipeline { .. }
            ) && result_types(&expected).is_some()
            {
                return Ok(command_result_type());
            }
            return self.check_expr_expected(expr, &expected, line);
        }
        self.check_expr(expr, line)
    }
}
