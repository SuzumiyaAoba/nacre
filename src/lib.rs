#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

mod ast;
mod checker;
mod emitter;
mod error;
mod lowering;
mod parser;
mod parser_peg;
mod policy;

use std::collections::HashSet;
use std::fs;
use std::path::Path;

pub use ast::{
    BinaryOp, BindingPattern, ClosureCapture, DoStep, Expr, ImplMethod, MatchArm, Param, Program,
    Statement, TraitMethod, Type, TypeParam, VariantDecl,
};
pub use checker::{type_check, type_check_with_policy};
pub use emitter::{transpile, transpile_with_policy};
pub use error::CompileError;
pub use parser::parse;
pub use policy::ExecutionPolicy;

pub fn compile_source(source: &str) -> Result<String, CompileError> {
    compile_source_with_policy(source, &ExecutionPolicy::deny_all())
}

pub fn compile_source_with_policy(
    source: &str,
    policy: &ExecutionPolicy,
) -> Result<String, CompileError> {
    let program = parse(source)?;
    let program = checker::type_check_and_lower_with_policy(&program, policy)?;
    Ok(transpile_with_policy(&program, policy))
}

pub fn compile_file(path: &Path) -> Result<String, CompileError> {
    compile_file_with_policy(path, &ExecutionPolicy::deny_all())
}

pub fn compile_file_with_policy(
    path: &Path,
    policy: &ExecutionPolicy,
) -> Result<String, CompileError> {
    let mut seen = HashSet::new();
    let program = parse_file_expanded(path, &mut seen)?;
    let program = checker::type_check_and_lower_with_policy(&program, policy)?;
    Ok(transpile_with_policy(&program, policy))
}

fn parse_file_expanded(
    path: &Path,
    seen: &mut HashSet<std::path::PathBuf>,
) -> Result<Program, CompileError> {
    let source = fs::read_to_string(path).map_err(|error| {
        CompileError::new(0, format!("failed to read {}: {error}", path.display()))
    })?;
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if !seen.insert(canonical) {
        return Ok(Program::new(Vec::new(), Vec::new()));
    }
    let program = parse(&source)?;
    expand_modules(
        program,
        path.parent().unwrap_or_else(|| Path::new(".")),
        seen,
    )
}

fn expand_modules(
    program: Program,
    base_dir: &Path,
    seen: &mut HashSet<std::path::PathBuf>,
) -> Result<Program, CompileError> {
    let mut statements = Vec::new();
    let mut lines = Vec::new();
    for (statement, line) in program.statements().iter().zip(program.statement_lines()) {
        if let Statement::Use { path } = statement {
            let module_path = resolve_module_path(base_dir, path, *line)?;
            let module = parse_file_expanded(&module_path, seen)?;
            let namespace = path.last().expect("module path is non-empty");
            let module = namespace_module(module, namespace);
            statements.extend_from_slice(module.statements());
            lines.extend_from_slice(module.statement_lines());
        } else {
            statements.push(statement.clone());
            lines.push(*line);
        }
    }
    Ok(Program::new(statements, lines))
}

fn resolve_module_path(
    base_dir: &Path,
    parts: &[String],
    line: usize,
) -> Result<std::path::PathBuf, CompileError> {
    let relative = parts.iter().collect::<std::path::PathBuf>();
    let mut roots = vec![base_dir.to_path_buf()];
    roots.push(Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf());
    if let Ok(paths) = std::env::var("NACRE_PATH") {
        roots.extend(std::env::split_paths(&paths));
    }
    for root in roots {
        let file = root.join(&relative).with_extension("ncr");
        if file.is_file() {
            return Ok(file);
        }
        let definition = root.join(&relative).with_extension("d.ncr");
        if definition.is_file() {
            return Ok(definition);
        }
        let index = root.join(&relative).join("index.ncr");
        if index.is_file() {
            return Ok(index);
        }
    }
    Err(CompileError::new(
        line,
        format!("module `{}` was not found", parts.join(".")),
    ))
}

