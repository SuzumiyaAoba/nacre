use super::type_utils::{option_element_type, result_types};
use super::validation::unsafe_execution_error;
use super::{FunctionSig, TypeChecker};
use crate::{BinaryOp, CompileError, Expr, Type};

impl TypeChecker {
    pub(super) fn check_expr_expected(
        &self,
        expr: &Expr,
        expected: &Type,
        line: usize,
    ) -> Result<Type, CompileError> {
        if let Expr::Lambda { params, body } = expr {
            return self.check_lambda(params, body, expected, line);
        }
        if let Expr::NewtypeCtor { name, value } = expr {
            if self.generic_newtypes.contains_key(name) {
                return self.check_newtype_ctor_expected(name, value, expected, line);
            }
        }
        if let Expr::Call { name, args } = expr {
            if self.variants.contains_key(name) {
                return self.check_variant_expected(name, args, expected, line);
            }
        }
        if let Expr::Ident(name) = expr {
            if self
                .variants
                .get(name)
                .is_some_and(|variant| variant.fields.is_empty())
            {
                return self.check_variant_expected(name, &[], expected, line);
            }
        }
        if let Expr::NewtypeCtor { name, value } = expr {
            if self.variants.contains_key(name) {
                return self.check_variant_expected(
                    name,
                    std::slice::from_ref(value.as_ref()),
                    expected,
                    line,
                );
            }
        }
        self.check_expr(expr, line)
    }

    pub(super) fn lower_expr_expected(
        &self,
        expr: &Expr,
        expected: &Type,
        line: usize,
    ) -> Result<Expr, CompileError> {
        if let Expr::Lambda { params, body } = expr {
            return self.lower_lambda(params, body, expected, line);
        }
        self.lower_binding_expr(expr, expected, line)
    }

