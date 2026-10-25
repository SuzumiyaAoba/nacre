use super::names::{associated_const_name, method_call_parts, module_binding_name};
use super::type_utils::{command_result_type, option_element_type, result_types};
use super::{Binding, TypeChecker};
use crate::{CompileError, Expr, MatchArm, Type};

impl TypeChecker {
    pub(super) fn lower_binding_expr(
        &self,
        expr: &Expr,
        binding_ty: &Type,
        line: usize,
    ) -> Result<Expr, CompileError> {
        if let Some((ok_ty, _)) = result_types(binding_ty) {
            match expr {
                Expr::Command { command, .. } => {
                    return Ok(Expr::CommandResult {
                        command: command.clone(),
                    });
                }
                Expr::TryResult(value) => {
                    return Ok(Expr::TryResult(Box::new(
                        self.lower_try_result_value(value, line)?,
                    )));
                }
                Expr::Pipeline { input, commands } => {
                    return Ok(Expr::PipelineResult {
                        input: input
                            .as_ref()
                            .map(|input| self.lower_expr(input, line).map(Box::new))
                            .transpose()?,
                        commands: commands.clone(),
                    });
                }
                Expr::TryPipeline { input, commands } => {
                    return Ok(Expr::PipelineResult {
                        input: input
                            .as_ref()
                            .map(|input| self.lower_expr(input, line).map(Box::new))
                            .transpose()?,
                        commands: commands.clone(),
                    });
                }
                _ => {}
            }
            let actual = self.check_expr(expr, line)?;
            if !self.is_assignable(binding_ty, &actual, expr)
                && self.is_assignable(ok_ty, &actual, expr)
            {
                return Ok(Expr::Ok(Box::new(self.lower_expr(expr, line)?)));
            }
        }
        self.lower_expr(expr, line)
    }

    pub(super) fn lower_try_result_value(
        &self,
        expr: &Expr,
        line: usize,
    ) -> Result<Expr, CompileError> {
        match expr {
            Expr::Command { command, .. } => Ok(Expr::CommandResult {
                command: command.clone(),
            }),
            Expr::Pipeline { input, commands } | Expr::TryPipeline { input, commands } => {
                Ok(Expr::PipelineResult {
                    input: input
                        .as_ref()
                        .map(|input| self.lower_expr(input, line).map(Box::new))
                        .transpose()?,
                    commands: commands.clone(),
                })
            }
            _ => self.lower_expr(expr, line),
        }
    }