fn namespace_module(program: Program, namespace: &str) -> Program {
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
    let statements = program
        .statements()
        .iter()
        .map(|statement| {
            namespace_statement(
                statement,
                namespace,
                &function_names,
                &binding_names,
                &type_names,
                &trait_names,
                &HashSet::new(),
                &HashSet::new(),
                true,
            )
        })
        .collect();
    Program::new(statements, program.statement_lines().to_vec())
}

fn namespace_statement(
    statement: &Statement,
    namespace: &str,
    function_names: &HashSet<String>,
    binding_names: &HashSet<String>,
    type_names: &HashSet<String>,
    trait_names: &HashSet<String>,
    local_names: &HashSet<String>,
    local_type_names: &HashSet<String>,
    top_level: bool,
) -> Statement {
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
                            namespace_expr(
                                expr,
                                namespace,
                                function_names,
                                binding_names,
                                type_names,
                                trait_names,
                                local_names,
                                &function_type_names,
                            )
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
                body: namespace_function_body(
                    body,
                    namespace,
                    function_names,
                    binding_names,
                    type_names,
                    trait_names,
                    params,
                    &function_type_names,
                ),
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
                            namespace_expr(
                                expr,
                                namespace,
                                function_names,
                                binding_names,
                                type_names,
                                trait_names,
                                local_names,
                                &function_type_names,
                            )
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
            expr: namespace_expr(
                expr,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            ),
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
            expr: namespace_expr(
                expr,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            ),
        },
        Statement::Destructure {
            mutable,
            pattern,
            expr,
        } => Statement::Destructure {
            mutable: *mutable,
            pattern: namespace_pattern(pattern, namespace, binding_names, top_level),
            expr: namespace_expr(
                expr,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            ),
        },
        Statement::Assign { name, expr } => Statement::Assign {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            expr: namespace_expr(
                expr,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            ),
        },
        Statement::Expr(expr) => Statement::Expr(namespace_expr(
            expr,
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
            local_names,
            local_type_names,
        )),
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
        Statement::SumType { name, variants } => Statement::SumType {
            name: qualify_type_name(name, namespace, type_names, local_type_names),
            variants: variants
                .iter()
                .map(|variant| VariantDecl {
                    name: qualify_function(&variant.name, namespace, function_names),
                    fields: variant
                        .fields
                        .iter()
                        .map(|field| namespace_type(field, namespace, type_names, local_type_names))
                        .collect(),
                })
                .collect(),
        },
        Statement::Newtype { name, base } => Statement::Newtype {
            name: qualify_type_name(name, namespace, type_names, local_type_names),
            base: namespace_type(base, namespace, type_names, local_type_names),
        },
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
                        namespace,
                        function_names,
                        binding_names,
                        type_names,
                        trait_names,
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
            condition: namespace_expr(
                condition,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            ),
            then_branch: namespace_program_body(
                then_branch,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            ),
            else_branch: else_branch.as_ref().map(|branch| {
                namespace_program_body(
                    branch,
                    namespace,
                    function_names,
                    binding_names,
                    type_names,
                    trait_names,
                    local_names,
                    local_type_names,
                )
            }),
        },
        Statement::Block { body } => Statement::Block {
            body: namespace_program_body(
                body,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            ),
        },
        Statement::While { condition, body } => Statement::While {
            condition: namespace_expr(
                condition,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            ),
            body: namespace_program_body(
                body,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            ),
        },
        Statement::For {
            name,
            iterable,
            body,
        } => Statement::For {
            name: name.clone(),
            iterable: namespace_expr(
                iterable,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            ),
            body: namespace_program_body(
                body,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                &with_local(local_names, name),
                local_type_names,
            ),
        },
        Statement::Return(expr) => Statement::Return(namespace_expr(
            expr,
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
            local_names,
            local_type_names,
        )),
        Statement::TryResult(expr) => Statement::TryResult(namespace_expr(
            expr,
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
            local_names,
            local_type_names,
        )),
        Statement::TryPipeline { input, commands } => Statement::TryPipeline {
            input: input.as_ref().map(|input| {
                Box::new(namespace_expr(
                    input,
                    namespace,
                    function_names,
                    binding_names,
                    type_names,
                    trait_names,
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
                    namespace,
                    function_names,
                    binding_names,
                    type_names,
                    trait_names,
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
    namespace: &str,
    function_names: &HashSet<String>,
    binding_names: &HashSet<String>,
    type_names: &HashSet<String>,
    trait_names: &HashSet<String>,
    local_names: &HashSet<String>,
    local_type_names: &HashSet<String>,
) -> Program {
    let mut block_locals = local_names.clone();
    let mut statements = Vec::new();
    for statement in program.statements() {
        statements.push(namespace_statement(
            statement,
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
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
    namespace: &str,
    function_names: &HashSet<String>,
    binding_names: &HashSet<String>,
    type_names: &HashSet<String>,
    trait_names: &HashSet<String>,
    params: &[Param],
    local_type_names: &HashSet<String>,
) -> Program {
    let local_names = params
        .iter()
        .map(|param| param.name.clone())
        .collect::<HashSet<_>>();
    namespace_program_body(
        program,
        namespace,
        function_names,
        binding_names,
        type_names,
        trait_names,
        &local_names,
        local_type_names,
    )
}

fn with_local(local_names: &HashSet<String>, name: &str) -> HashSet<String> {
    let mut names = local_names.clone();
    if name != "_" {
        names.insert(name.to_string());
    }
    names
}

fn binding_pattern_names(pattern: &BindingPattern) -> Vec<String> {
    match pattern {
        BindingPattern::Tuple(names) => names.iter().filter(|name| *name != "_").cloned().collect(),
        BindingPattern::Array { names, rest } => names
            .iter()
            .chain(rest.iter())
            .filter(|name| *name != "_")
            .cloned()
            .collect(),
        BindingPattern::Record(bindings) => bindings
            .iter()
            .filter_map(|(_, name)| (name != "_").then(|| name.clone()))
            .collect(),
    }
}

fn collect_match_pattern_names(pattern: &Expr, local_names: &mut HashSet<String>) {
    match pattern {
        Expr::Some(value) | Expr::Ok(value) | Expr::Err(value) => {
            if let Expr::Ident(name) = &**value {
                if name != "_" {
                    local_names.insert(name.clone());
                }
            }
        }
        Expr::Tuple(values) => {
            for value in values {
                if let Expr::Ident(name) = value {
                    if name != "_" {
                        local_names.insert(name.clone());
                    }
                }
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
        Expr::Call { args, .. } | Expr::Variant { args, .. } => {
            for value in args {
                if let Expr::Ident(name) = value {
                    if name != "_" {
                        local_names.insert(name.clone());
                    }
                }
            }
        }
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
        BindingPattern::Tuple(names) => BindingPattern::Tuple(
            names
                .iter()
                .map(|name| qualify_decl_name(name, namespace, binding_names, top_level))
                .collect(),
        ),
        BindingPattern::Array { names, rest } => BindingPattern::Array {
            names: names
                .iter()
                .map(|name| qualify_decl_name(name, namespace, binding_names, top_level))
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
                        qualify_decl_name(name, namespace, binding_names, top_level),
                    )
                })
                .collect(),
        ),
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
    namespace: &str,
    function_names: &HashSet<String>,
    binding_names: &HashSet<String>,
    type_names: &HashSet<String>,
    trait_names: &HashSet<String>,
    local_names: &HashSet<String>,
    local_type_names: &HashSet<String>,
) -> Expr {
    match expr {
        Expr::String(value) => Expr::String(namespace_interpolations(
            value,
            namespace,
            binding_names,
            local_names,
        )),
        Expr::Array(values) => Expr::Array(
            values
                .iter()
                .map(|value| {
                    namespace_expr(
                        value,
                        namespace,
                        function_names,
                        binding_names,
                        type_names,
                        trait_names,
                        local_names,
                        local_type_names,
                    )
                })
                .collect(),
        ),
        Expr::Map(entries) => Expr::Map(
            entries
                .iter()
                .map(|(key, value)| {
                    (
                        namespace_expr(
                            key,
                            namespace,
                            function_names,
                            binding_names,
                            type_names,
                            trait_names,
                            local_names,
                            local_type_names,
                        ),
                        namespace_expr(
                            value,
                            namespace,
                            function_names,
                            binding_names,
                            type_names,
                            trait_names,
                            local_names,
                            local_type_names,
                        ),
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
                        namespace_expr(
                            value,
                            namespace,
                            function_names,
                            binding_names,
                            type_names,
                            trait_names,
                            local_names,
                            local_type_names,
                        ),
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
                            namespace_expr(
                                value,
                                namespace,
                                function_names,
                                binding_names,
                                type_names,
                                trait_names,
                                local_names,
                                local_type_names,
                            )
                        }),
                    )
                })
                .collect(),
        ),
        Expr::Tuple(values) => Expr::Tuple(
            values
                .iter()
                .map(|value| {
                    namespace_expr(
                        value,
                        namespace,
                        function_names,
                        binding_names,
                        type_names,
                        trait_names,
                        local_names,
                        local_type_names,
                    )
                })
                .collect(),
        ),
        Expr::Index { name, index } => Expr::Index {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            index: Box::new(namespace_expr(
                index,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::IndexValue { value, index } => Expr::IndexValue {
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            index: Box::new(namespace_expr(
                index,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::Slice { name, start, end } => Expr::Slice {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            start: Box::new(namespace_expr(
                start,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            end: Box::new(namespace_expr(
                end,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArraySliceValue { value, start, end } => Expr::ArraySliceValue {
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            start: Box::new(namespace_expr(
                start,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            end: Box::new(namespace_expr(
                end,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::PathExists(path) => Expr::PathExists(Box::new(namespace_expr(
            path,
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
            local_names,
            local_type_names,
        ))),
        Expr::ProcessEnv { name } => Expr::ProcessEnv {
            name: Box::new(namespace_expr(
                name,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::FsIsFile { path } => Expr::FsIsFile {
            path: Box::new(namespace_expr(
                path,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::FsIsDir { path } => Expr::FsIsDir {
            path: Box::new(namespace_expr(
                path,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::FsSize { path } => Expr::FsSize {
            path: Box::new(namespace_expr(
                path,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::FsReadLines { path } => Expr::FsReadLines {
            path: Box::new(namespace_expr(
                path,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::FsList { path } => Expr::FsList {
            path: Box::new(namespace_expr(
                path,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::FsWriteLines { path, lines } => Expr::FsWriteLines {
            path: Box::new(namespace_expr(
                path,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            lines: Box::new(namespace_expr(
                lines,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::JsonParse { value } => Expr::JsonParse {
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
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
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::AsyncCommand(_) => expr.clone(),
        Expr::CommandResult { .. } => expr.clone(),
        Expr::AllowedCommand {
            group,
            command,
            args,
            program,
            read_args,
            write_args,
        } => Expr::AllowedCommand {
            group: group.clone(),
            command: command.clone(),
            args: args
                .iter()
                .map(|arg| {
                    namespace_expr(
                        arg,
                        namespace,
                        function_names,
                        binding_names,
                        type_names,
                        trait_names,
                        local_names,
                        local_type_names,
                    )
                })
                .collect(),
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
                    namespace,
                    function_names,
                    binding_names,
                    type_names,
                    trait_names,
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
                    namespace,
                    function_names,
                    binding_names,
                    type_names,
                    trait_names,
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
                    namespace,
                    function_names,
                    binding_names,
                    type_names,
                    trait_names,
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
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
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
                .map(|arg| {
                    namespace_expr(
                        arg,
                        namespace,
                        function_names,
                        binding_names,
                        type_names,
                        trait_names,
                        local_names,
                        local_type_names,
                    )
                })
                .collect(),
            field_types: field_types
                .iter()
                .map(|ty| namespace_type(ty, namespace, type_names, local_type_names))
                .collect(),
        },
        Expr::Cast { expr, ty } => Expr::Cast {
            expr: Box::new(namespace_expr(
                expr,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            ty: namespace_type(ty, namespace, type_names, local_type_names),
        },
        Expr::Lambda { params, body } => {
            let mut lambda_locals = local_names.clone();
            lambda_locals.extend(params.iter().cloned());
            Expr::Lambda {
                params: params.clone(),
                body: Box::new(namespace_expr(
                    body,
                    namespace,
                    function_names,
                    binding_names,
                    type_names,
                    trait_names,
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
                            expr: namespace_expr(
                                expr,
                                namespace,
                                function_names,
                                binding_names,
                                type_names,
                                trait_names,
                                &do_locals,
                                local_type_names,
                            ),
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
                            expr: namespace_expr(
                                expr,
                                namespace,
                                function_names,
                                binding_names,
                                type_names,
                                trait_names,
                                &do_locals,
                                local_type_names,
                            ),
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
                    namespace,
                    function_names,
                    binding_names,
                    type_names,
                    trait_names,
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
                    namespace,
                    function_names,
                    binding_names,
                    type_names,
                    trait_names,
                    local_names,
                    local_type_names,
                )),
                body: Box::new(namespace_expr(
                    body,
                    namespace,
                    function_names,
                    binding_names,
                    type_names,
                    trait_names,
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
                .map(|arg| {
                    namespace_expr(
                        arg,
                        namespace,
                        function_names,
                        binding_names,
                        type_names,
                        trait_names,
                        local_names,
                        local_type_names,
                    )
                })
                .collect(),
        },
        Expr::Some(value) => Expr::Some(Box::new(namespace_expr(
            value,
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
            local_names,
            local_type_names,
        ))),
        Expr::Ok(value) => Expr::Ok(Box::new(namespace_expr(
            value,
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
            local_names,
            local_type_names,
        ))),
        Expr::Err(value) => Expr::Err(Box::new(namespace_expr(
            value,
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
            local_names,
            local_type_names,
        ))),
        Expr::ResultOption(value) => Expr::ResultOption(Box::new(namespace_expr(
            value,
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
            local_names,
            local_type_names,
        ))),
        Expr::Default { value, fallback } => Expr::Default {
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            fallback: Box::new(namespace_expr(
                fallback,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::DefaultTry { value, fallback } => Expr::DefaultTry {
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            fallback: Box::new(namespace_expr(
                fallback,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
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
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            then_expr: Box::new(namespace_expr(
                then_expr,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            else_expr: Box::new(namespace_expr(
                else_expr,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::Match { value, arms } => Expr::Match {
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
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
                            namespace_expr(
                                pattern,
                                namespace,
                                function_names,
                                binding_names,
                                type_names,
                                trait_names,
                                &arm_local_names,
                                local_type_names,
                            )
                        }),
                        guard: arm.guard.as_ref().map(|guard| {
                            namespace_expr(
                                guard,
                                namespace,
                                function_names,
                                binding_names,
                                type_names,
                                trait_names,
                                &arm_local_names,
                                local_type_names,
                            )
                        }),
                        expr: namespace_expr(
                            &arm.expr,
                            namespace,
                            function_names,
                            binding_names,
                            type_names,
                            trait_names,
                            &arm_local_names,
                            local_type_names,
                        ),
                    }
                })
                .collect(),
        },
        Expr::MatchGuardResult(value) => Expr::MatchGuardResult(Box::new(namespace_expr(
            value,
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
            local_names,
            local_type_names,
        ))),
        Expr::Binary { left, op, right } => Expr::Binary {
            left: Box::new(namespace_expr(
                left,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            op: *op,
            right: Box::new(namespace_expr(
                right,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::Not(expr) => Expr::Not(Box::new(namespace_expr(
            expr,
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
            local_names,
            local_type_names,
        ))),
        Expr::BitNot(expr) => Expr::BitNot(Box::new(namespace_expr(
            expr,
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
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
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            field: *field,
        },
        Expr::FieldValue { value, field } => Expr::FieldValue {
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
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
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
            local_names,
            local_type_names,
        ))),
        Expr::MapLenValue(value) => Expr::MapLenValue(Box::new(namespace_expr(
            value,
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
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
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
            local_names,
            local_type_names,
        ))),
        Expr::MapIsEmptyValue(value) => Expr::MapIsEmptyValue(Box::new(namespace_expr(
            value,
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
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
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
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
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
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
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
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
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
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
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
            local_names,
            local_type_names,
        ))),
        Expr::ArrayMap { name, mapper } => Expr::ArrayMap {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            mapper: Box::new(namespace_expr(
                mapper,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayMapValue { value, mapper } => Expr::ArrayMapValue {
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            mapper: Box::new(namespace_expr(
                mapper,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::OptionMap { name, mapper } => Expr::OptionMap {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            mapper: Box::new(namespace_expr(
                mapper,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::OptionMapValue { value, mapper } => Expr::OptionMapValue {
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            mapper: Box::new(namespace_expr(
                mapper,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::OptionFlatMap { name, mapper } => Expr::OptionFlatMap {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            mapper: Box::new(namespace_expr(
                mapper,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::OptionFlatMapValue { value, mapper } => Expr::OptionFlatMapValue {
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            mapper: Box::new(namespace_expr(
                mapper,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::ResultMap { name, mapper } => Expr::ResultMap {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            mapper: Box::new(namespace_expr(
                mapper,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::ResultMapValue { value, mapper } => Expr::ResultMapValue {
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            mapper: Box::new(namespace_expr(
                mapper,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::ResultFlatMap { name, mapper } => Expr::ResultFlatMap {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            mapper: Box::new(namespace_expr(
                mapper,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::ResultFlatMapValue { value, mapper } => Expr::ResultFlatMapValue {
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            mapper: Box::new(namespace_expr(
                mapper,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::OptionAp { name, value } => Expr::OptionAp {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::OptionApValue { function, value } => Expr::OptionApValue {
            function: Box::new(namespace_expr(
                function,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::ResultAp { name, value } => Expr::ResultAp {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::ResultApValue { function, value } => Expr::ResultApValue {
            function: Box::new(namespace_expr(
                function,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::OptionOrElse { name, fallback } => Expr::OptionOrElse {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            fallback: Box::new(namespace_expr(
                fallback,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::OptionOrElseValue { value, fallback } => Expr::OptionOrElseValue {
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            fallback: Box::new(namespace_expr(
                fallback,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::OptionOrElseTry { value, fallback } => Expr::OptionOrElseTry {
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            fallback: Box::new(namespace_expr(
                fallback,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayTake { name, count } => Expr::ArrayTake {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            count: Box::new(namespace_expr(
                count,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayTakeValue { value, count } => Expr::ArrayTakeValue {
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            count: Box::new(namespace_expr(
                count,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayDrop { name, count } => Expr::ArrayDrop {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            count: Box::new(namespace_expr(
                count,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayDropValue { value, count } => Expr::ArrayDropValue {
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            count: Box::new(namespace_expr(
                count,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::Join { name, separator } => Expr::Join {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            separator: Box::new(namespace_expr(
                separator,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::JoinValue { value, separator } => Expr::JoinValue {
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            separator: Box::new(namespace_expr(
                separator,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayPush { name, value } => Expr::ArrayPush {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayPop { name } => Expr::ArrayPop {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
        },
        Expr::MapSet { name, key, value } => Expr::MapSet {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            key: Box::new(namespace_expr(
                key,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::MapRemove { name, key } => Expr::MapRemove {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            key: Box::new(namespace_expr(
                key,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayContains { name, value } => Expr::ArrayContains {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayContainsValue { value, item } => Expr::ArrayContainsValue {
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            item: Box::new(namespace_expr(
                item,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayIndexOf { name, value } => Expr::ArrayIndexOf {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::ArrayIndexOfValue { value, item } => Expr::ArrayIndexOfValue {
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            item: Box::new(namespace_expr(
                item,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::MapKeys(name) => Expr::MapKeys(qualify_ref_name(
            name,
            namespace,
            binding_names,
            local_names,
        )),
        Expr::MapKeysValue(value) => Expr::MapKeysValue(Box::new(namespace_expr(
            value,
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
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
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
            local_names,
            local_type_names,
        ))),
        Expr::MapHas { name, key } => Expr::MapHas {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            key: Box::new(namespace_expr(
                key,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::MapHasValue { value, key } => Expr::MapHasValue {
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            key: Box::new(namespace_expr(
                key,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::StringContains { name, needle } => Expr::StringContains {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            needle: Box::new(namespace_expr(
                needle,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::StringContainsValue { value, needle } => Expr::StringContainsValue {
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            needle: Box::new(namespace_expr(
                needle,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::StringIndexOf { name, needle } => Expr::StringIndexOf {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            needle: Box::new(namespace_expr(
                needle,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::StringIndexOfValue { value, needle } => Expr::StringIndexOfValue {
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            needle: Box::new(namespace_expr(
                needle,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::StringStartsWith { name, prefix } => Expr::StringStartsWith {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            prefix: Box::new(namespace_expr(
                prefix,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::StringStartsWithValue { value, prefix } => Expr::StringStartsWithValue {
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            prefix: Box::new(namespace_expr(
                prefix,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::StringEndsWith { name, suffix } => Expr::StringEndsWith {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            suffix: Box::new(namespace_expr(
                suffix,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::StringEndsWithValue { value, suffix } => Expr::StringEndsWithValue {
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            suffix: Box::new(namespace_expr(
                suffix,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
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
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
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
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
            local_names,
            local_type_names,
        ))),
        Expr::StringSlice { name, start, end } => Expr::StringSlice {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            start: Box::new(namespace_expr(
                start,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            end: Box::new(namespace_expr(
                end,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::StringSliceValue { value, start, end } => Expr::StringSliceValue {
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            start: Box::new(namespace_expr(
                start,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            end: Box::new(namespace_expr(
                end,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::StringTrim(name) => Expr::StringTrim(qualify_ref_name(
            name,
            namespace,
            binding_names,
            local_names,
        )),
        Expr::StringTrimValue(value) => Expr::StringTrimValue(Box::new(namespace_expr(
            value,
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
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
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
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
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
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
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
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
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
            local_names,
            local_type_names,
        ))),
        Expr::StringRepeat { name, count } => Expr::StringRepeat {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            count: Box::new(namespace_expr(
                count,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::StringRepeatValue { value, count } => Expr::StringRepeatValue {
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            count: Box::new(namespace_expr(
                count,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::StringSplit { name, separator } => Expr::StringSplit {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            separator: Box::new(namespace_expr(
                separator,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::StringSplitValue { value, separator } => Expr::StringSplitValue {
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            separator: Box::new(namespace_expr(
                separator,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::StringReplace { name, from, to } => Expr::StringReplace {
            name: qualify_ref_name(name, namespace, binding_names, local_names),
            from: Box::new(namespace_expr(
                from,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            to: Box::new(namespace_expr(
                to,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
        },
        Expr::StringReplaceValue { value, from, to } => Expr::StringReplaceValue {
            value: Box::new(namespace_expr(
                value,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            from: Box::new(namespace_expr(
                from,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
            to: Box::new(namespace_expr(
                to,
                namespace,
                function_names,
                binding_names,
                type_names,
                trait_names,
                local_names,
                local_type_names,
            )),
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
            namespace,
            function_names,
            binding_names,
            type_names,
            trait_names,
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
        let name = &after_start[..end];
        out.push_str(&qualify_ref_name(
            name,
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

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use std::env;
    use std::ffi::OsString;
    use std::fs;
    use std::process::Command;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    static NACRE_PATH_LOCK: Mutex<()> = Mutex::new(());

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
use std.fs
use std.log
use std.path
use std.str
const exists = fs.exists("/tmp")
const tmp = fs.createTempDir()
const name = path.basename("/tmp/nacre.txt")
const clean = str.trim(" nacre ")
log.info("checked")
"#,
        )
        .unwrap();

        let bash = compile_file(&path).unwrap();
        fs::remove_file(&path).unwrap();

        assert!(bash.contains("fs.exists() {"));
        assert!(bash.contains("fs.createTempDir() {"));
        assert!(bash.contains("log.info() {"));
        assert!(bash.contains("path.basename() {"));
        assert!(bash.contains("str.trim() {"));
        assert!(bash.contains("readonly exists=\"$(fs.exists '/tmp')\""));
        assert!(bash.contains("readonly tmp=\"$(fs.createTempDir)\""));
        assert!(bash.contains("readonly name=\"$(path.basename '/tmp/nacre.txt')\""));
        assert!(bash.contains("readonly clean=\"$(str.trim ' nacre ')\""));
        assert!(bash.contains("log.info 'checked'"));
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
    fn module_expansion_handles_repeated_files_and_nacre_path() {
        let _guard = NACRE_PATH_LOCK.lock().unwrap();
        let root = temp_path("nacre-path-modules");
        let main_dir = temp_path("nacre-path-main");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&main_dir).unwrap();
        let module = root.join("shared.ncr");
        let main = main_dir.join("main.ncr");
        fs::write(&module, "use shared\nconst sharedValue = \"ok\"\n").unwrap();
        fs::write(&main, "use shared\n").unwrap();

        let previous = env::var_os("NACRE_PATH");
        env::set_var("NACRE_PATH", &root);
        let bash = compile_file(&main).unwrap();
        restore_nacre_path(previous);
        fs::remove_dir_all(&root).unwrap();
        fs::remove_dir_all(&main_dir).unwrap();

        assert!(bash.contains("readonly shared_sharedValue='ok'"));
    }

    #[test]
    fn module_expansion_restores_existing_nacre_path() {
        let _guard = NACRE_PATH_LOCK.lock().unwrap();
        let root = temp_path("nacre-path-existing-modules");
        let main_dir = temp_path("nacre-path-existing-main");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&main_dir).unwrap();
        let module = root.join("shared.ncr");
        let main = main_dir.join("main.ncr");
        fs::write(&module, "const sharedValue = \"ok\"\n").unwrap();
        fs::write(&main, "use shared\n").unwrap();

        let original = env::var_os("NACRE_PATH");
        env::set_var("NACRE_PATH", &root);
        let previous = env::var_os("NACRE_PATH");
        env::set_var("NACRE_PATH", &root);
        let bash = compile_file(&main).unwrap();
        restore_nacre_path(previous);
        restore_nacre_path(None);
        restore_nacre_path(original);
        fs::remove_dir_all(&root).unwrap();
        fs::remove_dir_all(&main_dir).unwrap();

        assert!(bash.contains("readonly shared_sharedValue='ok'"));
    }

    fn restore_nacre_path(value: Option<OsString>) {
        if let Some(value) = value {
            env::set_var("NACRE_PATH", value);
        } else {
            env::remove_var("NACRE_PATH");
        }
    }

    #[test]
    fn namespace_helpers_cover_local_and_unterminated_interpolation_paths() {
        let function_names = ["make".to_string()].into_iter().collect::<HashSet<_>>();
        let binding_names = ["item".to_string()].into_iter().collect::<HashSet<_>>();
        let type_names = ["UserId".to_string()].into_iter().collect::<HashSet<_>>();
        let trait_names = ["Show".to_string()].into_iter().collect::<HashSet<_>>();
        let local_names = HashSet::new();
        let local_type_names = HashSet::new();

        assert_eq!(
            namespace_statement(
                &Statement::Expr(Expr::Value("item".into())),
                "mod",
                &function_names,
                &binding_names,
                &type_names,
                &trait_names,
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
                "mod",
                &function_names,
                &binding_names,
                &type_names,
                &trait_names,
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
                "mod",
                &function_names,
                &binding_names,
                &type_names,
                &trait_names,
                &local_names,
                &local_type_names,
            ),
            Expr::Await("mod_item".into())
        );
        assert_eq!(
            namespace_expr(
                &Expr::AsyncCommand("printf ${item}".into()),
                "mod",
                &function_names,
                &binding_names,
                &type_names,
                &trait_names,
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
                "mod",
                &function_names,
                &binding_names,
                &type_names,
                &trait_names,
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
                "mod",
                &function_names,
                &binding_names,
                &type_names,
                &trait_names,
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
                "mod",
                &function_names,
                &binding_names,
                &type_names,
                &trait_names,
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

    #[test]
    fn self_compiles_bootstrap_source() {
        let source = compile_file(Path::new("bootstrap/self.ncr")).unwrap();
        let compiler_path = temp_path("nacre-self.sh");
        let output_path = temp_path("nacre-self-out.sh");
        fs::write(&compiler_path, &source).unwrap();

        let status = Command::new("bash")
            .arg(&compiler_path)
            .arg("bootstrap/self.ncr")
            .arg(&output_path)
            .status()
            .unwrap();
        assert!(status.success());

        let bootstrapped = fs::read_to_string(&output_path).unwrap();
        fs::remove_file(&compiler_path).unwrap();
        fs::remove_file(&output_path).unwrap();

        assert_eq!(bootstrapped, source);
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("nacre-{unique}-{name}"))
    }
}
