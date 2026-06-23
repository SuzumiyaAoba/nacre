use std::collections::HashSet;

use crate::{
    AssignTarget, BindingPattern, ClosureCapture, DoStep, Expr, ForBinding, ImplConst, ImplMethod,
    MatchArm, Param, Program, Statement, TraitMethod, Type, TypeParam, VariantDecl,
};

struct NamespaceContext<'a> {
    namespace: &'a str,
    function_names: &'a HashSet<String>,
    binding_names: &'a HashSet<String>,
    type_names: &'a HashSet<String>,
    trait_names: &'a HashSet<String>,
}

pub(super) fn namespace_module(program: Program, namespace: &str) -> Program {
    let function_names = program
        .statements()
        .iter()
        .flat_map(|statement| match statement {
            Statement::Function { name, .. } | Statement::ExternalFunction { name, .. } => {
                vec![name.clone()]
            }
            Statement::SumType { variants, .. } => variants
                .iter()
                .map(|variant| variant.name.clone())
                .collect(),
            _ => Vec::new(),
        })
        .collect::<HashSet<_>>();
    let binding_names = program
        .statements()
        .iter()
        .flat_map(|statement| match statement {
            Statement::Const { name, .. } | Statement::Let { name, .. } if name != "_" => {
                vec![name.clone()]
            }
            Statement::Destructure { pattern, .. } => binding_pattern_names(pattern),
            _ => Vec::new(),
        })
        .collect::<HashSet<_>>();
    let type_names = program
        .statements()
        .iter()
        .filter_map(|statement| match statement {
            Statement::TypeAlias { name, .. }
            | Statement::SumType { name, .. }
            | Statement::Newtype { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect::<HashSet<_>>();
    let trait_names = program
        .statements()
        .iter()
        .filter_map(|statement| match statement {
            Statement::Trait { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect::<HashSet<_>>();
    let context = NamespaceContext {
        namespace,
        function_names: &function_names,
        binding_names: &binding_names,
        type_names: &type_names,
        trait_names: &trait_names,
    };
    let statements = program
        .statements()
        .iter()
        .map(|statement| {
            namespace_statement(statement, &context, &HashSet::new(), &HashSet::new(), true)
        })
        .collect();
    Program::new(statements, program.statement_lines().to_vec())
}

fn namespace_statement(
    statement: &Statement,
    context: &NamespaceContext<'_>,
    local_names: &HashSet<String>,
    local_type_names: &HashSet<String>,
    top_level: bool,
) -> Statement {
    let NamespaceContext {
        namespace,
        function_names,
        binding_names,
        type_names,
        trait_names,
    } = context;
    match statement {
        Statement::Function {
            name,
            override_constructor,
            type_params,
            params,
            return_type,
            body,
        } => {
            let function_type_names = type_params
                .iter()
                .map(|param| param.name.clone())
                .collect::<HashSet<_>>();
            Statement::Function {
                name: qualify_function(name, namespace, function_names),
                override_constructor: *override_constructor,
                type_params: namespace_type_params(type_params, namespace, trait_names),
                params: params
                    .iter()
                    .map(|param| Param {
                        name: param.name.clone(),
                        ty: namespace_type(&param.ty, namespace, type_names, &function_type_names),
                        default: param.default.as_ref().map(|expr| {
                            namespace_expr(expr, context, local_names, &function_type_names)
                        }),
                        variadic: param.variadic,
                        capture_name: param.capture_name.clone(),
                    })
                    .collect(),
                return_type: namespace_type(
                    return_type,
                    namespace,
                    type_names,
                    &function_type_names,
                ),
                body: namespace_function_body(body, context, params, &function_type_names),
            }
        }
        Statement::ExternalFunction {
            name,
            type_params,
            params,
            return_type,
        } => {
            let function_type_names = type_params
                .iter()
                .map(|param| param.name.clone())
                .collect::<HashSet<_>>();
            Statement::ExternalFunction {
                name: qualify_function(name, namespace, function_names),
                type_params: namespace_type_params(type_params, namespace, trait_names),
                params: params
                    .iter()
                    .map(|param| Param {
                        name: param.name.clone(),
                        ty: namespace_type(&param.ty, namespace, type_names, &function_type_names),
                        default: param.default.as_ref().map(|expr| {
                            namespace_expr(expr, context, local_names, &function_type_names)
                        }),
                        variadic: param.variadic,
                        capture_name: param.capture_name.clone(),
                    })
                    .collect(),
                return_type: namespace_type(
                    return_type,
                    namespace,
                    type_names,
                    &function_type_names,
                ),
            }
        }
        Statement::Const {
            name,
            annotation,
            expr,
        } => Statement::Const {
            name: qualify_decl_name(name, namespace, binding_names, top_level),
            annotation: annotation
                .as_ref()
                .map(|ty| namespace_type(ty, namespace, type_names, local_type_names)),
            expr: namespace_expr(expr, context, local_names, local_type_names),
        },
        Statement::Let {
            name,
            annotation,
            expr,
        } => Statement::Let {
            name: qualify_decl_name(name, namespace, binding_names, top_level),
            annotation: annotation
                .as_ref()
                .map(|ty| namespace_type(ty, namespace, type_names, local_type_names)),
            expr: namespace_expr(expr, context, local_names, local_type_names),
        },
        Statement::Destructure {
            mutable,
            pattern,
            expr,
        } => Statement::Destructure {
            mutable: *mutable,
            pattern: namespace_pattern(pattern, namespace, binding_names, top_level),
            expr: namespace_expr(expr, context, local_names, local_type_names),
        },
        Statement::Assign { target, expr } => Statement::Assign {
            target: namespace_assign_target(target, context, local_names, local_type_names),
            expr: namespace_expr(expr, context, local_names, local_type_names),
        },
        Statement::Expr(expr) => {
            Statement::Expr(namespace_expr(expr, context, local_names, local_type_names))
        }
        Statement::TypeAlias {
            name,
            type_params,
            ty,
        } => {
            let alias_type_names = type_params.iter().cloned().collect::<HashSet<_>>();
            Statement::TypeAlias {
                name: qualify_type_name(name, namespace, type_names, local_type_names),
                type_params: type_params.clone(),
                ty: namespace_type(ty, namespace, type_names, &alias_type_names),
            }
        }
        Statement::SumType {
            name,
            type_params,
            variants,
        } => {
            let type_param_names = type_params.iter().cloned().collect::<HashSet<_>>();
            Statement::SumType {
                name: qualify_type_name(name, namespace, type_names, local_type_names),
                type_params: type_params.clone(),
                variants: variants
                    .iter()
                    .map(|variant| VariantDecl {
                        name: qualify_function(&variant.name, namespace, function_names),
                        fields: variant
                            .fields
                            .iter()
                            .map(|field| {
                                namespace_type(field, namespace, type_names, &type_param_names)
                            })
                            .collect(),
                    })
                    .collect(),
            }
        }
        Statement::Newtype {
            name,
            type_params,
            base,
        } => {
            let type_param_names = type_params.iter().cloned().collect::<HashSet<_>>();
            Statement::Newtype {
                name: qualify_type_name(name, namespace, type_names, local_type_names),
                type_params: type_params.clone(),
                base: namespace_type(base, namespace, type_names, &type_param_names),
            }
        }
        Statement::Trait {
            name,
            type_param,
            methods,
        } => {
            let trait_type_names = HashSet::from([type_param.clone()]);
            Statement::Trait {
                name: qualify_trait_name(name, namespace, trait_names),
                type_param: type_param.clone(),
                methods: methods
                    .iter()
                    .map(|method| TraitMethod {
                        name: method.name.clone(),
                        params: method
                            .params
                            .iter()
                            .map(|param| Param {
                                name: param.name.clone(),
                                ty: namespace_type(
                                    &param.ty,
                                    namespace,
                                    type_names,
                                    &trait_type_names,
                                ),
                                default: param.default.clone(),
                                variadic: param.variadic,
                                capture_name: param.capture_name.clone(),
                            })
                            .collect(),
                        return_type: namespace_type(
                            &method.return_type,
                            namespace,
                            type_names,
                            &trait_type_names,
                        ),
                    })
                    .collect(),
            }
        }
        Statement::Impl {
            trait_name,
            for_type,
            methods,
        } => Statement::Impl {
            trait_name: qualify_trait_name(trait_name, namespace, trait_names),
            for_type: namespace_type(for_type, namespace, type_names, local_type_names),
            methods: methods
                .iter()
                .map(|method| ImplMethod {
                    name: method.name.clone(),
                    params: method
                        .params
                        .iter()
                        .map(|param| Param {
                            name: param.name.clone(),
                            ty: namespace_type(&param.ty, namespace, type_names, local_type_names),
                            default: param.default.clone(),
                            variadic: param.variadic,
                            capture_name: param.capture_name.clone(),
                        })
                        .collect(),
                    return_type: namespace_type(
                        &method.return_type,
                        namespace,
                        type_names,
                        local_type_names,
                    ),
                    body: namespace_program_body(
                        &method.body,
                        context,
                        &method
                            .params
                            .iter()
                            .map(|param| param.name.clone())
                            .collect::<HashSet<_>>(),
                        local_type_names,
                    ),
                })
                .collect(),
        },
        Statement::InherentImpl {
            for_type,
            consts,
            methods,
        } => Statement::InherentImpl {
            for_type: namespace_type(for_type, namespace, type_names, local_type_names),
            consts: consts
                .iter()
                .map(|value| ImplConst {
                    name: value.name.clone(),
                    annotation: value
                        .annotation
                        .as_ref()
                        .map(|ty| namespace_type(ty, namespace, type_names, local_type_names)),
                    expr: namespace_expr(&value.expr, context, local_names, local_type_names),
                })
                .collect(),
            methods: methods
                .iter()
                .map(|method| ImplMethod {
                    name: method.name.clone(),
                    params: method
                        .params
                        .iter()
                        .map(|param| Param {
                            name: param.name.clone(),
                            ty: namespace_type(&param.ty, namespace, type_names, local_type_names),
                            default: param.default.as_ref().map(|default| {
                                namespace_expr(default, context, local_names, local_type_names)
                            }),
                            variadic: param.variadic,
                            capture_name: param.capture_name.clone(),
                        })
                        .collect(),
                    return_type: namespace_type(
                        &method.return_type,
                        namespace,
                        type_names,
                        local_type_names,
                    ),
                    body: namespace_program_body(
                        &method.body,
                        context,
                        &method
                            .params
                            .iter()
                            .map(|param| param.name.clone())
                            .collect::<HashSet<_>>(),
                        local_type_names,
                    ),
                })
                .collect(),
        },
        Statement::If {
            condition,
            then_branch,
            else_branch,
        } => Statement::If {
            condition: namespace_expr(condition, context, local_names, local_type_names),
            then_branch: namespace_program_body(
                then_branch,
                context,
                local_names,
                local_type_names,
            ),
            else_branch: else_branch.as_ref().map(|branch| {
                namespace_program_body(branch, context, local_names, local_type_names)
            }),
        },
        Statement::Block { body } => Statement::Block {
            body: namespace_program_body(body, context, local_names, local_type_names),
        },
        Statement::While { condition, body } => Statement::While {
            condition: namespace_expr(condition, context, local_names, local_type_names),
            body: namespace_program_body(body, context, local_names, local_type_names),
        },
        Statement::For {
            binding,
            iterable,
            body,
        } => Statement::For {
            binding: binding.clone(),
            iterable: namespace_expr(iterable, context, local_names, local_type_names),
            body: namespace_program_body(
                body,
                context,
                &with_for_binding_locals(local_names, binding),
                local_type_names,
            ),
        },
        Statement::Return(expr) => {
            Statement::Return(namespace_expr(expr, context, local_names, local_type_names))
        }
        Statement::TryResult(expr) => {
            Statement::TryResult(namespace_expr(expr, context, local_names, local_type_names))
        }
        Statement::TryPipeline { input, commands } => Statement::TryPipeline {
            input: input.as_ref().map(|input| {
                Box::new(namespace_expr(
                    input,
                    context,
                    local_names,
                    local_type_names,
                ))
            }),
            commands: commands.clone(),
        },
        Statement::TryPipelineResult { input, commands } => Statement::TryPipelineResult {
            input: input.as_ref().map(|input| {
                Box::new(namespace_expr(
                    input,
                    context,
                    local_names,
                    local_type_names,
                ))
            }),
            commands: commands.clone(),
        },
        other => other.clone(),
    }
}

fn namespace_program_body(
    program: &Program,
    context: &NamespaceContext<'_>,
    local_names: &HashSet<String>,
    local_type_names: &HashSet<String>,
) -> Program {
    let mut block_locals = local_names.clone();
    let mut statements = Vec::new();
    for statement in program.statements() {
        statements.push(namespace_statement(
            statement,
            context,
            &block_locals,
            local_type_names,
            false,
        ));
        match statement {
            Statement::Const { name, .. } | Statement::Let { name, .. } if name != "_" => {
                block_locals.insert(name.clone());
            }
            Statement::Destructure { pattern, .. } => {
                block_locals.extend(binding_pattern_names(pattern));
            }
            _ => {}
        }
    }

    Program::new(statements, program.statement_lines().to_vec())
}

fn namespace_function_body(
    program: &Program,
    context: &NamespaceContext<'_>,
    params: &[Param],
    local_type_names: &HashSet<String>,
) -> Program {
    let local_names = params
        .iter()
        .map(|param| param.name.clone())
        .collect::<HashSet<_>>();
    namespace_program_body(program, context, &local_names, local_type_names)
}

fn with_for_binding_locals(local_names: &HashSet<String>, binding: &ForBinding) -> HashSet<String> {
    let mut names = local_names.clone();
    match binding {
        ForBinding::Name(name) if name != "_" => {
            names.insert(name.clone());
        }
        ForBinding::Name(_) => {}
        ForBinding::Pattern(pattern) => {
            names.extend(binding_pattern_names(pattern));
        }
    }
    names
}

#[cfg(test)]
fn with_local(local_names: &HashSet<String>, name: &str) -> HashSet<String> {
    let mut names = local_names.clone();
    if name != "_" {
        names.insert(name.to_string());
    }
    names
}

fn binding_pattern_names(pattern: &BindingPattern) -> Vec<String> {
    let mut names = Vec::new();
    collect_binding_pattern_names(pattern, &mut names);
    names
}

fn collect_binding_pattern_names(pattern: &BindingPattern, names: &mut Vec<String>) {
    match pattern {
        BindingPattern::Name(name) if name != "_" => names.push(name.clone()),
        BindingPattern::Name(_) => {}
        BindingPattern::Tuple(values) => {
            for value in values {
                collect_binding_pattern_names(value, names);
            }
        }
        BindingPattern::Array { patterns, rest } => {
            for value in patterns {
                collect_binding_pattern_names(value, names);
            }
            if let Some(rest) = rest.as_ref().filter(|name| *name != "_") {
                names.push(rest.clone());
            }
        }
        BindingPattern::Record(bindings) => {
            for (_, pattern) in bindings {
                collect_binding_pattern_names(pattern, names);
            }
        }
    }
}

fn collect_match_pattern_names(pattern: &Expr, local_names: &mut HashSet<String>) {
    match pattern {
        Expr::Some(value) | Expr::Ok(value) | Expr::Err(value) => {
            collect_match_pattern_names(value, local_names);
        }
        Expr::Tuple(values) => {
            for value in values {
                collect_match_pattern_names(value, local_names);
            }
        }
        Expr::RecordPattern(fields) => {
            for (field, value) in fields {
                match value {
                    None if field != "_" => {
                        local_names.insert(field.clone());
                    }
                    Some(Expr::Ident(name)) if name != "_" => {
                        local_names.insert(name.clone());
                    }
                    _ => {}
                }
            }
        }
        Expr::ArrayPattern { patterns, rest } => {
            for value in patterns {
                collect_match_pattern_names(value, local_names);
            }
            if let Some(rest) = rest.as_ref().filter(|name| *name != "_") {
                local_names.insert(rest.clone());
            }
        }
        Expr::AliasPattern { pattern, alias } => {
            collect_match_pattern_names(pattern, local_names);
            if alias != "_" {
                local_names.insert(alias.clone());
            }
        }
        Expr::Call { args, .. } | Expr::Variant { args, .. } => {
            for value in args {
                collect_match_pattern_names(value, local_names);
            }
        }
        Expr::Ident(name) if name != "_" => {
            local_names.insert(name.clone());
        }
        Expr::NamedArg { value, .. } => collect_match_pattern_names(value, local_names),
        _ => {}
    }
}

fn namespace_pattern(
    pattern: &BindingPattern,
    namespace: &str,
    binding_names: &HashSet<String>,
    top_level: bool,
) -> BindingPattern {
    match pattern {
        BindingPattern::Name(name) => {
            BindingPattern::Name(qualify_decl_name(name, namespace, binding_names, top_level))
        }
        BindingPattern::Tuple(names) => BindingPattern::Tuple(
            names
                .iter()
                .map(|pattern| namespace_pattern(pattern, namespace, binding_names, top_level))
                .collect(),
        ),
        BindingPattern::Array { patterns, rest } => BindingPattern::Array {
            patterns: patterns
                .iter()
                .map(|pattern| namespace_pattern(pattern, namespace, binding_names, top_level))
                .collect(),
            rest: rest
                .as_ref()
                .map(|name| qualify_decl_name(name, namespace, binding_names, top_level)),
        },
        BindingPattern::Record(bindings) => BindingPattern::Record(
            bindings
                .iter()
                .map(|(field, name)| {
                    (
                        field.clone(),
                        namespace_pattern(name, namespace, binding_names, top_level),
                    )
                })
                .collect(),
        ),
    }
}

fn namespace_assign_target(
    target: &AssignTarget,
    context: &NamespaceContext<'_>,
    local_names: &HashSet<String>,
    local_type_names: &HashSet<String>,
) -> AssignTarget {
    let NamespaceContext {
        namespace,
        binding_names,
        ..
    } = context;
    match target {
        AssignTarget::Name(name) => AssignTarget::Name(qualify_ref_name(
            name,
            namespace,
            binding_names,
            local_names,
        )),
        AssignTarget::Index { name, index } => AssignTarget::Index {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            index: namespace_expr(index, context, local_names, local_type_names),
        },
        AssignTarget::FieldIndex { name, field, index } => AssignTarget::FieldIndex {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            field: field.clone(),
            index: namespace_expr(index, context, local_names, local_type_names),
        },
        AssignTarget::Field { name, field } => AssignTarget::Field {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            field: field.clone(),
        },
        AssignTarget::TupleField { name, field } => AssignTarget::TupleField {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            field: *field,
        },
    }
}

fn namespace_type(
    ty: &Type,
    namespace: &str,
    type_names: &HashSet<String>,
    local_type_names: &HashSet<String>,
) -> Type {
    match ty {
        Type::Future(value) => Type::Future(Box::new(namespace_type(
            value,
            namespace,
            type_names,
            local_type_names,
        ))),
        Type::Array(element) => Type::Array(Box::new(namespace_type(
            element,
            namespace,
            type_names,
            local_type_names,
        ))),
        Type::Map(key, value) => Type::Map(
            Box::new(namespace_type(key, namespace, type_names, local_type_names)),
            Box::new(namespace_type(
                value,
                namespace,
                type_names,
                local_type_names,
            )),
        ),
        Type::Record(fields) => Type::Record(
            fields
                .iter()
                .map(|(name, ty)| {
                    (
                        name.clone(),
                        namespace_type(ty, namespace, type_names, local_type_names),
                    )
                })
                .collect(),
        ),
        Type::Tuple(elements) => Type::Tuple(
            elements
                .iter()
                .map(|element| namespace_type(element, namespace, type_names, local_type_names))
                .collect(),
        ),
        Type::Function(params, return_type) => Type::Function(
            params
                .iter()
                .map(|param| namespace_type(param, namespace, type_names, local_type_names))
                .collect(),
            Box::new(namespace_type(
                return_type,
                namespace,
                type_names,
                local_type_names,
            )),
        ),
        Type::Union(types) => Type::Union(
            types
                .iter()
                .map(|ty| namespace_type(ty, namespace, type_names, local_type_names))
                .collect(),
        ),
        Type::Intersection(types) => Type::Intersection(
            types
                .iter()
                .map(|ty| namespace_type(ty, namespace, type_names, local_type_names))
                .collect(),
        ),
        Type::Applied(name, args) => Type::Applied(
            qualify_type_name(name, namespace, type_names, local_type_names),
            args.iter()
                .map(|arg| namespace_type(arg, namespace, type_names, local_type_names))
                .collect(),
        ),
        Type::Named(name) => Type::Named(qualify_type_name(
            name,
            namespace,
            type_names,
            local_type_names,
        )),
        other => other.clone(),
    }
}

fn namespace_type_params(
    type_params: &[TypeParam],
    namespace: &str,
    trait_names: &HashSet<String>,
) -> Vec<TypeParam> {
    type_params
        .iter()
        .map(|param| TypeParam {
            name: param.name.clone(),
            bounds: param
                .bounds
                .iter()
                .map(|bound| qualify_trait_name(bound, namespace, trait_names))
                .collect(),
        })
        .collect()
}

fn namespace_expr(
    expr: &Expr,
    context: &NamespaceContext<'_>,
    local_names: &HashSet<String>,
    local_type_names: &HashSet<String>,
) -> Expr {
    let NamespaceContext {
        namespace,
        function_names,
        binding_names,
        type_names,
        trait_names,
    } = context;
    match expr {
        Expr::String(value) => Expr::String(namespace_interpolations(
            value,
            namespace,
            binding_names,
            local_names,
        )),
        Expr::Range {
            start,
            end,
            inclusive,
        } => Expr::Range {
            start: Box::new(namespace_expr(
                start,
                context,
                local_names,
                local_type_names,
            )),
            end: Box::new(namespace_expr(end, context, local_names, local_type_names)),
            inclusive: *inclusive,
        },
        Expr::Array(values) => Expr::Array(
            values
                .iter()
                .map(|value| namespace_expr(value, context, local_names, local_type_names))
                .collect(),
        ),
        Expr::Map(entries) => Expr::Map(
            entries
                .iter()
                .map(|(key, value)| {
                    (
                        namespace_expr(key, context, local_names, local_type_names),
                        namespace_expr(value, context, local_names, local_type_names),
                    )
                })
                .collect(),
        ),
        Expr::Record(fields) => Expr::Record(
            fields
                .iter()
                .map(|(name, value)| {
                    (
                        name.clone(),
                        namespace_expr(value, context, local_names, local_type_names),
                    )
                })
                .collect(),
        ),
        Expr::RecordPattern(fields) => Expr::RecordPattern(
            fields
                .iter()
                .map(|(name, value)| {
                    (
                        name.clone(),
                        value.as_ref().map(|value| {
                            namespace_expr(value, context, local_names, local_type_names)
                        }),
                    )
                })
                .collect(),
        ),
        Expr::ArrayPattern { patterns, rest } => Expr::ArrayPattern {
            patterns: patterns
                .iter()
                .map(|value| namespace_expr(value, context, local_names, local_type_names))
                .collect(),
            rest: rest.clone(),
        },
        Expr::AliasPattern { pattern, alias } => Expr::AliasPattern {
            pattern: Box::new(namespace_expr(
                pattern,
                context,
                local_names,
                local_type_names,
            )),
            alias: alias.clone(),
        },
        Expr::Tuple(values) => Expr::Tuple(
            values
                .iter()
                .map(|value| namespace_expr(value, context, local_names, local_type_names))
                .collect(),
        ),
        Expr::Index { name, index } => Expr::Index {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            index: Box::new(namespace_expr(
                index,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::IndexValue { value, index } => Expr::IndexValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            index: Box::new(namespace_expr(
                index,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::Slice { name, start, end } => Expr::Slice {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            start: Box::new(namespace_expr(
                start,
                context,
                local_names,
                local_type_names,
            )),
            end: Box::new(namespace_expr(end, context, local_names, local_type_names)),
        },
        Expr::ArraySliceValue { value, start, end } => Expr::ArraySliceValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            start: Box::new(namespace_expr(
                start,
                context,
                local_names,
                local_type_names,
            )),
            end: Box::new(namespace_expr(end, context, local_names, local_type_names)),
        },
        Expr::PathExists(path) => Expr::PathExists(Box::new(namespace_expr(
            path,
            context,
            local_names,
            local_type_names,
        ))),
        Expr::ProcessEnv { name } => Expr::ProcessEnv {
            name: Box::new(namespace_expr(name, context, local_names, local_type_names)),
        },
        Expr::FsIsFile { path } => Expr::FsIsFile {
            path: Box::new(namespace_expr(path, context, local_names, local_type_names)),
        },
        Expr::FsIsDir { path } => Expr::FsIsDir {
            path: Box::new(namespace_expr(path, context, local_names, local_type_names)),
        },
        Expr::FsSize { path } => Expr::FsSize {
            path: Box::new(namespace_expr(path, context, local_names, local_type_names)),
        },
        Expr::FsReadLines { path } => Expr::FsReadLines {
            path: Box::new(namespace_expr(path, context, local_names, local_type_names)),
        },
        Expr::FsList { path } => Expr::FsList {
            path: Box::new(namespace_expr(path, context, local_names, local_type_names)),
        },
        Expr::FsWriteLines { path, lines } => Expr::FsWriteLines {
            path: Box::new(namespace_expr(path, context, local_names, local_type_names)),
            lines: Box::new(namespace_expr(
                lines,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::JsonParse { value } => Expr::JsonParse {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::JsonStringify { name } => Expr::JsonStringify {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
        },
        Expr::JsonStringifyValue { value } => Expr::JsonStringifyValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::AsyncCommand(_) => expr.clone(),
        Expr::Async(value) => Expr::Async(Box::new(namespace_expr(
            value,
            context,
            local_names,
            local_type_names,
        ))),
        Expr::CommandResult { .. } => expr.clone(),
        Expr::AllowedCommand {
            group,
            command,
            args,
            result,
            program,
            read_args,
            write_args,
        } => Expr::AllowedCommand {
            group: group.clone(),
            command: command.clone(),
            args: args
                .iter()
                .map(|arg| namespace_expr(arg, context, local_names, local_type_names))
                .collect(),
            result: *result,
            program: program.clone(),
            read_args: read_args.clone(),
            write_args: write_args.clone(),
        },
        Expr::Await(name) => Expr::Await(qualify_ref_name(
            name,
            namespace,
            binding_names,
            local_names,
        )),
        Expr::Pipeline { input, commands } => Expr::Pipeline {
            input: input.as_ref().map(|input| {
                Box::new(namespace_expr(
                    input,
                    context,
                    local_names,
                    local_type_names,
                ))
            }),
            commands: commands.clone(),
        },
        Expr::TryPipeline { input, commands } => Expr::TryPipeline {
            input: input.as_ref().map(|input| {
                Box::new(namespace_expr(
                    input,
                    context,
                    local_names,
                    local_type_names,
                ))
            }),
            commands: commands.clone(),
        },
        Expr::PipelineResult { input, commands } => Expr::PipelineResult {
            input: input.as_ref().map(|input| {
                Box::new(namespace_expr(
                    input,
                    context,
                    local_names,
                    local_type_names,
                ))
            }),
            commands: commands.clone(),
        },
        Expr::NewtypeCtor { name, value } => Expr::NewtypeCtor {
            name: if function_names.contains(name) {
                qualify_function(name, namespace, function_names)
            } else {
                qualify_type_name(name, namespace, type_names, local_type_names)
            },
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::Variant {
            name,
            args,
            field_types,
        } => Expr::Variant {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            args: args
                .iter()
                .map(|arg| namespace_expr(arg, context, local_names, local_type_names))
                .collect(),
            field_types: field_types
                .iter()
                .map(|ty| namespace_type(ty, namespace, type_names, local_type_names))
                .collect(),
        },
        Expr::Cast { expr, ty } => Expr::Cast {
            expr: Box::new(namespace_expr(expr, context, local_names, local_type_names)),
            ty: namespace_type(ty, namespace, type_names, local_type_names),
        },
        Expr::Lambda { params, body } => {
            let mut lambda_locals = local_names.clone();
            lambda_locals.extend(params.iter().cloned());
            Expr::Lambda {
                params: params.clone(),
                body: Box::new(namespace_expr(
                    body,
                    context,
                    &lambda_locals,
                    local_type_names,
                )),
            }
        }
        Expr::Closure { name, captures } => Expr::Closure {
            name: qualify_ref_name(name, namespace, function_names, local_names),
            captures: captures
                .iter()
                .map(|capture| ClosureCapture {
                    source: qualify_ref_name(
                        &capture.source,
                        namespace,
                        binding_names,
                        local_names,
                    ),
                    target: capture.target.clone(),
                    suffixes: capture.suffixes.clone(),
                })
                .collect(),
        },
        Expr::Do { steps, result } => {
            let mut do_locals = local_names.clone();
            let steps = steps
                .iter()
                .map(|step| {
                    let step = match step {
                        DoStep::Bind { name, expr } => DoStep::Bind {
                            name: name.clone(),
                            expr: namespace_expr(expr, context, &do_locals, local_type_names),
                        },
                        DoStep::Let {
                            name,
                            annotation,
                            expr,
                        } => DoStep::Let {
                            name: name.clone(),
                            annotation: annotation.as_ref().map(|ty| {
                                namespace_type(ty, namespace, type_names, local_type_names)
                            }),
                            expr: namespace_expr(expr, context, &do_locals, local_type_names),
                        },
                    };
                    match &step {
                        DoStep::Bind { name, .. } | DoStep::Let { name, .. } => {
                            do_locals.insert(name.clone());
                        }
                    }
                    step
                })
                .collect();
            Expr::Do {
                steps,
                result: Box::new(namespace_expr(
                    result,
                    context,
                    &do_locals,
                    local_type_names,
                )),
            }
        }
        Expr::LetIn {
            name,
            annotation,
            value,
            body,
        } => {
            let mut body_locals = local_names.clone();
            body_locals.insert(name.clone());
            Expr::LetIn {
                name: name.clone(),
                annotation: annotation
                    .as_ref()
                    .map(|ty| namespace_type(ty, namespace, type_names, local_type_names)),
                value: Box::new(namespace_expr(
                    value,
                    context,
                    local_names,
                    local_type_names,
                )),
                body: Box::new(namespace_expr(
                    body,
                    context,
                    &body_locals,
                    local_type_names,
                )),
            }
        }
        Expr::Call { name, args } => Expr::Call {
            name: qualify_call_name(
                name,
                namespace,
                function_names,
                binding_names,
                trait_names,
                local_names,
            ),
            args: args
                .iter()
                .map(|arg| namespace_expr(arg, context, local_names, local_type_names))
                .collect(),
        },
        Expr::NamedArg { name, value } => Expr::NamedArg {
            name: name.clone(),
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::Some(value) => Expr::Some(Box::new(namespace_expr(
            value,
            context,
            local_names,
            local_type_names,
        ))),
        Expr::Ok(value) => Expr::Ok(Box::new(namespace_expr(
            value,
            context,
            local_names,
            local_type_names,
        ))),
        Expr::Err(value) => Expr::Err(Box::new(namespace_expr(
            value,
            context,
            local_names,
            local_type_names,
        ))),
        Expr::ResultOption(value) => Expr::ResultOption(Box::new(namespace_expr(
            value,
            context,
            local_names,
            local_type_names,
        ))),
        Expr::Default { value, fallback } => Expr::Default {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            fallback: Box::new(namespace_expr(
                fallback,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::DefaultTry { value, fallback } => Expr::DefaultTry {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            fallback: Box::new(namespace_expr(
                fallback,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::IfElse {
            condition,
            then_expr,
            else_expr,
        } => Expr::IfElse {
            condition: Box::new(namespace_expr(
                condition,
                context,
                local_names,
                local_type_names,
            )),
            then_expr: Box::new(namespace_expr(
                then_expr,
                context,
                local_names,
                local_type_names,
            )),
            else_expr: Box::new(namespace_expr(
                else_expr,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::Match { value, arms } => Expr::Match {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            arms: arms
                .iter()
                .map(|arm| {
                    let mut arm_local_names = local_names.clone();
                    if let Some(pattern) = &arm.pattern {
                        collect_match_pattern_names(pattern, &mut arm_local_names);
                    }
                    MatchArm {
                        pattern: arm.pattern.as_ref().map(|pattern| {
                            namespace_expr(pattern, context, &arm_local_names, local_type_names)
                        }),
                        guard: arm.guard.as_ref().map(|guard| {
                            namespace_expr(guard, context, &arm_local_names, local_type_names)
                        }),
                        expr: namespace_expr(
                            &arm.expr,
                            context,
                            &arm_local_names,
                            local_type_names,
                        ),
                    }
                })
                .collect(),
        },
        Expr::MatchGuardResult(value) => Expr::MatchGuardResult(Box::new(namespace_expr(
            value,
            context,
            local_names,
            local_type_names,
        ))),
        Expr::Binary { left, op, right } => Expr::Binary {
            left: Box::new(namespace_expr(left, context, local_names, local_type_names)),
            op: *op,
            right: Box::new(namespace_expr(
                right,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::Not(expr) => Expr::Not(Box::new(namespace_expr(
            expr,
            context,
            local_names,
            local_type_names,
        ))),
        Expr::BitNot(expr) => Expr::BitNot(Box::new(namespace_expr(
            expr,
            context,
            local_names,
            local_type_names,
        ))),
        Expr::TupleField { name, field } => Expr::TupleField {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            field: *field,
        },
        Expr::TupleFieldValue { value, field } => Expr::TupleFieldValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            field: *field,
        },
        Expr::FieldValue { value, field } => Expr::FieldValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            field: field.clone(),
        },
        Expr::Field { name, field } => Expr::Field {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            field: field.clone(),
        },
        Expr::Value(name) => Expr::Value(qualify_ref_name(
            name,
            namespace,
            binding_names,
            local_names,
        )),
        Expr::Len(name) => Expr::Len(qualify_ref_name(
            name,
            namespace,
            binding_names,
            local_names,
        )),
        Expr::ArrayLenValue(value) => Expr::ArrayLenValue(Box::new(namespace_expr(
            value,
            context,
            local_names,
            local_type_names,
        ))),
        Expr::MapLenValue(value) => Expr::MapLenValue(Box::new(namespace_expr(
            value,
            context,
            local_names,
            local_type_names,
        ))),
        Expr::IsEmpty(name) => Expr::IsEmpty(qualify_ref_name(
            name,
            namespace,
            binding_names,
            local_names,
        )),
        Expr::ArrayIsEmptyValue(value) => Expr::ArrayIsEmptyValue(Box::new(namespace_expr(
            value,
            context,
            local_names,
            local_type_names,
        ))),
        Expr::MapIsEmptyValue(value) => Expr::MapIsEmptyValue(Box::new(namespace_expr(
            value,
            context,
            local_names,
            local_type_names,
        ))),
        Expr::ArrayFirst(name) => Expr::ArrayFirst(qualify_ref_name(
            name,
            namespace,
            binding_names,
            local_names,
        )),
        Expr::ArrayFirstValue(value) => Expr::ArrayFirstValue(Box::new(namespace_expr(
            value,
            context,
            local_names,
            local_type_names,
        ))),
        Expr::ArrayLast(name) => Expr::ArrayLast(qualify_ref_name(
            name,
            namespace,
            binding_names,
            local_names,
        )),
        Expr::ArrayLastValue(value) => Expr::ArrayLastValue(Box::new(namespace_expr(
            value,
            context,
            local_names,
            local_type_names,
        ))),
        Expr::ArrayReverse(name) => Expr::ArrayReverse(qualify_ref_name(
            name,
            namespace,
            binding_names,
            local_names,
        )),
        Expr::ArrayReverseValue(value) => Expr::ArrayReverseValue(Box::new(namespace_expr(
            value,
            context,
            local_names,
            local_type_names,
        ))),
        Expr::ArraySort(name) => Expr::ArraySort(qualify_ref_name(
            name,
            namespace,
            binding_names,
            local_names,
        )),
        Expr::ArraySortValue(value) => Expr::ArraySortValue(Box::new(namespace_expr(
            value,
            context,
            local_names,
            local_type_names,
        ))),
        Expr::ArrayUnique(name) => Expr::ArrayUnique(qualify_ref_name(
            name,
            namespace,
            binding_names,
            local_names,
        )),
        Expr::ArrayUniqueValue(value) => Expr::ArrayUniqueValue(Box::new(namespace_expr(
            value,
            context,
            local_names,
            local_type_names,
        ))),
        Expr::ArrayMap { name, mapper } => Expr::ArrayMap {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            mapper: Box::new(namespace_expr(
                mapper,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayMapValue { value, mapper } => Expr::ArrayMapValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            mapper: Box::new(namespace_expr(
                mapper,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayFilter { name, predicate } => Expr::ArrayFilter {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            predicate: Box::new(namespace_expr(
                predicate,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayFilterValue { value, predicate } => Expr::ArrayFilterValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            predicate: Box::new(namespace_expr(
                predicate,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayFlatMap { name, mapper } => Expr::ArrayFlatMap {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            mapper: Box::new(namespace_expr(
                mapper,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayFlatMapValue { value, mapper } => Expr::ArrayFlatMapValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            mapper: Box::new(namespace_expr(
                mapper,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayFind { name, predicate } => Expr::ArrayFind {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            predicate: Box::new(namespace_expr(
                predicate,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayFindValue { value, predicate } => Expr::ArrayFindValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            predicate: Box::new(namespace_expr(
                predicate,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayAny { name, predicate } => Expr::ArrayAny {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            predicate: Box::new(namespace_expr(
                predicate,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayAnyValue { value, predicate } => Expr::ArrayAnyValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            predicate: Box::new(namespace_expr(
                predicate,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayAll { name, predicate } => Expr::ArrayAll {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            predicate: Box::new(namespace_expr(
                predicate,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayAllValue { value, predicate } => Expr::ArrayAllValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            predicate: Box::new(namespace_expr(
                predicate,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayFold {
            name,
            initial,
            reducer,
        } => Expr::ArrayFold {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            initial: Box::new(namespace_expr(
                initial,
                context,
                local_names,
                local_type_names,
            )),
            reducer: Box::new(namespace_expr(
                reducer,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayFoldValue {
            value,
            initial,
            reducer,
        } => Expr::ArrayFoldValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            initial: Box::new(namespace_expr(
                initial,
                context,
                local_names,
                local_type_names,
            )),
            reducer: Box::new(namespace_expr(
                reducer,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::OptionMap { name, mapper } => Expr::OptionMap {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            mapper: Box::new(namespace_expr(
                mapper,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::OptionMapValue { value, mapper } => Expr::OptionMapValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            mapper: Box::new(namespace_expr(
                mapper,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::OptionFlatMap { name, mapper } => Expr::OptionFlatMap {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            mapper: Box::new(namespace_expr(
                mapper,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::OptionFlatMapValue { value, mapper } => Expr::OptionFlatMapValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            mapper: Box::new(namespace_expr(
                mapper,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::ResultMap { name, mapper } => Expr::ResultMap {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            mapper: Box::new(namespace_expr(
                mapper,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::ResultMapValue { value, mapper } => Expr::ResultMapValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            mapper: Box::new(namespace_expr(
                mapper,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::ResultFlatMap { name, mapper } => Expr::ResultFlatMap {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            mapper: Box::new(namespace_expr(
                mapper,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::ResultFlatMapValue { value, mapper } => Expr::ResultFlatMapValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            mapper: Box::new(namespace_expr(
                mapper,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::OptionAp { name, value } => Expr::OptionAp {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::OptionApValue { function, value } => Expr::OptionApValue {
            function: Box::new(namespace_expr(
                function,
                context,
                local_names,
                local_type_names,
            )),
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::ResultAp { name, value } => Expr::ResultAp {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::ResultApValue { function, value } => Expr::ResultApValue {
            function: Box::new(namespace_expr(
                function,
                context,
                local_names,
                local_type_names,
            )),
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::OptionOrElse { name, fallback } => Expr::OptionOrElse {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            fallback: Box::new(namespace_expr(
                fallback,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::OptionOrElseValue { value, fallback } => Expr::OptionOrElseValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            fallback: Box::new(namespace_expr(
                fallback,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::OptionOrElseTry { value, fallback } => Expr::OptionOrElseTry {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            fallback: Box::new(namespace_expr(
                fallback,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayTake { name, count } => Expr::ArrayTake {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            count: Box::new(namespace_expr(
                count,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayTakeValue { value, count } => Expr::ArrayTakeValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            count: Box::new(namespace_expr(
                count,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayDrop { name, count } => Expr::ArrayDrop {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            count: Box::new(namespace_expr(
                count,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayDropValue { value, count } => Expr::ArrayDropValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            count: Box::new(namespace_expr(
                count,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::Join { name, separator } => Expr::Join {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            separator: Box::new(namespace_expr(
                separator,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::JoinValue { value, separator } => Expr::JoinValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            separator: Box::new(namespace_expr(
                separator,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayPush { name, value } => Expr::ArrayPush {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayPop { name } => Expr::ArrayPop {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
        },
        Expr::MapSet { name, key, value } => Expr::MapSet {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            key: Box::new(namespace_expr(key, context, local_names, local_type_names)),
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::MapRemove { name, key } => Expr::MapRemove {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            key: Box::new(namespace_expr(key, context, local_names, local_type_names)),
        },
        Expr::ArrayContains { name, value } => Expr::ArrayContains {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayContainsValue { value, item } => Expr::ArrayContainsValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            item: Box::new(namespace_expr(item, context, local_names, local_type_names)),
        },
        Expr::ArrayIndexOf { name, value } => Expr::ArrayIndexOf {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayIndexOfValue { value, item } => Expr::ArrayIndexOfValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            item: Box::new(namespace_expr(item, context, local_names, local_type_names)),
        },
        Expr::MapKeys(name) => Expr::MapKeys(qualify_ref_name(
            name,
            namespace,
            binding_names,
            local_names,
        )),
        Expr::MapKeysValue(value) => Expr::MapKeysValue(Box::new(namespace_expr(
            value,
            context,
            local_names,
            local_type_names,
        ))),
        Expr::MapValues(name) => Expr::MapValues(qualify_ref_name(
            name,
            namespace,
            binding_names,
            local_names,
        )),
        Expr::MapValuesValue(value) => Expr::MapValuesValue(Box::new(namespace_expr(
            value,
            context,
            local_names,
            local_type_names,
        ))),
        Expr::MapHas { name, key } => Expr::MapHas {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            key: Box::new(namespace_expr(key, context, local_names, local_type_names)),
        },
        Expr::MapHasValue { value, key } => Expr::MapHasValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            key: Box::new(namespace_expr(key, context, local_names, local_type_names)),
        },
        Expr::StringContains { name, needle } => Expr::StringContains {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            needle: Box::new(namespace_expr(
                needle,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::StringContainsValue { value, needle } => Expr::StringContainsValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            needle: Box::new(namespace_expr(
                needle,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::StringIndexOf { name, needle } => Expr::StringIndexOf {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            needle: Box::new(namespace_expr(
                needle,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::StringIndexOfValue { value, needle } => Expr::StringIndexOfValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            needle: Box::new(namespace_expr(
                needle,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::StringStartsWith { name, prefix } => Expr::StringStartsWith {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            prefix: Box::new(namespace_expr(
                prefix,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::StringStartsWithValue { value, prefix } => Expr::StringStartsWithValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            prefix: Box::new(namespace_expr(
                prefix,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::StringEndsWith { name, suffix } => Expr::StringEndsWith {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            suffix: Box::new(namespace_expr(
                suffix,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::StringEndsWithValue { value, suffix } => Expr::StringEndsWithValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            suffix: Box::new(namespace_expr(
                suffix,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::StringLen(name) => Expr::StringLen(qualify_ref_name(
            name,
            namespace,
            binding_names,
            local_names,
        )),
        Expr::StringLenValue(value) => Expr::StringLenValue(Box::new(namespace_expr(
            value,
            context,
            local_names,
            local_type_names,
        ))),
        Expr::StringIsEmpty(name) => Expr::StringIsEmpty(qualify_ref_name(
            name,
            namespace,
            binding_names,
            local_names,
        )),
        Expr::StringIsEmptyValue(value) => Expr::StringIsEmptyValue(Box::new(namespace_expr(
            value,
            context,
            local_names,
            local_type_names,
        ))),
        Expr::StringSlice { name, start, end } => Expr::StringSlice {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            start: Box::new(namespace_expr(
                start,
                context,
                local_names,
                local_type_names,
            )),
            end: Box::new(namespace_expr(end, context, local_names, local_type_names)),
        },
        Expr::StringSliceValue { value, start, end } => Expr::StringSliceValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            start: Box::new(namespace_expr(
                start,
                context,
                local_names,
                local_type_names,
            )),
            end: Box::new(namespace_expr(end, context, local_names, local_type_names)),
        },
        Expr::StringTrim(name) => Expr::StringTrim(qualify_ref_name(
            name,
            namespace,
            binding_names,
            local_names,
        )),
        Expr::StringTrimValue(value) => Expr::StringTrimValue(Box::new(namespace_expr(
            value,
            context,
            local_names,
            local_type_names,
        ))),
        Expr::StringTrimStart(name) => Expr::StringTrimStart(qualify_ref_name(
            name,
            namespace,
            binding_names,
            local_names,
        )),
        Expr::StringTrimStartValue(value) => Expr::StringTrimStartValue(Box::new(namespace_expr(
            value,
            context,
            local_names,
            local_type_names,
        ))),
        Expr::StringTrimEnd(name) => Expr::StringTrimEnd(qualify_ref_name(
            name,
            namespace,
            binding_names,
            local_names,
        )),
        Expr::StringTrimEndValue(value) => Expr::StringTrimEndValue(Box::new(namespace_expr(
            value,
            context,
            local_names,
            local_type_names,
        ))),
        Expr::StringToUpper(name) => Expr::StringToUpper(qualify_ref_name(
            name,
            namespace,
            binding_names,
            local_names,
        )),
        Expr::StringToUpperValue(value) => Expr::StringToUpperValue(Box::new(namespace_expr(
            value,
            context,
            local_names,
            local_type_names,
        ))),
        Expr::StringToLower(name) => Expr::StringToLower(qualify_ref_name(
            name,
            namespace,
            binding_names,
            local_names,
        )),
        Expr::StringToLowerValue(value) => Expr::StringToLowerValue(Box::new(namespace_expr(
            value,
            context,
            local_names,
            local_type_names,
        ))),
        Expr::StringRepeat { name, count } => Expr::StringRepeat {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            count: Box::new(namespace_expr(
                count,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::StringRepeatValue { value, count } => Expr::StringRepeatValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            count: Box::new(namespace_expr(
                count,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::StringSplit { name, separator } => Expr::StringSplit {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            separator: Box::new(namespace_expr(
                separator,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::StringSplitValue { value, separator } => Expr::StringSplitValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            separator: Box::new(namespace_expr(
                separator,
                context,
                local_names,
                local_type_names,
            )),
        },
        Expr::StringReplace { name, from, to } => Expr::StringReplace {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            from: Box::new(namespace_expr(from, context, local_names, local_type_names)),
            to: Box::new(namespace_expr(to, context, local_names, local_type_names)),
        },
        Expr::StringReplaceValue { value, from, to } => Expr::StringReplaceValue {
            value: Box::new(namespace_expr(
                value,
                context,
                local_names,
                local_type_names,
            )),
            from: Box::new(namespace_expr(from, context, local_names, local_type_names)),
            to: Box::new(namespace_expr(to, context, local_names, local_type_names)),
        },
        Expr::Ident(name) if local_names.contains(name) => Expr::Ident(name.clone()),
        Expr::Ident(name) if function_names.contains(name) => {
            Expr::Ident(qualify_function(name, namespace, function_names))
        }
        Expr::Ident(name) if binding_names.contains(name) => {
            Expr::Ident(qualify_binding(name, namespace, binding_names))
        }
        Expr::TryResult(value) => Expr::TryResult(Box::new(namespace_expr(
            value,
            context,
            local_names,
            local_type_names,
        ))),
        other => other.clone(),
    }
}

fn qualify_function(name: &str, namespace: &str, function_names: &HashSet<String>) -> String {
    if function_names.contains(name) {
        if is_private_decl(name) {
            private_function_name(namespace, name)
        } else {
            format!("{namespace}.{name}")
        }
    } else {
        name.to_string()
    }
}

fn qualify_binding(name: &str, namespace: &str, binding_names: &HashSet<String>) -> String {
    if name != "_" && binding_names.contains(name) {
        if is_private_decl(name) {
            private_binding_name(namespace, name)
        } else {
            format!("{namespace}_{name}")
        }
    } else {
        name.to_string()
    }
}

fn is_private_decl(name: &str) -> bool {
    name.starts_with('_') && name != "_"
}

fn private_function_name(namespace: &str, name: &str) -> String {
    format!("{namespace}.__nacre-private{name}")
}

fn private_binding_name(namespace: &str, name: &str) -> String {
    let bare = name.trim_start_matches('_');
    format!("__nacre_private_{namespace}_{bare}")
}

fn qualify_type_name(
    name: &str,
    namespace: &str,
    type_names: &HashSet<String>,
    local_type_names: &HashSet<String>,
) -> String {
    if local_type_names.contains(name) {
        name.to_string()
    } else if type_names.contains(name) {
        format!("{namespace}.{name}")
    } else {
        name.to_string()
    }
}

fn qualify_trait_name(name: &str, namespace: &str, trait_names: &HashSet<String>) -> String {
    if trait_names.contains(name) {
        format!("{namespace}.{name}")
    } else {
        name.to_string()
    }
}

fn qualify_decl_name(
    name: &str,
    namespace: &str,
    binding_names: &HashSet<String>,
    top_level: bool,
) -> String {
    if top_level {
        qualify_binding(name, namespace, binding_names)
    } else {
        name.to_string()
    }
}

fn qualify_ref_name(
    name: &str,
    namespace: &str,
    binding_names: &HashSet<String>,
    local_names: &HashSet<String>,
) -> String {
    if local_names.contains(name) {
        name.to_string()
    } else {
        qualify_binding(name, namespace, binding_names)
    }
}

fn qualify_call_name(
    name: &str,
    namespace: &str,
    function_names: &HashSet<String>,
    binding_names: &HashSet<String>,
    trait_names: &HashSet<String>,
    local_names: &HashSet<String>,
) -> String {
    if function_names.contains(name) {
        return qualify_function(name, namespace, function_names);
    }
    if binding_names.contains(name) && !local_names.contains(name) {
        return qualify_binding(name, namespace, binding_names);
    }
    let Some((receiver, method)) = name.rsplit_once('.') else {
        return name.to_string();
    };
    if trait_names.contains(receiver) {
        return format!(
            "{}.{method}",
            qualify_trait_name(receiver, namespace, trait_names)
        );
    }
    if binding_names.contains(receiver) && !local_names.contains(receiver) {
        format!(
            "{}.{method}",
            qualify_binding(receiver, namespace, binding_names)
        )
    } else {
        name.to_string()
    }
}

fn namespace_interpolations(
    value: &str,
    namespace: &str,
    binding_names: &HashSet<String>,
    local_names: &HashSet<String>,
) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start + 2]);
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find('}') else {
            out.push_str(after_start);
            return out;
        };
        let value = &after_start[..end];
        out.push_str(&namespace_interpolation_ref(
            value,
            namespace,
            binding_names,
            local_names,
        ));
        out.push('}');
        rest = &after_start[end + 1..];
    }
    out.push_str(rest);
    out
}

fn namespace_interpolation_ref(
    value: &str,
    namespace: &str,
    binding_names: &HashSet<String>,
    local_names: &HashSet<String>,
) -> String {
    if let Some((base, index)) = value
        .strip_suffix(']')
        .and_then(|value| value.split_once('['))
    {
        return format!(
            "{}[{index}]",
            qualify_ref_name(base, namespace, binding_names, local_names)
        );
    }
    if let Some((base, field)) = value.split_once('.') {
        let separator = if field.starts_with('_') { "" } else { "_" };
        return format!(
            "{}{separator}{field}",
            qualify_ref_name(base, namespace, binding_names, local_names)
        );
    }
    qualify_ref_name(value, namespace, binding_names, local_names)
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use crate::{compile_file, compile_source};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn compile_file_reads_source() {
        let path = temp_path("compile-file.ncr");
        fs::write(&path, "const x = 1\n").unwrap();
        let bash = compile_file(&path).unwrap();
        fs::remove_file(&path).unwrap();

        assert!(bash.contains("readonly x=1"));
    }

    #[test]
    fn compile_file_inlines_used_modules() {
        let root = temp_path("modules");
        let lib = root.join("lib");
        fs::create_dir_all(&lib).unwrap();
        let module = lib.join("utils.ncr");
        let main = root.join("main.ncr");
        fs::write(
            &module,
            r#"
fn label(value: String): String {
return "module ${value}"
}
"#,
        )
        .unwrap();
        fs::write(
            &main,
            r#"
use lib.utils
const message = utils.label("ok")
"#,
        )
        .unwrap();

        let bash = compile_file(&main).unwrap();
        fs::remove_dir_all(&root).unwrap();

        assert!(bash.contains("utils.label() {"));
        assert!(bash.contains("readonly message=\"$(utils.label 'ok')\""));
    }

    #[test]
    fn compile_file_resolves_standard_library_modules() {
        let path = temp_path("std-modules.ncr");
        fs::write(
            &path,
            r#"
use std.path
use std.str
const name = path.basename("/tmp/nacre.txt")
const clean = str.trim(" nacre ")
"#,
        )
        .unwrap();

        let bash = compile_file(&path).unwrap();
        fs::remove_file(&path).unwrap();

        assert!(bash.contains("path.basename() {"));
        assert!(bash.contains("str.trim() {"));
        assert!(bash.contains("readonly name=\"$(path.basename '/tmp/nacre.txt')\""));
        assert!(bash.contains("readonly clean=\"$(str.trim ' nacre ')\""));
    }

    #[test]
    fn compile_file_reports_read_failure() {
        let path = temp_path("missing.ncr");
        let error = compile_file(&path).unwrap_err();

        assert_eq!(error.line(), 0);
        assert!(error.message().contains("failed to read"));
    }

    #[test]
    fn compile_source_reports_parse_failure() {
        let error = compile_source("const bad-name = 1").unwrap_err();

        assert_eq!(error.line(), 1);
        assert!(error.message().contains("invalid variable name"));
    }

    #[test]
    fn namespace_helpers_cover_local_and_unterminated_interpolation_paths() {
        let function_names = ["make".to_string()].into_iter().collect::<HashSet<_>>();
        let binding_names = ["item".to_string()].into_iter().collect::<HashSet<_>>();
        let type_names = ["UserId".to_string()].into_iter().collect::<HashSet<_>>();
        let trait_names = ["Show".to_string()].into_iter().collect::<HashSet<_>>();
        let local_names = HashSet::new();
        let local_type_names = HashSet::new();
        let context = NamespaceContext {
            namespace: "mod",
            function_names: &function_names,
            binding_names: &binding_names,
            type_names: &type_names,
            trait_names: &trait_names,
        };

        assert_eq!(
            namespace_statement(
                &Statement::Expr(Expr::Value("item".into())),
                &context,
                &local_names,
                &local_type_names,
                true,
            ),
            Statement::Expr(Expr::Value("mod_item".into()))
        );
        assert_eq!(
            namespace_statement(
                &Statement::Function {
                    name: "make".into(),
                    override_constructor: false,
                    type_params: Vec::new(),
                    params: vec![Param {
                        name: "arg".into(),
                        ty: Type::String,
                        default: Some(Expr::Ident("item".into())),
                        variadic: false,
                        capture_name: None,
                    }],
                    return_type: Type::String,
                    body: Program::new(Vec::new(), Vec::new()),
                },
                &context,
                &local_names,
                &local_type_names,
                true,
            ),
            Statement::Function {
                name: "mod.make".into(),
                override_constructor: false,
                type_params: Vec::new(),
                params: vec![Param {
                    name: "arg".into(),
                    ty: Type::String,
                    default: Some(Expr::Ident("mod_item".into())),
                    variadic: false,
                    capture_name: None,
                }],
                return_type: Type::String,
                body: Program::new(Vec::new(), Vec::new()),
            }
        );
        assert_eq!(
            namespace_expr(
                &Expr::Await("item".into()),
                &context,
                &local_names,
                &local_type_names,
            ),
            Expr::Await("mod_item".into())
        );
        assert_eq!(
            namespace_expr(
                &Expr::AsyncCommand("printf ${item}".into()),
                &context,
                &local_names,
                &local_type_names,
            ),
            Expr::AsyncCommand("printf ${item}".into())
        );
        assert_eq!(
            namespace_expr(
                &Expr::NewtypeCtor {
                    name: "UserId".into(),
                    value: Box::new(Expr::Ident("item".into())),
                },
                &context,
                &local_names,
                &local_type_names,
            ),
            Expr::NewtypeCtor {
                name: "mod.UserId".into(),
                value: Box::new(Expr::Ident("mod_item".into())),
            }
        );
        assert_eq!(
            namespace_expr(
                &Expr::Call {
                    name: "item.method".into(),
                    args: vec![Expr::Ident("item".into())],
                },
                &context,
                &local_names,
                &local_type_names,
            ),
            Expr::Call {
                name: "mod_item.method".into(),
                args: vec![Expr::Ident("mod_item".into())],
            }
        );
        assert_eq!(
            namespace_expr(
                &Expr::Call {
                    name: "other.method".into(),
                    args: Vec::new(),
                },
                &context,
                &local_names,
                &local_type_names,
            ),
            Expr::Call {
                name: "other.method".into(),
                args: Vec::new(),
            }
        );
        assert_eq!(
            namespace_interpolations(
                "hello ${item} ${missing",
                "mod",
                &binding_names,
                &local_names
            ),
            "hello ${mod_item} ${missing"
        );
        assert_eq!(qualify_function("other", "mod", &function_names), "other");
        assert_eq!(qualify_binding("other", "mod", &binding_names), "other");
        assert_eq!(qualify_binding("_", "mod", &binding_names), "_");
        assert_eq!(with_local(&local_names, "_"), local_names);
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("nacre-{unique}-{name}"))
    }
}