    pub(super) fn check_expr(&self, expr: &Expr, line: usize) -> Result<Type, CompileError> {
        match expr {
            Expr::Int(_) => Ok(Type::Int),
            Expr::Float(_) => Ok(Type::Float),
            Expr::Bool(_) => Ok(Type::Bool),
            Expr::Unit => Ok(Type::Unit),
            Expr::Some(value) => Ok(Type::Applied(
                "Option".to_string(),
                vec![self.check_expr(value, line)?],
            )),
            Expr::None => Ok(Type::Applied("Option".to_string(), vec![Type::Unit])),
            Expr::Ok(value) => Ok(Type::Applied(
                "Result".to_string(),
                vec![self.check_expr(value, line)?, Type::Unit],
            )),
            Expr::Err(value) => Ok(Type::Applied(
                "Result".to_string(),
                vec![Type::Unit, self.check_expr(value, line)?],
            )),
            Expr::ResultOption(value) => self.check_result_option(value, line),
            Expr::TryResult(_) => Err(CompileError::new(
                line,
                "try propagation requires a Result-returning function or lambda".to_string(),
            )),
            Expr::Default { value, fallback } => self.check_default(value, fallback, line),
            Expr::DefaultTry { value, fallback } => {
                self.check_default_try(value, fallback, line)
            }
            Expr::String(value) => {
                self.check_string_interpolations(value, line)?;
                Ok(Type::String)
            }
            Expr::AllowedCommand {
                group,
                command,
                args,
                result,
                ..
            } => self.check_allowed_command(group, command, args, *result, line),
            Expr::Async(value) => self.check_async(value, line),
            Expr::HasCommand(_) => Err(CompileError::new(
                line,
                "`hasCommand` is disabled; declare executables in the external policy and call them through `run.<group>.<command>(...)`".to_string(),
            )),
            Expr::PathExists(path) => {
                self.require_read_access("pathExists", line)?;
                self.check_path_exists(path, line)
            }
            Expr::ProcessEnv { name } => self.check_process_env(name, line),
            Expr::JsonParse { value } => self.check_json_parse(value, line),
            Expr::JsonStringify { name } => self.check_json_stringify(name, line),
            Expr::JsonStringifyValue { value } => self.check_json_stringify_value(value, line),
            Expr::Range { start, end, .. } => {
                let start_ty = self.check_expr(start, line)?;
                if !self.is_assignable(&Type::Int, &start_ty, start) {
                    return Err(CompileError::new(
                        line,
                        format!("type mismatch: expected Int, found {}", start_ty.name()),
                    ));
                }
                let end_ty = self.check_expr(end, line)?;
                if !self.is_assignable(&Type::Int, &end_ty, end) {
                    return Err(CompileError::new(
                        line,
                        format!("type mismatch: expected Int, found {}", end_ty.name()),
                    ));
                }
                Ok(Type::Array(Box::new(Type::Int)))
            }
            Expr::Array(values) => self.check_array(values, line),
            Expr::Map(entries) => self.check_map(entries, line),
            Expr::Record(fields) => self.check_record(fields, line),
            Expr::RecordPattern(_) => Err(CompileError::new(
                line,
                "record patterns are only valid in match arms".to_string(),
            )),
            Expr::ArrayPattern { .. } => Err(CompileError::new(
                line,
                "array patterns are only valid in match arms".to_string(),
            )),
            Expr::AliasPattern { .. } => Err(CompileError::new(
                line,
                "alias patterns are only valid in match arms".to_string(),
            )),
            Expr::Tuple(values) => self.check_tuple(values, line),
            Expr::Index { name, index } => self.check_index(name, index, line),
            Expr::IndexValue { value, index } => self.check_index_value(value, index, line),
            Expr::Slice { name, start, end } | Expr::StringSlice { name, start, end } => {
                self.check_slice(name, start, end, line)
            }
            Expr::ArraySliceValue { value, start, end } => {
                self.check_array_slice_value(value, start, end, line)
            }
            Expr::StringSliceValue { value, start, end } => {
                self.check_string_slice_value(value, start, end, line)
            }
            Expr::TupleField { name, field } => self.check_tuple_field(name, *field, line),
            Expr::TupleFieldValue { value, field } => {
                self.check_tuple_field_value(value, *field, line)
            }
            Expr::Field { name, field } => self.check_field(name, field, line),
            Expr::FieldValue { value, field } => self.check_field_value(value, field, line),
            Expr::NewtypeCtor { name, value } if self.variants.contains_key(name) => {
                self.check_variant(name, std::slice::from_ref(value.as_ref()), line)
            }
            Expr::NewtypeCtor { name, value } => self.check_newtype_ctor(name, value, line),
            Expr::Variant { name, args, .. } => self.check_variant(name, args, line),
            Expr::Cast { expr, ty } => self.check_cast(expr, ty, line),
            Expr::Lambda { .. } => Err(CompileError::new(
                line,
                "lambda type cannot be inferred; provide a function type annotation or pass it to a typed function parameter".to_string(),
            )),
            Expr::Closure { .. } => Err(CompileError::new(
                line,
                "internal closure expression cannot appear in source".to_string(),
            )),
            Expr::Do { steps, result } => {
                let desugared = self.desugar_do(steps, result, line)?;
                self.check_expr(&desugared, line)
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
                let mut body_checker = self.clone();
                body_checker.define(name, binding_ty, false, line)?;
                body_checker.check_expr(body, line)
            }
            Expr::Call { name, args } if self.variants.contains_key(name) => {
                self.check_variant(name, args, line)
            }
            Expr::Call { name, args } => self.check_call(name, args, line),
            Expr::NamedArg { .. } => Err(CompileError::new(
                line,
                "named arguments are only valid inside function calls".to_string(),
            )),
            Expr::Value(name) => self.check_value_access(name, line),
            Expr::Len(name) | Expr::StringLen(name) => self.check_len(name, line),
            Expr::ArrayLenValue(value) => self
                .check_array_value(value, "len", line)
                .map(|_| Type::Int),
            Expr::MapLenValue(value) => self.check_map_value(value, "len", line).map(|_| Type::Int),
            Expr::StringLenValue(value) => self
                .check_string_transform_value(value, "len", line)
                .map(|_| Type::Int),
            Expr::IsEmpty(name) | Expr::StringIsEmpty(name) => self.check_is_empty(name, line),
            Expr::ArrayIsEmptyValue(value) => self
                .check_array_value(value, "isEmpty", line)
                .map(|_| Type::Bool),
            Expr::MapIsEmptyValue(value) => self
                .check_map_value(value, "isEmpty", line)
                .map(|_| Type::Bool),
            Expr::StringIsEmptyValue(value) => self
                .check_string_transform_value(value, "isEmpty", line)
                .map(|_| Type::Bool),
            Expr::ArrayFirst(name) => self.check_array_edge(name, "first", line),
            Expr::ArrayFirstValue(value) => self.check_array_edge_value(value, "first", line),
            Expr::ArrayLast(name) => self.check_array_edge(name, "last", line),
            Expr::ArrayLastValue(value) => self.check_array_edge_value(value, "last", line),
            Expr::ArrayReverse(name) => self.check_array_reverse(name, line),
            Expr::ArrayReverseValue(value) => {
                self.check_array_transform_value(value, "reverse", line)
            }
            Expr::ArraySort(name) => self.check_array_sort(name, line),
            Expr::ArraySortValue(value) => self.check_array_sort_value(value, line),
            Expr::ArrayUnique(name) => self.check_array_unique(name, line),
            Expr::ArrayUniqueValue(value) => {
                self.check_array_transform_value(value, "unique", line)
            }
            Expr::ArrayMap { name, mapper } => {
                if self.is_qualified_function_call(name, "map") {
                    self.check_call(&format!("{name}.map"), std::slice::from_ref(mapper), line)
                } else if self
                    .lookup_binding(name)
                    .is_some_and(|binding| option_element_type(&binding.ty).is_some())
                {
                    self.check_option_map(name, mapper, line).map(
                        |(_, mapped)| Type::Applied("Option".to_string(), vec![mapped]),
                    )
                } else if self
                    .lookup_binding(name)
                    .is_some_and(|binding| result_types(&binding.ty).is_some())
                {
                    self.check_result_map(name, mapper, line)
                        .map(|(_, error, mapped)| {
                            Type::Applied("Result".to_string(), vec![mapped, error])
                        })
                } else {
                    self.check_array_map(name, mapper, line)
                        .map(|(_, mapped)| Type::Array(Box::new(mapped)))
                }
            }
            Expr::ArrayMapValue { value, mapper } => {
                let value_ty = self.check_expr(value, line)?;
                if option_element_type(&value_ty).is_some() {
                    self.check_option_map_value(value, mapper, line).map(
                        |(_, mapped)| Type::Applied("Option".to_string(), vec![mapped]),
                    )
                } else if result_types(&value_ty).is_some() {
                    self.check_result_map_value(value, mapper, line)
                        .map(|(_, error, mapped)| {
                            Type::Applied("Result".to_string(), vec![mapped, error])
                        })
                } else {
                    self.check_array_map_value(value, mapper, line)
                        .map(|(_, mapped)| Type::Array(Box::new(mapped)))
                }
            }
            Expr::ArrayFilter { name, predicate } => self
                .check_array_predicate(name, predicate, "filter", line)
                .map(|element| Type::Array(Box::new(element))),
            Expr::ArrayFilterValue { value, predicate } => self
                .check_array_predicate_value(value, predicate, "filter", line)
                .map(|element| Type::Array(Box::new(element))),
            Expr::ArrayFlatMap { name, mapper } => {
                if self.is_qualified_function_call(name, "flatMap") {
                    self.check_call(
                        &format!("{name}.flatMap"),
                        std::slice::from_ref(mapper),
                        line,
                    )
                } else if self
                    .lookup_binding(name)
                    .is_some_and(|binding| result_types(&binding.ty).is_some())
                {
                    self.check_result_flat_map(name, mapper, line)
                        .map(|(_, _, result)| result)
                } else if self
                    .lookup_binding(name)
                    .is_some_and(|binding| option_element_type(&binding.ty).is_some())
                {
                    self.check_option_flat_map(name, mapper, line)
                        .map(|(_, mapped)| mapped)
                } else {
                    self.check_array_flat_map(name, mapper, line)
                        .map(|mapped| Type::Array(Box::new(mapped)))
                }
            }
            Expr::ArrayFlatMapValue { value, mapper } => {
                let value_ty = self.check_expr(value, line)?;
                if result_types(&value_ty).is_some() {
                    self.check_result_flat_map_value(value, mapper, line)
                        .map(|(_, _, result)| result)
                } else if option_element_type(&value_ty).is_some() {
                    self.check_option_flat_map_value(value, mapper, line)
                        .map(|(_, mapped)| mapped)
                } else {
                    self.check_array_flat_map_value(value, mapper, line)
                        .map(|mapped| Type::Array(Box::new(mapped)))
                }
            }
            Expr::ArrayFind { name, predicate } => self
                .check_array_predicate(name, predicate, "find", line)
                .map(|element| Type::Applied("Option".to_string(), vec![element])),
            Expr::ArrayFindValue { value, predicate } => self
                .check_array_predicate_value(value, predicate, "find", line)
                .map(|element| Type::Applied("Option".to_string(), vec![element])),
            Expr::ArrayAny { name, predicate } => self
                .check_array_predicate(name, predicate, "any", line)
                .map(|_| Type::Bool),
            Expr::ArrayAnyValue { value, predicate } => self
                .check_array_predicate_value(value, predicate, "any", line)
                .map(|_| Type::Bool),
            Expr::ArrayAll { name, predicate } => self
                .check_array_predicate(name, predicate, "all", line)
                .map(|_| Type::Bool),
            Expr::ArrayAllValue { value, predicate } => self
                .check_array_predicate_value(value, predicate, "all", line)
                .map(|_| Type::Bool),
            Expr::ArrayFold {
                name,
                initial,
                reducer,
            } => self.check_array_fold(name, initial, reducer, line),
            Expr::ArrayFoldValue {
                value,
                initial,
                reducer,
            } => self.check_array_fold_value(value, initial, reducer, line),
            Expr::OptionMap { name, mapper } => self
                .check_option_map(name, mapper, line)
                .map(|(_, mapped)| Type::Applied("Option".to_string(), vec![mapped])),
            Expr::OptionMapValue { value, mapper } => self
                .check_option_map_value(value, mapper, line)
                .map(|(_, mapped)| Type::Applied("Option".to_string(), vec![mapped])),
            Expr::ResultMap { name, mapper } => self
                .check_result_map(name, mapper, line)
                .map(|(_, error, mapped)| {
                    Type::Applied("Result".to_string(), vec![mapped, error])
                }),
            Expr::ResultMapValue { value, mapper } => self
                .check_result_map_value(value, mapper, line)
                .map(|(_, error, mapped)| {
                    Type::Applied("Result".to_string(), vec![mapped, error])
                }),
            Expr::OptionFlatMap { name, mapper } => {
                if self.is_qualified_function_call(name, "flatMap") {
                    self.check_call(
                        &format!("{name}.flatMap"),
                        std::slice::from_ref(mapper),
                        line,
                    )
                } else if self
                    .lookup_binding(name)
                    .is_some_and(|binding| result_types(&binding.ty).is_some())
                {
                    self.check_result_flat_map(name, mapper, line)
                        .map(|(_, _, result)| result)
                } else {
                    self.check_option_flat_map(name, mapper, line)
                        .map(|(_, mapped)| mapped)
                }
            }
            Expr::OptionFlatMapValue { value, mapper } => {
                let value_ty = self.check_expr(value, line)?;
                if result_types(&value_ty).is_some() {
                    self.check_result_flat_map_value(value, mapper, line)
                        .map(|(_, _, result)| result)
                } else {
                    self.check_option_flat_map_value(value, mapper, line)
                        .map(|(_, mapped)| mapped)
                }
            }
            Expr::ResultFlatMap { name, mapper } => self
                .check_result_flat_map(name, mapper, line)
                .map(|(_, _, result)| result),
            Expr::ResultFlatMapValue { value, mapper } => self
                .check_result_flat_map_value(value, mapper, line)
                .map(|(_, _, result)| result),
            Expr::OptionAp { name, value } => {
                if self.is_qualified_function_call(name, "ap") {
                    self.check_call(&format!("{name}.ap"), std::slice::from_ref(value), line)
                } else if self
                    .lookup_binding(name)
                    .is_some_and(|binding| result_types(&binding.ty).is_some())
                {
                    self.check_result_ap(name, value, line)
                } else {
                    self.check_option_ap(name, value, line)
                }
            }
            Expr::OptionApValue { function, value } => {
                let function_ty = self.check_expr(function, line)?;
                if result_types(&function_ty).is_some() {
                    self.check_result_ap_value(function, value, line)
                } else {
                    self.check_option_ap_value(function, value, line)
                }
            }
            Expr::ResultAp { name, value } => self.check_result_ap(name, value, line),
            Expr::ResultApValue { function, value } => {
                self.check_result_ap_value(function, value, line)
            }
            Expr::OptionOrElse { name, fallback } => {
                if self.is_qualified_function_call(name, "orElse") {
                    self.check_call(
                        &format!("{name}.orElse"),
                        std::slice::from_ref(fallback),
                        line,
                    )
                } else {
                    self.check_option_or_else(name, fallback, line)
                }
            }
            Expr::OptionOrElseValue { value, fallback } => {
                self.check_option_or_else_value(value, fallback, line)
            }
            Expr::OptionOrElseTry { value, fallback } => {
                self.check_option_or_else_try(value, fallback, line)
            }
            Expr::ArrayTake { name, count } => self.check_array_count(name, count, "take", line),
            Expr::ArrayTakeValue { value, count } => {
                self.check_array_count_value(value, count, "take", line)
            }
            Expr::ArrayDrop { name, count } => self.check_array_count(name, count, "drop", line),
            Expr::ArrayDropValue { value, count } => {
                self.check_array_count_value(value, count, "drop", line)
            }
            Expr::Join { name, separator } => self.check_join(name, separator, line),
            Expr::JoinValue { value, separator } => self.check_join_value(value, separator, line),
            Expr::ArrayPush { .. } => Err(CompileError::new(
                line,
                "push is only valid as a statement".to_string(),
            )),
            Expr::ArrayPop { .. } => Err(CompileError::new(
                line,
                "pop is only valid as a statement".to_string(),
            )),
            Expr::ArrayContains { name, value } => self.check_array_contains(name, value, line),
            Expr::ArrayContainsValue { value, item } => {
                self.check_array_contains_value(value, item, line)
            }
            Expr::ArrayIndexOf { name, value } => self.check_index_of(name, value, line),
            Expr::ArrayIndexOfValue { value, item } => {
                self.check_array_index_of_value(value, item, line)
            }
            Expr::MapKeys(name) => self.check_map_keys(name, line),
            Expr::MapKeysValue(value) => self.check_map_keys_value(value, line),
            Expr::MapValues(name) => self.check_map_values(name, line),
            Expr::MapValuesValue(value) => self.check_map_values_value(value, line),
            Expr::MapHas { name, key } => self.check_map_has(name, key, line),
            Expr::MapHasValue { value, key } => self.check_map_has_value(value, key, line),
            Expr::MapSet { name, key, value } => {
                if self.is_qualified_function_call(name, "set") {
                    self.check_call(
                        &format!("{name}.set"),
                        &[key.as_ref().clone(), value.as_ref().clone()],
                        line,
                    )
                } else {
                    Err(CompileError::new(
                        line,
                        "set is only valid as a statement".to_string(),
                    ))
                }
            }
            Expr::MapRemove { name, key } => {
                if self.is_qualified_function_call(name, "remove") {
                    self.check_call(
                        &format!("{name}.remove"),
                        std::slice::from_ref(key.as_ref()),
                        line,
                    )
                } else {
                    Err(CompileError::new(
                        line,
                        "remove is only valid as a statement".to_string(),
                    ))
                }
            }
            Expr::StringContains { name, needle } => self.check_string_contains(name, needle, line),
            Expr::StringContainsValue { value, needle } => {
                self.check_string_predicate_value(value, needle, "contains", "needle", line)
            }
            Expr::StringIndexOf { name, needle } => self.check_string_index_of(name, needle, line),
            Expr::StringIndexOfValue { value, needle } => {
                self.check_string_index_of_value(value, needle, line)
            }
            Expr::StringStartsWith { name, prefix } => {
                self.check_string_starts_with(name, prefix, line)
            }
            Expr::StringStartsWithValue { value, prefix } => {
                self.check_string_predicate_value(value, prefix, "startsWith", "prefix", line)
            }
            Expr::StringEndsWith { name, suffix } => {
                self.check_string_ends_with(name, suffix, line)
            }
            Expr::StringEndsWithValue { value, suffix } => {
                self.check_string_predicate_value(value, suffix, "endsWith", "suffix", line)
            }
            Expr::StringTrim(name) => self.check_string_trim(name, line),
            Expr::StringTrimValue(value) => self.check_string_transform_value(value, "trim", line),
            Expr::StringTrimStart(name) => self.check_string_transform(name, "trimStart", line),
            Expr::StringTrimStartValue(value) => {
                self.check_string_transform_value(value, "trimStart", line)
            }
            Expr::StringTrimEnd(name) => self.check_string_transform(name, "trimEnd", line),
            Expr::StringTrimEndValue(value) => {
                self.check_string_transform_value(value, "trimEnd", line)
            }
            Expr::StringToUpper(name) => self.check_string_to_upper(name, line),
            Expr::StringToUpperValue(value) => {
                self.check_string_transform_value(value, "toUpper", line)
            }
            Expr::StringToLower(name) => self.check_string_to_lower(name, line),
            Expr::StringToLowerValue(value) => {
                self.check_string_transform_value(value, "toLower", line)
            }
            Expr::StringRepeat { name, count } => self.check_string_repeat(name, count, line),
            Expr::StringRepeatValue { value, count } => {
                self.check_string_repeat_value(value, count, line)
            }
            Expr::StringSplit { name, separator } => self.check_string_split(name, separator, line),
            Expr::StringSplitValue { value, separator } => {
                self.check_string_split_value(value, separator, line)
            }
            Expr::StringReplace { name, from, to } => {
                self.check_string_replace(name, from, to, line)
            }
            Expr::StringReplaceValue { value, from, to } => {
                self.check_string_replace_value(value, from, to, line)
            }
            Expr::PathBasename(name) => self.check_path_transform(name, "basename", line),
            Expr::PathBasenameValue(value) => {
                self.check_path_transform_value(value, "basename", line)
            }
            Expr::PathDirname(name) => self.check_path_transform(name, "dirname", line),
            Expr::PathDirnameValue(value) => {
                self.check_path_transform_value(value, "dirname", line)
            }
            Expr::PathStem(name) => self.check_path_transform(name, "stem", line),
            Expr::PathStemValue(value) => self.check_path_transform_value(value, "stem", line),
            Expr::PathExtname(name) => self.check_path_transform(name, "extname", line),
            Expr::PathExtnameValue(value) => {
                self.check_path_transform_value(value, "extname", line)
            }
            Expr::PathIsAbsolute(name) => self.check_path_predicate(name, "isAbsolute", line),
            Expr::PathIsAbsoluteValue(value) => self
                .check_path_transform_value(value, "isAbsolute", line)
                .map(|_| Type::Bool),
            Expr::RawString(_) => Ok(Type::String),
            Expr::Env(name) | Expr::EnvDefault { name, .. } => {
                self.require_env_access(name, line)?;
                Ok(Type::String)
            }
            Expr::Command { .. }
            | Expr::CommandResult { .. }
            | Expr::AsyncCommand(_)
            | Expr::Pipeline { .. }
            | Expr::TryPipeline { .. }
            | Expr::PipelineResult { .. } => Err(unsafe_execution_error(line)),
            Expr::ProcessArgs => Ok(Type::Array(Box::new(Type::String))),
            Expr::FsIsFile { path } | Expr::FsIsDir { path } => {
                self.require_read_access("filesystem read", line)?;
                self.check_fs_path(path, line).map(|_| Type::Bool)
            }
            Expr::FsSize { path } => {
                self.require_read_access("fs.size", line)?;
                self.check_fs_path(path, line).map(|_| Type::Int)
            }
            Expr::FsReadLines { path } => {
                self.require_read_access("fs.readLines", line)?;
                self.check_fs_path(path, line)
                    .map(|_| Type::Array(Box::new(Type::String)))
            }
            Expr::FsList { path } => {
                self.require_read_access("fs.list", line)?;
                self.check_fs_path(path, line)
                    .map(|_| Type::Array(Box::new(Type::String)))
            }
            Expr::FsWriteLines { path, lines } => {
                self.require_write_access("fs.writeLines", line)?;
                self.check_fs_write_lines(path, lines, "fs.writeLines", line)
            }
            Expr::FsAppendLines { path, lines } => {
                self.require_write_access("fs.appendLines", line)?;
                self.check_fs_write_lines(path, lines, "fs.appendLines", line)
            }
            Expr::CliParse => Ok(Type::Map(Box::new(Type::String), Box::new(Type::String))),
            Expr::Await(name) => self.check_await(name, line),
            Expr::IfElse {
                condition,
                then_expr,
                else_expr,
            } => self.check_if_expr(condition, then_expr, else_expr, line),
            Expr::Match { value, arms } => self.check_match(value, arms, line),
            Expr::MatchGuardResult(value) => {
                let ty = self.check_expr(value, line)?;
                let Some((ok, _)) = result_types(&ty) else {
                    return Err(CompileError::new(
                        line,
                        format!("match guard propagation expects Result, found {}", ty.name()),
                    ));
                };
                if *ok == Type::Bool {
                    Ok(Type::Bool)
                } else {
                    Err(CompileError::new(
                        line,
                        format!("match guard must be Bool, found {}", ok.name()),
                    ))
                }
            }
            Expr::Ident(name) => {
                if self
                    .variants
                    .get(name)
                    .is_some_and(|variant| variant.fields.is_empty())
                {
                    return self.check_variant(name, &[], line);
                }
                self.lookup_binding(name)
                    .map(|binding| binding.ty.clone())
                    .or_else(|| self.functions.get(name).map(FunctionSig::function_type))
                    .ok_or_else(|| CompileError::new(line, format!("undefined variable `{name}`")))
            }
            Expr::Not(expr) => {
                let ty = self.check_expr(expr, line)?;
                if ty == Type::Bool {
                    Ok(Type::Bool)
                } else {
                    Err(CompileError::new(
                        line,
                        format!("operator `!` requires Bool operand, found {}", ty.name()),
                    ))
                }
            }
            Expr::BitNot(expr) => {
                let ty = self.check_expr(expr, line)?;
                if self.is_integer_numeric(&ty) {
                    Ok(Type::Int)
                } else {
                    Err(CompileError::new(
                        line,
                        format!("operator `~` requires Int operands, found {}", ty.name()),
                    ))
                }
            }
            Expr::Binary { left, op, right } => self.check_binary(left, *op, right, line),
        }
    }