    pub(super) fn lower_expr(&self, expr: &Expr, line: usize) -> Result<Expr, CompileError> {
        match expr {
            Expr::AllowedCommand {
                group,
                command,
                args,
                result,
                ..
            } => {
                let policy = self.policy.command(group, command);
                if self.enforce_capabilities && policy.is_none() {
                    return Err(CompileError::new(
                        line,
                        format!(
                            "command `{group}.{command}` is not allowed by the execution policy"
                        ),
                    ));
                }
                Ok(Expr::AllowedCommand {
                    group: group.clone(),
                    command: command.clone(),
                    args: args
                        .iter()
                        .map(|arg| self.lower_expr(arg, line))
                        .collect::<Result<Vec<_>, _>>()?,
                    result: *result,
                    program: policy.map(|policy| policy.program().to_string_lossy().into_owned()),
                    read_args: policy
                        .map(|policy| policy.read_args().to_vec())
                        .unwrap_or_default(),
                    write_args: policy
                        .map(|policy| policy.write_args().to_vec())
                        .unwrap_or_default(),
                })
            }
            Expr::Async(value) => Ok(Expr::Async(Box::new(self.lower_expr(value, line)?))),
            Expr::Call { name, args } => {
                if self.variants.contains_key(name) {
                    return Ok(Expr::Variant {
                        name: name.clone(),
                        args: args
                            .iter()
                            .map(|arg| self.lower_expr(arg, line))
                            .collect::<Result<Vec<_>, _>>()?,
                        field_types: self.instantiate_variant_fields(name, args, line)?,
                    });
                }
                if let Some((trait_name, method)) = self.scoped_trait_method_parts(name) {
                    let lowered_name =
                        self.resolve_scoped_method_name(trait_name, method, args, line)?;
                    return Ok(Expr::Call {
                        args: self.lower_call_args(&lowered_name, args, line)?,
                        name: lowered_name,
                    });
                }
                if let Some((receiver, method)) = method_call_parts(name) {
                    if !self.functions.contains_key(name) {
                        let lowered_name = self.resolve_method_name(receiver, method, line)?;
                        let mut source_args = Vec::with_capacity(args.len() + 1);
                        source_args.push(Expr::Ident(receiver.to_string()));
                        source_args.extend_from_slice(args);
                        let lowered_args =
                            self.lower_call_args(&lowered_name, &source_args, line)?;
                        return Ok(Expr::Call {
                            name: lowered_name,
                            args: lowered_args,
                        });
                    }
                }
                if !self.functions.contains_key(name) {
                    if let Some(lowered_name) = self.resolve_unscoped_method_call(name, args, line)?
                    {
                        return Ok(Expr::Call {
                            name: lowered_name.clone(),
                            args: self.lower_call_args(&lowered_name, args, line)?,
                        });
                    }
                }
                Ok(Expr::Call {
                    name: name.clone(),
                    args: self.lower_call_args(name, args, line)?,
                })
            }
            Expr::NamedArg { .. } => Err(CompileError::new(
                line,
                "named arguments are only valid inside function calls".to_string(),
            )),
            Expr::Range {
                start,
                end,
                inclusive,
            } => Ok(Expr::Range {
                start: Box::new(self.lower_expr(start, line)?),
                end: Box::new(self.lower_expr(end, line)?),
                inclusive: *inclusive,
            }),
            Expr::Array(values) => Ok(Expr::Array(
                values
                    .iter()
                    .map(|value| self.lower_expr(value, line))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            Expr::Map(entries) => Ok(Expr::Map(
                entries
                    .iter()
                    .map(|(key, value)| {
                        Ok((self.lower_expr(key, line)?, self.lower_expr(value, line)?))
                    })
                    .collect::<Result<Vec<_>, CompileError>>()?,
            )),
            Expr::Record(fields) => Ok(Expr::Record(
                fields
                    .iter()
                    .map(|(name, value)| Ok((name.clone(), self.lower_expr(value, line)?)))
                    .collect::<Result<Vec<_>, CompileError>>()?,
            )),
            Expr::RecordPattern(fields) => Ok(Expr::RecordPattern(
                fields
                    .iter()
                    .map(|(name, value)| {
                        Ok((
                            name.clone(),
                            value
                                .as_ref()
                                .map(|value| self.lower_expr(value, line))
                                .transpose()?,
                        ))
                    })
                    .collect::<Result<Vec<_>, CompileError>>()?,
            )),
            Expr::Tuple(values) => Ok(Expr::Tuple(
                values
                    .iter()
                    .map(|value| self.lower_expr(value, line))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            Expr::Some(value) => Ok(Expr::Some(Box::new(self.lower_expr(value, line)?))),
            Expr::Ok(value) => Ok(Expr::Ok(Box::new(self.lower_expr(value, line)?))),
            Expr::Err(value) => Ok(Expr::Err(Box::new(self.lower_expr(value, line)?))),
            Expr::ResultOption(value) => {
                Ok(Expr::ResultOption(Box::new(self.lower_expr(value, line)?)))
            }
            Expr::TryResult(value) => Ok(Expr::TryResult(Box::new(
                self.lower_try_result_value(value, line)?,
            ))),
            Expr::Default { value, fallback } => Ok(Expr::Default {
                value: Box::new(self.lower_expr(value, line)?),
                fallback: Box::new(self.lower_expr(fallback, line)?),
            }),
            Expr::DefaultTry { value, fallback } => Ok(Expr::DefaultTry {
                value: Box::new(self.lower_expr(value, line)?),
                fallback: Box::new(self.lower_expr(fallback, line)?),
            }),
            Expr::Index { name, index } => Ok(Expr::Index {
                name: name.clone(),
                index: Box::new(self.lower_expr(index, line)?),
            }),
            Expr::IndexValue { value, index } => Ok(Expr::IndexValue {
                value: Box::new(self.lower_expr(value, line)?),
                index: Box::new(self.lower_expr(index, line)?),
            }),
            Expr::TupleFieldValue { value, field } => Ok(Expr::TupleFieldValue {
                value: Box::new(self.lower_expr(value, line)?),
                field: *field,
            }),
            Expr::FieldValue { value, field } => {
                if let Some(name) = value.qualified_name() {
                    if self.bindings.contains_key(&associated_const_name(&name, field)) {
                        return Ok(Expr::Ident(associated_const_name(&name, field)));
                    }
                }
                Ok(Expr::FieldValue {
                    value: Box::new(self.lower_expr(value, line)?),
                    field: field.clone(),
                })
            }
            Expr::Slice { name, start, end } => {
                let start = Box::new(self.lower_expr(start, line)?);
                let end = Box::new(self.lower_expr(end, line)?);
                if self
                    .lookup_binding(name)
                    .is_some_and(|binding| self.is_string_like(&binding.ty))
                {
                    Ok(Expr::StringSlice {
                        name: name.clone(),
                        start,
                        end,
                    })
                } else {
                    Ok(Expr::Slice {
                        name: name.clone(),
                        start,
                        end,
                    })
                }
            }
            Expr::ArraySliceValue { value, start, end } => Ok(Expr::ArraySliceValue {
                value: Box::new(self.lower_expr(value, line)?),
                start: Box::new(self.lower_expr(start, line)?),
                end: Box::new(self.lower_expr(end, line)?),
            }),
            Expr::Len(name) => Ok(self.lower_len(name)),
            Expr::ArrayLenValue(value) => {
                Ok(Expr::ArrayLenValue(Box::new(self.lower_expr(value, line)?)))
            }
            Expr::MapLenValue(value) => {
                Ok(Expr::MapLenValue(Box::new(self.lower_expr(value, line)?)))
            }
            Expr::IsEmpty(name) => Ok(self.lower_is_empty(name)),
            Expr::ArrayIsEmptyValue(value) => Ok(Expr::ArrayIsEmptyValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::MapIsEmptyValue(value) => Ok(Expr::MapIsEmptyValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::ArrayFirst(name) => Ok(Expr::ArrayFirst(name.clone())),
            Expr::ArrayFirstValue(value) => Ok(Expr::ArrayFirstValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::ArrayLast(name) => Ok(Expr::ArrayLast(name.clone())),
            Expr::ArrayLastValue(value) => Ok(Expr::ArrayLastValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::ArrayReverse(name) => Ok(Expr::ArrayReverse(name.clone())),
            Expr::ArrayReverseValue(value) => Ok(Expr::ArrayReverseValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::ArraySort(name) => Ok(Expr::ArraySort(name.clone())),
            Expr::ArraySortValue(value) => Ok(Expr::ArraySortValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::ArrayUnique(name) => Ok(Expr::ArrayUnique(name.clone())),
            Expr::ArrayUniqueValue(value) => Ok(Expr::ArrayUniqueValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::ArrayMap { name, mapper } => {
                if self.is_qualified_function_call(name, "map") {
                    return Ok(Expr::Call {
                        name: format!("{name}.map"),
                        args: vec![self.lower_expr(mapper, line)?],
                    });
                }
                if self
                    .lookup_binding(name)
                    .is_some_and(|binding| option_element_type(&binding.ty).is_some())
                {
                    let (element, mapped) = self.check_option_map(name, mapper, line)?;
                    return Ok(Expr::OptionMap {
                        name: name.clone(),
                        mapper: Box::new(self.lower_map_mapper(
                            mapper, &element, &mapped, line,
                        )?),
                    });
                }
                if self
                    .lookup_binding(name)
                    .is_some_and(|binding| result_types(&binding.ty).is_some())
                {
                    let (element, _, mapped) = self.check_result_map(name, mapper, line)?;
                    return Ok(Expr::ResultMap {
                        name: name.clone(),
                        mapper: Box::new(self.lower_map_mapper(
                            mapper, &element, &mapped, line,
                        )?),
                    });
                }
                let (element, mapped) = self.check_array_map(name, mapper, line)?;
                Ok(Expr::ArrayMap {
                    name: name.clone(),
                    mapper: Box::new(self.lower_map_mapper(
                        mapper, &element, &mapped, line,
                    )?),
                })
            }
            Expr::ArrayMapValue { value, mapper } => {
                let value_ty = self.check_expr(value, line)?;
                if option_element_type(&value_ty).is_some() {
                    let (element, mapped) = self.check_option_map_value(value, mapper, line)?;
                    return Ok(Expr::OptionMapValue {
                        value: Box::new(self.lower_expr(value, line)?),
                        mapper: Box::new(self.lower_map_mapper(
                            mapper, &element, &mapped, line,
                        )?),
                    });
                }
                if result_types(&value_ty).is_some() {
                    let (element, _, mapped) =
                        self.check_result_map_value(value, mapper, line)?;
                    return Ok(Expr::ResultMapValue {
                        value: Box::new(self.lower_expr(value, line)?),
                        mapper: Box::new(self.lower_map_mapper(
                            mapper, &element, &mapped, line,
                        )?),
                    });
                }
                let (element, mapped) = self.check_array_map_value(value, mapper, line)?;
                Ok(Expr::ArrayMapValue {
                    value: Box::new(self.lower_expr(value, line)?),
                    mapper: Box::new(self.lower_map_mapper(
                        mapper, &element, &mapped, line,
                    )?),
                })
            }
            Expr::ArrayFilter { name, predicate } => {
                let element = self.check_array_predicate(name, predicate, "filter", line)?;
                Ok(Expr::ArrayFilter {
                    name: name.clone(),
                    predicate: Box::new(self.lower_map_mapper(
                        predicate,
                        &element,
                        &Type::Bool,
                        line,
                    )?),
                })
            }
            Expr::ArrayFilterValue { value, predicate } => {
                let element = self.check_array_predicate_value(value, predicate, "filter", line)?;
                Ok(Expr::ArrayFilterValue {
                    value: Box::new(self.lower_expr(value, line)?),
                    predicate: Box::new(self.lower_map_mapper(
                        predicate,
                        &element,
                        &Type::Bool,
                        line,
                    )?),
                })
            }
            Expr::ArrayFlatMap { name, mapper } => {
                if self.is_qualified_function_call(name, "flatMap") {
                    return Ok(Expr::Call {
                        name: format!("{name}.flatMap"),
                        args: vec![self.lower_expr(mapper, line)?],
                    });
                }
                if self
                    .lookup_binding(name)
                    .is_some_and(|binding| result_types(&binding.ty).is_some())
                {
                    let (element, _, result) = self.check_result_flat_map(name, mapper, line)?;
                    return Ok(Expr::ResultFlatMap {
                        name: name.clone(),
                        mapper: Box::new(self.lower_map_mapper(
                            mapper, &element, &result, line,
                        )?),
                    });
                }
                if self
                    .lookup_binding(name)
                    .is_some_and(|binding| option_element_type(&binding.ty).is_some())
                {
                    let (element, mapped) = self.check_option_flat_map(name, mapper, line)?;
                    return Ok(Expr::OptionFlatMap {
                        name: name.clone(),
                        mapper: Box::new(self.lower_map_mapper(
                            mapper, &element, &mapped, line,
                        )?),
                    });
                }
                let Some(binding) = self.lookup_binding(name) else {
                    return Err(CompileError::new(line, format!("undefined variable `{name}`")));
                };
                let Type::Array(element) = binding.ty else {
                    return Err(CompileError::new(
                        line,
                        format!("type {} has no flatMap method", binding.ty.name()),
                    ));
                };
                let mapped = self.check_array_flat_map_mapper(mapper, &element, line)?;
                Ok(Expr::ArrayFlatMap {
                    name: name.clone(),
                    mapper: Box::new(self.lower_map_mapper(
                        mapper,
                        &element,
                        &Type::Array(Box::new(mapped)),
                        line,
                    )?),
                })
            }
            Expr::ArrayFlatMapValue { value, mapper } => {
                let value_ty = self.check_expr(value, line)?;
                if result_types(&value_ty).is_some() {
                    let (element, _, result) =
                        self.check_result_flat_map_value(value, mapper, line)?;
                    return Ok(Expr::ResultFlatMapValue {
                        value: Box::new(self.lower_expr(value, line)?),
                        mapper: Box::new(self.lower_map_mapper(
                            mapper, &element, &result, line,
                        )?),
                    });
                }
                if option_element_type(&value_ty).is_some() {
                    let (element, mapped) =
                        self.check_option_flat_map_value(value, mapper, line)?;
                    return Ok(Expr::OptionFlatMapValue {
                        value: Box::new(self.lower_expr(value, line)?),
                        mapper: Box::new(self.lower_map_mapper(
                            mapper, &element, &mapped, line,
                        )?),
                    });
                }
                let Type::Array(element) = value_ty else {
                    return Err(CompileError::new(
                        line,
                        format!("type {} has no flatMap method", value_ty.name()),
                    ));
                };
                let mapped = self.check_array_flat_map_mapper(mapper, &element, line)?;
                Ok(Expr::ArrayFlatMapValue {
                    value: Box::new(self.lower_expr(value, line)?),
                    mapper: Box::new(self.lower_map_mapper(
                        mapper,
                        &element,
                        &Type::Array(Box::new(mapped)),
                        line,
                    )?),
                })
            }
            Expr::ArrayFind { name, predicate } => {
                let element = self.check_array_predicate(name, predicate, "find", line)?;
                Ok(Expr::ArrayFind {
                    name: name.clone(),
                    predicate: Box::new(self.lower_map_mapper(
                        predicate,
                        &element,
                        &Type::Bool,
                        line,
                    )?),
                })
            }
            Expr::ArrayFindValue { value, predicate } => {
                let element = self.check_array_predicate_value(value, predicate, "find", line)?;
                Ok(Expr::ArrayFindValue {
                    value: Box::new(self.lower_expr(value, line)?),
                    predicate: Box::new(self.lower_map_mapper(
                        predicate,
                        &element,
                        &Type::Bool,
                        line,
                    )?),
                })
            }
            Expr::ArrayAny { name, predicate } => {
                let element = self.check_array_predicate(name, predicate, "any", line)?;
                Ok(Expr::ArrayAny {
                    name: name.clone(),
                    predicate: Box::new(self.lower_map_mapper(
                        predicate,
                        &element,
                        &Type::Bool,
                        line,
                    )?),
                })
            }
            Expr::ArrayAnyValue { value, predicate } => {
                let element = self.check_array_predicate_value(value, predicate, "any", line)?;
                Ok(Expr::ArrayAnyValue {
                    value: Box::new(self.lower_expr(value, line)?),
                    predicate: Box::new(self.lower_map_mapper(
                        predicate,
                        &element,
                        &Type::Bool,
                        line,
                    )?),
                })
            }
            Expr::ArrayAll { name, predicate } => {
                let element = self.check_array_predicate(name, predicate, "all", line)?;
                Ok(Expr::ArrayAll {
                    name: name.clone(),
                    predicate: Box::new(self.lower_map_mapper(
                        predicate,
                        &element,
                        &Type::Bool,
                        line,
                    )?),
                })
            }
            Expr::ArrayAllValue { value, predicate } => {
                let element = self.check_array_predicate_value(value, predicate, "all", line)?;
                Ok(Expr::ArrayAllValue {
                    value: Box::new(self.lower_expr(value, line)?),
                    predicate: Box::new(self.lower_map_mapper(
                        predicate,
                        &element,
                        &Type::Bool,
                        line,
                    )?),
                })
            }
            Expr::ArrayFold {
                name,
                initial,
                reducer,
            } => {
                let Some(binding) = self.lookup_binding(name) else {
                    return Err(CompileError::new(line, format!("undefined variable `{name}`")));
                };
                let Type::Array(element) = binding.ty else {
                    return Err(CompileError::new(
                        line,
                        format!("type {} has no fold method", binding.ty.name()),
                    ));
                };
                let accumulator = self.check_expr(initial, line)?;
                Ok(Expr::ArrayFold {
                    name: name.clone(),
                    initial: Box::new(self.lower_expr(initial, line)?),
                    reducer: Box::new(self.lower_fold_reducer(
                        reducer,
                        &accumulator,
                        &element,
                        line,
                    )?),
                })
            }
            Expr::ArrayFoldValue {
                value,
                initial,
                reducer,
            } => {
                let value_ty = self.check_expr(value, line)?;
                let Type::Array(element) = value_ty else {
                    return Err(CompileError::new(
                        line,
                        format!("type {} has no fold method", value_ty.name()),
                    ));
                };
                let accumulator = self.check_expr(initial, line)?;
                Ok(Expr::ArrayFoldValue {
                    value: Box::new(self.lower_expr(value, line)?),
                    initial: Box::new(self.lower_expr(initial, line)?),
                    reducer: Box::new(self.lower_fold_reducer(
                        reducer,
                        &accumulator,
                        &element,
                        line,
                    )?),
                })
            }
            Expr::OptionMap { name, mapper } => {
                let (element, mapped) = self.check_option_map(name, mapper, line)?;
                Ok(Expr::OptionMap {
                    name: name.clone(),
                    mapper: Box::new(self.lower_map_mapper(
                        mapper, &element, &mapped, line,
                    )?),
                })
            }
            Expr::OptionMapValue { value, mapper } => {
                let (element, mapped) = self.check_option_map_value(value, mapper, line)?;
                Ok(Expr::OptionMapValue {
                    value: Box::new(self.lower_expr(value, line)?),
                    mapper: Box::new(self.lower_map_mapper(
                        mapper, &element, &mapped, line,
                    )?),
                })
            }
            Expr::ResultMap { name, mapper } => {
                let (element, _, mapped) = self.check_result_map(name, mapper, line)?;
                Ok(Expr::ResultMap {
                    name: name.clone(),
                    mapper: Box::new(self.lower_map_mapper(
                        mapper, &element, &mapped, line,
                    )?),
                })
            }
            Expr::ResultMapValue { value, mapper } => {
                let (element, _, mapped) =
                    self.check_result_map_value(value, mapper, line)?;
                Ok(Expr::ResultMapValue {
                    value: Box::new(self.lower_expr(value, line)?),
                    mapper: Box::new(self.lower_map_mapper(
                        mapper, &element, &mapped, line,
                    )?),
                })
            }
            Expr::OptionFlatMap { name, mapper } => {
                if self.is_qualified_function_call(name, "flatMap") {
                    return Ok(Expr::Call {
                        name: format!("{name}.flatMap"),
                        args: vec![self.lower_expr(mapper, line)?],
                    });
                }
                if self
                    .lookup_binding(name)
                    .is_some_and(|binding| result_types(&binding.ty).is_some())
                {
                    let (element, mapper_result, _) =
                        self.check_result_flat_map(name, mapper, line)?;
                    return Ok(Expr::ResultFlatMap {
                        name: name.clone(),
                        mapper: Box::new(self.lower_map_mapper(
                            mapper,
                            &element,
                            &mapper_result,
                            line,
                        )?),
                    });
                }
                let (element, mapped) = self.check_option_flat_map(name, mapper, line)?;
                Ok(Expr::OptionFlatMap {
                    name: name.clone(),
                    mapper: Box::new(self.lower_map_mapper(
                        mapper, &element, &mapped, line,
                    )?),
                })
            }
            Expr::OptionFlatMapValue { value, mapper } => {
                let value_ty = self.check_expr(value, line)?;
                if result_types(&value_ty).is_some() {
                    let (element, mapper_result, _) =
                        self.check_result_flat_map_value(value, mapper, line)?;
                    return Ok(Expr::ResultFlatMapValue {
                        value: Box::new(self.lower_expr(value, line)?),
                        mapper: Box::new(self.lower_map_mapper(
                            mapper,
                            &element,
                            &mapper_result,
                            line,
                        )?),
                    });
                }
                let (element, mapped) = self.check_option_flat_map_value(value, mapper, line)?;
                Ok(Expr::OptionFlatMapValue {
                    value: Box::new(self.lower_expr(value, line)?),
                    mapper: Box::new(self.lower_map_mapper(
                        mapper, &element, &mapped, line,
                    )?),
                })
            }
            Expr::ResultFlatMap { name, mapper } => {
                let (element, mapper_result, _) =
                    self.check_result_flat_map(name, mapper, line)?;
                Ok(Expr::ResultFlatMap {
                    name: name.clone(),
                    mapper: Box::new(self.lower_map_mapper(
                        mapper,
                        &element,
                        &mapper_result,
                        line,
                    )?),
                })
            }
            Expr::ResultFlatMapValue { value, mapper } => {
                let (element, mapper_result, _) =
                    self.check_result_flat_map_value(value, mapper, line)?;
                Ok(Expr::ResultFlatMapValue {
                    value: Box::new(self.lower_expr(value, line)?),
                    mapper: Box::new(self.lower_map_mapper(
                        mapper,
                        &element,
                        &mapper_result,
                        line,
                    )?),
                })
            }
            Expr::OptionAp { name, value } => {
                if self.is_qualified_function_call(name, "ap") {
                    return Ok(Expr::Call {
                        name: format!("{name}.ap"),
                        args: vec![self.lower_expr(value, line)?],
                    });
                }
                if self
                    .lookup_binding(name)
                    .is_some_and(|binding| result_types(&binding.ty).is_some())
                {
                    self.check_result_ap(name, value, line)?;
                    return Ok(Expr::ResultAp {
                        name: name.clone(),
                        value: Box::new(self.lower_expr(value, line)?),
                    });
                }
                self.check_option_ap(name, value, line)?;
                Ok(Expr::OptionAp {
                    name: name.clone(),
                    value: Box::new(self.lower_expr(value, line)?),
                })
            }
            Expr::OptionApValue { function, value } => {
                let function_ty = self.check_expr(function, line)?;
                if result_types(&function_ty).is_some() {
                    self.check_result_ap_value(function, value, line)?;
                    return Ok(Expr::ResultApValue {
                        function: Box::new(self.lower_expr(function, line)?),
                        value: Box::new(self.lower_expr(value, line)?),
                    });
                }
                self.check_option_ap_value(function, value, line)?;
                Ok(Expr::OptionApValue {
                    function: Box::new(self.lower_expr(function, line)?),
                    value: Box::new(self.lower_expr(value, line)?),
                })
            }
            Expr::ResultAp { name, value } => {
                self.check_result_ap(name, value, line)?;
                Ok(Expr::ResultAp {
                    name: name.clone(),
                    value: Box::new(self.lower_expr(value, line)?),
                })
            }
            Expr::ResultApValue { function, value } => {
                self.check_result_ap_value(function, value, line)?;
                Ok(Expr::ResultApValue {
                    function: Box::new(self.lower_expr(function, line)?),
                    value: Box::new(self.lower_expr(value, line)?),
                })
            }
            Expr::OptionOrElse { name, fallback } => {
                if self.is_qualified_function_call(name, "orElse") {
                    return Ok(Expr::Call {
                        name: format!("{name}.orElse"),
                        args: vec![self.lower_expr(fallback, line)?],
                    });
                }
                self.check_option_or_else(name, fallback, line)?;
                Ok(Expr::OptionOrElse {
                    name: name.clone(),
                    fallback: Box::new(self.lower_expr(fallback, line)?),
                })
            }
            Expr::OptionOrElseValue { value, fallback } => {
                self.check_option_or_else_value(value, fallback, line)?;
                Ok(Expr::OptionOrElseValue {
                    value: Box::new(self.lower_expr(value, line)?),
                    fallback: Box::new(self.lower_expr(fallback, line)?),
                })
            }
            Expr::OptionOrElseTry { value, fallback } => {
                self.check_option_or_else_try(value, fallback, line)?;
                Ok(Expr::OptionOrElseTry {
                    value: Box::new(self.lower_expr(value, line)?),
                    fallback: Box::new(self.lower_expr(fallback, line)?),
                })
            }
            Expr::ArrayTake { name, count } => Ok(Expr::ArrayTake {
                name: name.clone(),
                count: Box::new(self.lower_expr(count, line)?),
            }),
            Expr::ArrayTakeValue { value, count } => Ok(Expr::ArrayTakeValue {
                value: Box::new(self.lower_expr(value, line)?),
                count: Box::new(self.lower_expr(count, line)?),
            }),
            Expr::ArrayDrop { name, count } => Ok(Expr::ArrayDrop {
                name: name.clone(),
                count: Box::new(self.lower_expr(count, line)?),
            }),
            Expr::ArrayDropValue { value, count } => Ok(Expr::ArrayDropValue {
                value: Box::new(self.lower_expr(value, line)?),
                count: Box::new(self.lower_expr(count, line)?),
            }),
            Expr::Join { name, separator } => Ok(Expr::Join {
                name: name.clone(),
                separator: Box::new(self.lower_expr(separator, line)?),
            }),
            Expr::JoinValue { value, separator } => Ok(Expr::JoinValue {
                value: Box::new(self.lower_expr(value, line)?),
                separator: Box::new(self.lower_expr(separator, line)?),
            }),
            Expr::ArrayPush { name, value } => Ok(Expr::ArrayPush {
                name: name.clone(),
                value: Box::new(self.lower_expr(value, line)?),
            }),
            Expr::ArrayPop { name } => Ok(Expr::ArrayPop { name: name.clone() }),
            Expr::MapSet { name, key, value } => {
                let key = self.lower_expr(key, line)?;
                let value = self.lower_expr(value, line)?;
                if self.is_qualified_function_call(name, "set") {
                    Ok(Expr::Call {
                        name: format!("{name}.set"),
                        args: vec![key, value],
                    })
                } else {
                    Ok(Expr::MapSet {
                        name: name.clone(),
                        key: Box::new(key),
                        value: Box::new(value),
                    })
                }
            }
            Expr::MapRemove { name, key } => {
                let key = self.lower_expr(key, line)?;
                if self.is_qualified_function_call(name, "remove") {
                    Ok(Expr::Call {
                        name: format!("{name}.remove"),
                        args: vec![key],
                    })
                } else {
                    Ok(Expr::MapRemove {
                        name: name.clone(),
                        key: Box::new(key),
                    })
                }
            }
            Expr::ArrayContains { name, value } => Ok(Expr::ArrayContains {
                name: name.clone(),
                value: Box::new(self.lower_expr(value, line)?),
            }),
            Expr::ArrayContainsValue { value, item } => Ok(Expr::ArrayContainsValue {
                value: Box::new(self.lower_expr(value, line)?),
                item: Box::new(self.lower_expr(item, line)?),
            }),
            Expr::ArrayIndexOf { name, value } => {
                let value = Box::new(self.lower_expr(value, line)?);
                if self
                    .lookup_binding(name)
                    .is_some_and(|binding| self.is_string_like(&binding.ty))
                {
                    Ok(Expr::StringIndexOf {
                        name: name.clone(),
                        needle: value,
                    })
                } else {
                    Ok(Expr::ArrayIndexOf {
                        name: name.clone(),
                        value,
                    })
                }
            }
            Expr::ArrayIndexOfValue { value, item } => Ok(Expr::ArrayIndexOfValue {
                value: Box::new(self.lower_expr(value, line)?),
                item: Box::new(self.lower_expr(item, line)?),
            }),
            Expr::MapKeys(name) => Ok(Expr::MapKeys(name.clone())),
            Expr::MapKeysValue(value) => {
                Ok(Expr::MapKeysValue(Box::new(self.lower_expr(value, line)?)))
            }
            Expr::MapValues(name) => Ok(Expr::MapValues(name.clone())),
            Expr::MapValuesValue(value) => Ok(Expr::MapValuesValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::MapHas { name, key } => Ok(Expr::MapHas {
                name: name.clone(),
                key: Box::new(self.lower_expr(key, line)?),
            }),
            Expr::MapHasValue { value, key } => Ok(Expr::MapHasValue {
                value: Box::new(self.lower_expr(value, line)?),
                key: Box::new(self.lower_expr(key, line)?),
            }),
            Expr::StringContains { name, needle } => {
                let needle = Box::new(self.lower_expr(needle, line)?);
                if matches!(
                    self.lookup_binding(name).map(|binding| binding.ty),
                    Some(Type::Array(_))
                ) {
                    Ok(Expr::ArrayContains {
                        name: name.clone(),
                        value: needle,
                    })
                } else {
                    Ok(Expr::StringContains {
                        name: name.clone(),
                        needle,
                    })
                }
            }
            Expr::StringContainsValue { value, needle } => Ok(Expr::StringContainsValue {
                value: Box::new(self.lower_expr(value, line)?),
                needle: Box::new(self.lower_expr(needle, line)?),
            }),
            Expr::StringIndexOf { name, needle } => Ok(Expr::StringIndexOf {
                name: name.clone(),
                needle: Box::new(self.lower_expr(needle, line)?),
            }),
            Expr::StringIndexOfValue { value, needle } => Ok(Expr::StringIndexOfValue {
                value: Box::new(self.lower_expr(value, line)?),
                needle: Box::new(self.lower_expr(needle, line)?),
            }),
            Expr::StringStartsWith { name, prefix } => Ok(Expr::StringStartsWith {
                name: name.clone(),
                prefix: Box::new(self.lower_expr(prefix, line)?),
            }),
            Expr::StringStartsWithValue { value, prefix } => Ok(Expr::StringStartsWithValue {
                value: Box::new(self.lower_expr(value, line)?),
                prefix: Box::new(self.lower_expr(prefix, line)?),
            }),
            Expr::StringEndsWith { name, suffix } => Ok(Expr::StringEndsWith {
                name: name.clone(),
                suffix: Box::new(self.lower_expr(suffix, line)?),
            }),
            Expr::StringEndsWithValue { value, suffix } => Ok(Expr::StringEndsWithValue {
                value: Box::new(self.lower_expr(value, line)?),
                suffix: Box::new(self.lower_expr(suffix, line)?),
            }),
            Expr::StringLen(name) => Ok(Expr::StringLen(name.clone())),
            Expr::StringLenValue(value) => Ok(Expr::StringLenValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::StringIsEmpty(name) => Ok(Expr::StringIsEmpty(name.clone())),
            Expr::StringIsEmptyValue(value) => Ok(Expr::StringIsEmptyValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::StringSlice { name, start, end } => Ok(Expr::StringSlice {
                name: name.clone(),
                start: Box::new(self.lower_expr(start, line)?),
                end: Box::new(self.lower_expr(end, line)?),
            }),
            Expr::StringSliceValue { value, start, end } => Ok(Expr::StringSliceValue {
                value: Box::new(self.lower_expr(value, line)?),
                start: Box::new(self.lower_expr(start, line)?),
                end: Box::new(self.lower_expr(end, line)?),
            }),
            Expr::StringTrim(name) => Ok(Expr::StringTrim(name.clone())),
            Expr::StringTrimValue(value) => Ok(Expr::StringTrimValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::StringTrimStart(name) => Ok(Expr::StringTrimStart(name.clone())),
            Expr::StringTrimStartValue(value) => Ok(Expr::StringTrimStartValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::StringTrimEnd(name) => Ok(Expr::StringTrimEnd(name.clone())),
            Expr::StringTrimEndValue(value) => Ok(Expr::StringTrimEndValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::StringToUpper(name) => Ok(Expr::StringToUpper(name.clone())),
            Expr::StringToUpperValue(value) => Ok(Expr::StringToUpperValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::StringToLower(name) => Ok(Expr::StringToLower(name.clone())),
            Expr::StringToLowerValue(value) => Ok(Expr::StringToLowerValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::StringRepeat { name, count } => Ok(Expr::StringRepeat {
                name: name.clone(),
                count: Box::new(self.lower_expr(count, line)?),
            }),
            Expr::StringRepeatValue { value, count } => Ok(Expr::StringRepeatValue {
                value: Box::new(self.lower_expr(value, line)?),
                count: Box::new(self.lower_expr(count, line)?),
            }),
            Expr::StringSplit { name, separator } => Ok(Expr::StringSplit {
                name: name.clone(),
                separator: Box::new(self.lower_expr(separator, line)?),
            }),
            Expr::StringSplitValue { value, separator } => Ok(Expr::StringSplitValue {
                value: Box::new(self.lower_expr(value, line)?),
                separator: Box::new(self.lower_expr(separator, line)?),
            }),
            Expr::StringReplace { name, from, to } => Ok(Expr::StringReplace {
                name: name.clone(),
                from: Box::new(self.lower_expr(from, line)?),
                to: Box::new(self.lower_expr(to, line)?),
            }),
            Expr::StringReplaceValue { value, from, to } => Ok(Expr::StringReplaceValue {
                value: Box::new(self.lower_expr(value, line)?),
                from: Box::new(self.lower_expr(from, line)?),
                to: Box::new(self.lower_expr(to, line)?),
            }),
            Expr::PathBasename(name) => Ok(Expr::PathBasename(name.clone())),
            Expr::PathBasenameValue(value) => Ok(Expr::PathBasenameValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::PathDirname(name) => Ok(Expr::PathDirname(name.clone())),
            Expr::PathDirnameValue(value) => Ok(Expr::PathDirnameValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::PathStem(name) => Ok(Expr::PathStem(name.clone())),
            Expr::PathStemValue(value) => {
                Ok(Expr::PathStemValue(Box::new(self.lower_expr(value, line)?)))
            }
            Expr::PathExtname(name) => Ok(Expr::PathExtname(name.clone())),
            Expr::PathExtnameValue(value) => Ok(Expr::PathExtnameValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::PathIsAbsolute(name) => Ok(Expr::PathIsAbsolute(name.clone())),
            Expr::PathIsAbsoluteValue(value) => Ok(Expr::PathIsAbsoluteValue(Box::new(
                self.lower_expr(value, line)?,
            ))),
            Expr::PathExists(path) => Ok(Expr::PathExists(Box::new(self.lower_expr(path, line)?))),
            Expr::ProcessEnv { name } => Ok(Expr::ProcessEnv {
                name: Box::new(self.lower_expr(name, line)?),
            }),
            Expr::FsIsFile { path } => Ok(Expr::FsIsFile {
                path: Box::new(self.lower_expr(path, line)?),
            }),
            Expr::FsIsDir { path } => Ok(Expr::FsIsDir {
                path: Box::new(self.lower_expr(path, line)?),
            }),
            Expr::FsSize { path } => Ok(Expr::FsSize {
                path: Box::new(self.lower_expr(path, line)?),
            }),
            Expr::FsReadLines { path } => Ok(Expr::FsReadLines {
                path: Box::new(self.lower_expr(path, line)?),
            }),
            Expr::FsList { path } => Ok(Expr::FsList {
                path: Box::new(self.lower_expr(path, line)?),
            }),
            Expr::FsWriteLines { path, lines } => Ok(Expr::FsWriteLines {
                path: Box::new(self.lower_expr(path, line)?),
                lines: Box::new(self.lower_expr(lines, line)?),
            }),
            Expr::FsAppendLines { path, lines } => Ok(Expr::FsAppendLines {
                path: Box::new(self.lower_expr(path, line)?),
                lines: Box::new(self.lower_expr(lines, line)?),
            }),
            Expr::JsonParse { value } => Ok(Expr::JsonParse {
                value: Box::new(self.lower_expr(value, line)?),
            }),
            Expr::JsonStringify { name } => Ok(Expr::JsonStringify { name: name.clone() }),
            Expr::JsonStringifyValue { value } => Ok(Expr::JsonStringifyValue {
                value: Box::new(self.lower_expr(value, line)?),
            }),
            Expr::NewtypeCtor { name, value } if self.constructor_overrides.contains(name) => {
                Ok(Expr::Call {
                    name: name.clone(),
                    args: vec![self.lower_expr(value, line)?],
                })
            }
            Expr::NewtypeCtor { name, value } if self.variants.contains_key(name) => {
                Ok(Expr::Variant {
                    name: name.clone(),
                    args: vec![self.lower_expr(value, line)?],
                    field_types: self.instantiate_variant_fields(
                        name,
                        std::slice::from_ref(value.as_ref()),
                        line,
                    )?,
                })
            }
            Expr::NewtypeCtor { name, value } => Ok(Expr::NewtypeCtor {
                name: name.clone(),
                value: Box::new(self.lower_expr(value, line)?),
            }),
            Expr::Variant {
                name,
                args,
                field_types,
            } => Ok(Expr::Variant {
                name: name.clone(),
                args: args
                    .iter()
                    .map(|arg| self.lower_expr(arg, line))
                    .collect::<Result<Vec<_>, _>>()?,
                field_types: field_types.clone(),
            }),
            Expr::Cast { expr, ty } => Ok(Expr::Cast {
                expr: Box::new(self.lower_expr(expr, line)?),
                ty: ty.clone(),
            }),
            Expr::Lambda { .. } => Err(CompileError::new(
                line,
                "lambda type cannot be inferred; provide a function type annotation or pass it to a typed function parameter".to_string(),
            )),
            Expr::Closure { name, captures } => Ok(Expr::Closure {
                name: name.clone(),
                captures: captures.clone(),
            }),
            Expr::Do { steps, result } => {
                let desugared = self.desugar_do(steps, result, line)?;
                self.lower_expr(&desugared, line)
            }
            Expr::LetIn {
                name,
                annotation,
                value,
                body,
            } => {
                let value_ty = self.binding_expr_type(annotation.as_ref(), value, line)?;
                let binding_ty =
                    self.check_annotation(annotation.clone(), value_ty, value, line)?;
                let value = self.lower_expr_expected(value, &binding_ty, line)?;
                let mut body_checker = self.clone();
                body_checker.define(name, binding_ty.clone(), false, line)?;
                Ok(Expr::LetIn {
                    name: name.clone(),
                    annotation: Some(binding_ty),
                    value: Box::new(value),
                    body: Box::new(body_checker.lower_expr(body, line)?),
                })
            }
            Expr::IfElse {
                condition,
                then_expr,
                else_expr,
            } => Ok(Expr::IfElse {
                condition: Box::new(self.lower_expr(condition, line)?),
                then_expr: Box::new(self.lower_expr(then_expr, line)?),
                else_expr: Box::new(self.lower_expr(else_expr, line)?),
            }),
            Expr::Match { value, arms } => {
                let value_ty =
                    if matches!(value.as_ref(), Expr::Command { .. } | Expr::Pipeline { .. }) {
                        command_result_type()
                    } else {
                        self.check_expr(value, line)?
                    };
                let mut lowered_arms = Vec::with_capacity(arms.len());
                for arm in arms {
                    let mut arm_checker = self.clone();
                    if let Some(pattern) = &arm.pattern {
                        for (name, ty) in
                            self.check_match_pattern(pattern, value, &value_ty, line)?
                        {
                            arm_checker.define(&name, ty, false, line)?;
                        }
                    }
                    lowered_arms.push(MatchArm {
                        pattern: arm
                            .pattern
                            .as_ref()
                            .map(|pattern| self.lower_match_pattern(pattern, &value_ty, line))
                            .transpose()?,
                        guard: arm
                            .guard
                            .as_ref()
                            .map(|guard| arm_checker.lower_expr(guard, line))
                            .transpose()?,
                        expr: arm_checker.lower_expr(&arm.expr, line)?,
                    });
                }
                Ok(Expr::Match {
                    value: Box::new(self.lower_expr(value, line)?),
                    arms: lowered_arms,
                })
            }
            Expr::MatchGuardResult(value) => {
                Ok(Expr::MatchGuardResult(Box::new(self.lower_expr(value, line)?)))
            }
            Expr::Not(expr) => Ok(Expr::Not(Box::new(self.lower_expr(expr, line)?))),
            Expr::BitNot(expr) => Ok(Expr::BitNot(Box::new(self.lower_expr(expr, line)?))),
            Expr::Binary { left, op, right } => Ok(Expr::Binary {
                left: Box::new(self.lower_expr(left, line)?),
                op: *op,
                right: Box::new(self.lower_expr(right, line)?),
            }),
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
            Expr::Int(_)
            | Expr::Float(_)
            | Expr::Bool(_)
            | Expr::String(_)
            | Expr::RawString(_)
            | Expr::Unit
            | Expr::None
            | Expr::Command { .. }
            | Expr::CommandResult { .. }
            | Expr::AsyncCommand(_)
            | Expr::Await(_)
            | Expr::Pipeline { .. }
            | Expr::TryPipeline { .. }
            | Expr::PipelineResult { .. }
            | Expr::HasCommand(_)
            | Expr::TupleField { .. }
            | Expr::Value(_)
            | Expr::EnvDefault { .. }
            | Expr::Env(_)
            | Expr::ProcessArgs
            | Expr::CliParse
            | Expr::Ident(_) => Ok(expr.clone()),
            Expr::ArrayPattern { patterns, rest } => Ok(Expr::ArrayPattern {
                patterns: patterns
                    .iter()
                    .map(|value| self.lower_expr(value, line))
                    .collect::<Result<Vec<_>, _>>()?,
                rest: rest.clone(),
            }),
            Expr::AliasPattern { pattern, alias } => Ok(Expr::AliasPattern {
                pattern: Box::new(self.lower_expr(pattern, line)?),
                alias: alias.clone(),
            }),
            Expr::Field { name, field } if !self.bindings.contains_key(name) => {
                let qualified = format!("{name}.{field}");
                if self.functions.contains_key(&qualified) {
                    Ok(Expr::Ident(qualified))
                } else if self
                    .bindings
                    .contains_key(&associated_const_name(name, field))
                {
                    Ok(Expr::Ident(associated_const_name(name, field)))
                } else if self.bindings.contains_key(&module_binding_name(name, field)) {
                    Ok(Expr::Ident(module_binding_name(name, field)))
                } else {
                    Ok(expr.clone())
                }
            }
            Expr::Field { .. } => Ok(expr.clone()),
        }
    }

    pub(super) fn lower_call_args(
        &self,
        name: &str,
        args: &[Expr],
        line: usize,
    ) -> Result<Vec<Expr>, CompileError> {
        if let Some(sig) = self.functions.get(name) {
            let args = self.normalize_call_args(name, args, sig, line)?;
            return args
                .iter()
                .enumerate()
                .map(|(index, arg)| {
                    sig.params
                        .get(index)
                        .map(|param| self.lower_expr_expected(arg, &param.ty, line))
                        .unwrap_or_else(|| self.lower_expr(arg, line))
                })
                .collect();
        }
        if args.iter().any(|arg| matches!(arg, Expr::NamedArg { .. })) {
            return Err(CompileError::new(
                line,
                format!("function value `{name}` does not support named arguments"),
            ));
        }
        if let Some(Binding {
            ty: Type::Function(params, _),
            ..
        }) = self.bindings.get(name)
        {
            return args
                .iter()
                .enumerate()
                .map(|(index, arg)| {
                    params
                        .get(index)
                        .map(|expected| self.lower_expr_expected(arg, expected, line))
                        .unwrap_or_else(|| self.lower_expr(arg, line))
                })
                .collect();
        }
        args.iter().map(|arg| self.lower_expr(arg, line)).collect()
    }
}
