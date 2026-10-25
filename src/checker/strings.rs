use super::TypeChecker;
use crate::{CompileError, Expr, Type};

impl TypeChecker {
    pub(super) fn check_string_contains(
        &self,
        name: &str,
        needle: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        self.check_string_predicate(name, needle, "contains", "needle", line)
    }

    pub(super) fn check_string_index_of(
        &self,
        name: &str,
        needle: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        if !self.is_string_like(&binding.ty) {
            return Err(CompileError::new(
                line,
                format!("type {} has no indexOf method", binding.ty.name()),
            ));
        }
        let needle_ty = self.check_expr(needle, line)?;
        if self.is_string_like(&needle_ty) {
            Ok(Type::Int)
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "indexOf needle must be String or Path, found {}",
                    needle_ty.name()
                ),
            ))
        }
    }

    pub(super) fn check_string_starts_with(
        &self,
        name: &str,
        prefix: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        self.check_string_predicate(name, prefix, "startsWith", "prefix", line)
    }

    pub(super) fn check_string_ends_with(
        &self,
        name: &str,
        suffix: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        self.check_string_predicate(name, suffix, "endsWith", "suffix", line)
    }

    pub(super) fn check_string_predicate(
        &self,
        name: &str,
        value: &Expr,
        method: &str,
        arg_name: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        if let Type::Array(element) = &binding.ty {
            if method != "contains" {
                return Err(CompileError::new(
                    line,
                    format!("type {} has no {method} method", binding.ty.name()),
                ));
            }
            let value_ty = self.check_expr(value, line)?;
            return if self.is_assignable(element, &value_ty, value) {
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
            };
        }
        if !self.is_string_like(&binding.ty) {
            return Err(CompileError::new(
                line,
                format!("type {} has no {method} method", binding.ty.name()),
            ));
        }
        let value_ty = self.check_expr(value, line)?;
        if self.is_string_like(&value_ty) {
            Ok(Type::Bool)
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "{method} {arg_name} must be String or Path, found {}",
                    value_ty.name()
                ),
            ))
        }
    }

    pub(super) fn check_string_predicate_value(
        &self,
        receiver: &Expr,
        value: &Expr,
        method: &str,
        arg_name: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let receiver_ty = self.check_expr(receiver, line)?;
        if !self.is_string_like(&receiver_ty) {
            return Err(CompileError::new(
                line,
                format!("type {} has no {method} method", receiver_ty.name()),
            ));
        }
        let value_ty = self.check_expr(value, line)?;
        if self.is_string_like(&value_ty) {
            Ok(Type::Bool)
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "{method} {arg_name} must be String or Path, found {}",
                    value_ty.name()
                ),
            ))
        }
    }

    pub(super) fn check_string_index_of_value(
        &self,
        receiver: &Expr,
        needle: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let receiver_ty = self.check_expr(receiver, line)?;
        if !self.is_string_like(&receiver_ty) {
            return Err(CompileError::new(
                line,
                format!("type {} has no indexOf method", receiver_ty.name()),
            ));
        }
        let needle_ty = self.check_expr(needle, line)?;
        if self.is_string_like(&needle_ty) {
            Ok(Type::Int)
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "indexOf needle must be String or Path, found {}",
                    needle_ty.name()
                ),
            ))
        }
    }

    pub(super) fn check_string_trim(&self, name: &str, line: usize) -> Result<Type, CompileError> {
        self.check_string_transform(name, "trim", line)
    }

    pub(super) fn check_string_to_upper(
        &self,
        name: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        self.check_string_transform(name, "toUpper", line)
    }

    pub(super) fn check_string_to_lower(
        &self,
        name: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        self.check_string_transform(name, "toLower", line)
    }

    pub(super) fn check_string_repeat(
        &self,
        name: &str,
        count: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        if !self.is_string_like(&binding.ty) {
            return Err(CompileError::new(
                line,
                format!("type {} has no repeat method", binding.ty.name()),
            ));
        }
        let count_ty = self.check_expr(count, line)?;
        if count_ty == Type::Int {
            Ok(Type::String)
        } else {
            Err(CompileError::new(
                line,
                format!("repeat count must be Int, found {}", count_ty.name()),
            ))
        }
    }

    pub(super) fn check_string_repeat_value(
        &self,
        value: &Expr,
        count: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, line)?;
        if !self.is_string_like(&value_ty) {
            return Err(CompileError::new(
                line,
                format!("type {} has no repeat method", value_ty.name()),
            ));
        }
        let count_ty = self.check_expr(count, line)?;
        if count_ty == Type::Int {
            Ok(Type::String)
        } else {
            Err(CompileError::new(
                line,
                format!("repeat count must be Int, found {}", count_ty.name()),
            ))
        }
    }

    pub(super) fn check_string_transform(
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
        if self.is_string_like(&binding.ty) {
            Ok(Type::String)
        } else {
            Err(CompileError::new(
                line,
                format!("type {} has no {method} method", binding.ty.name()),
            ))
        }
    }

    pub(super) fn check_string_transform_value(
        &self,
        value: &Expr,
        method: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, line)?;
        if self.is_string_like(&value_ty) {
            Ok(Type::String)
        } else {
            Err(CompileError::new(
                line,
                format!("type {} has no {method} method", value_ty.name()),
            ))
        }
    }

    pub(super) fn check_string_split(
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
        if !self.is_string_like(&binding.ty) {
            return Err(CompileError::new(
                line,
                format!("type {} has no split method", binding.ty.name()),
            ));
        }
        let separator_ty = self.check_expr(separator, line)?;
        if self.is_string_like(&separator_ty) {
            Ok(Type::Array(Box::new(Type::String)))
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "split separator must be String or Path, found {}",
                    separator_ty.name()
                ),
            ))
        }
    }

    pub(super) fn check_string_split_value(
        &self,
        value: &Expr,
        separator: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, line)?;
        if !self.is_string_like(&value_ty) {
            return Err(CompileError::new(
                line,
                format!("type {} has no split method", value_ty.name()),
            ));
        }
        let separator_ty = self.check_expr(separator, line)?;
        if self.is_string_like(&separator_ty) {
            Ok(Type::Array(Box::new(Type::String)))
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "split separator must be String or Path, found {}",
                    separator_ty.name()
                ),
            ))
        }
    }

    pub(super) fn check_string_replace(
        &self,
        name: &str,
        from: &Expr,
        to: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Some(binding) = self.lookup_binding(name) else {
            return Err(CompileError::new(
                line,
                format!("undefined variable `{name}`"),
            ));
        };
        if !self.is_string_like(&binding.ty) {
            return Err(CompileError::new(
                line,
                format!("type {} has no replace method", binding.ty.name()),
            ));
        }
        let from_ty = self.check_expr(from, line)?;
        if !self.is_string_like(&from_ty) {
            return Err(CompileError::new(
                line,
                format!(
                    "replace search must be String or Path, found {}",
                    from_ty.name()
                ),
            ));
        }
        let to_ty = self.check_expr(to, line)?;
        if !self.is_string_like(&to_ty) {
            return Err(CompileError::new(
                line,
                format!(
                    "replace replacement must be String or Path, found {}",
                    to_ty.name()
                ),
            ));
        }
        Ok(Type::String)
    }

    pub(super) fn check_string_replace_value(
        &self,
        value: &Expr,
        from: &Expr,
        to: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, line)?;
        if !self.is_string_like(&value_ty) {
            return Err(CompileError::new(
                line,
                format!("type {} has no replace method", value_ty.name()),
            ));
        }
        let from_ty = self.check_expr(from, line)?;
        if !self.is_string_like(&from_ty) {
            return Err(CompileError::new(
                line,
                format!(
                    "replace search must be String or Path, found {}",
                    from_ty.name()
                ),
            ));
        }
        let to_ty = self.check_expr(to, line)?;
        if !self.is_string_like(&to_ty) {
            return Err(CompileError::new(
                line,
                format!(
                    "replace replacement must be String or Path, found {}",
                    to_ty.name()
                ),
            ));
        }
        Ok(Type::String)
    }

    pub(super) fn check_path_transform(
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
        if self.is_string_like(&binding.ty) {
            Ok(Type::String)
        } else {
            Err(CompileError::new(
                line,
                format!("type {} has no {method} method", binding.ty.name()),
            ))
        }
    }

    pub(super) fn check_path_transform_value(
        &self,
        value: &Expr,
        method: &str,
        line: usize,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, line)?;
        if self.is_string_like(&value_ty) {
            Ok(Type::String)
        } else {
            Err(CompileError::new(
                line,
                format!("type {} has no {method} method", value_ty.name()),
            ))
        }
    }

    pub(super) fn check_path_predicate(
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
        if self.is_string_like(&binding.ty) {
            Ok(Type::Bool)
        } else {
            Err(CompileError::new(
                line,
                format!("type {} has no {method} method", binding.ty.name()),
            ))
        }
    }
}