    pub(super) fn check_binary(
        &self,
        left: &Expr,
        op: BinaryOp,
        right: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        let left_ty = self.check_expr(left, line)?;
        let right_ty = self.check_expr(right, line)?;
        match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div => {
                if self.is_numeric(&left_ty) && self.is_numeric(&right_ty) {
                    if left_ty == Type::Float || right_ty == Type::Float {
                        Ok(Type::Float)
                    } else {
                        Ok(Type::Int)
                    }
                } else {
                    Err(CompileError::new(
                        line,
                        format!(
                            "operator `{}` requires numeric operands, found {} and {}",
                            op.bash(),
                            left_ty.name(),
                            right_ty.name()
                        ),
                    ))
                }
            }
            BinaryOp::Mod => {
                if self.is_integer_numeric(&left_ty) && self.is_integer_numeric(&right_ty) {
                    Ok(Type::Int)
                } else {
                    Err(CompileError::new(
                        line,
                        format!(
                            "operator `{}` requires Int operands, found {} and {}",
                            op.bash(),
                            left_ty.name(),
                            right_ty.name()
                        ),
                    ))
                }
            }
            BinaryOp::Concat => {
                if self.is_string_like(&left_ty) && self.is_string_like(&right_ty) {
                    Ok(Type::String)
                } else {
                    Err(CompileError::new(
                        line,
                        format!(
                            "operator `++` requires String or Path operands, found {} and {}",
                            left_ty.name(),
                            right_ty.name()
                        ),
                    ))
                }
            }
            BinaryOp::BitAnd
            | BinaryOp::BitOr
            | BinaryOp::BitXor
            | BinaryOp::Shl
            | BinaryOp::Shr => {
                if self.is_integer_numeric(&left_ty) && self.is_integer_numeric(&right_ty) {
                    Ok(Type::Int)
                } else {
                    Err(CompileError::new(
                        line,
                        format!(
                            "operator `{}` requires Int operands, found {} and {}",
                            op.bash(),
                            left_ty.name(),
                            right_ty.name()
                        ),
                    ))
                }
            }
            BinaryOp::Eq | BinaryOp::Ne => {
                if self.is_comparable_by_equality(&left_ty, &right_ty) {
                    Ok(Type::Bool)
                } else {
                    Err(CompileError::new(
                        line,
                        format!(
                            "operator `{}` requires matching operand types, found {} and {}",
                            op.bash(),
                            left_ty.name(),
                            right_ty.name()
                        ),
                    ))
                }
            }
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                if self.is_numeric(&left_ty) && self.is_numeric(&right_ty) {
                    Ok(Type::Bool)
                } else {
                    Err(CompileError::new(
                        line,
                        format!(
                            "operator `{}` requires numeric operands, found {} and {}",
                            op.bash(),
                            left_ty.name(),
                            right_ty.name()
                        ),
                    ))
                }
            }
            BinaryOp::And | BinaryOp::Or => {
                if left_ty == Type::Bool && right_ty == Type::Bool {
                    Ok(Type::Bool)
                } else {
                    Err(CompileError::new(
                        line,
                        format!(
                            "operator `{}` requires Bool operands, found {} and {}",
                            op.bash(),
                            left_ty.name(),
                            right_ty.name()
                        ),
                    ))
                }
            }
        }
    }

    pub(super) fn check_cast(
        &self,
        expr: &Expr,
        target: &Type,
        line: usize,
    ) -> Result<Type, CompileError> {
        let actual = self.check_expr(expr, line)?;
        let target = self.resolve_type(target, line)?;
        if self.is_castable(&target, &actual, expr) {
            Ok(target)
        } else {
            Err(CompileError::new(
                line,
                format!("cannot cast {} to {}", actual.name(), target.name()),
            ))
        }
    }

    pub(super) fn check_if_expr(
        &self,
        condition: &Expr,
        then_expr: &Expr,
        else_expr: &Expr,
        line: usize,
    ) -> Result<Type, CompileError> {
        self.check_condition(condition, line)?;
        let then_ty = self.check_expr(then_expr, line)?;
        let else_ty = self.check_expr(else_expr, line)?;
        if self.is_assignable(&then_ty, &else_ty, else_expr) {
            Ok(then_ty)
        } else if self.is_assignable(&else_ty, &then_ty, then_expr) {
            Ok(else_ty)
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "if expression branches must have matching types, found {} and {}",
                    then_ty.name(),
                    else_ty.name()
                ),
            ))
        }
    }
}
