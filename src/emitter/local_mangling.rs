use std::collections::HashMap;

use crate::{
    BindingPattern, ClosureCapture, Expr, ForBinding, MatchArm, Param, Program, Statement,
};

#[derive(Debug)]
pub(super) struct LocalMangler {
    prefix: String,
    next: usize,
}

impl LocalMangler {
    pub(super) fn new(function_name: &str) -> Self {
        Self {
            prefix: format!("__nacre_local_{}_", sanitize_shell_ident(function_name)),
            next: 0,
        }
    }

    fn fresh(&mut self, name: &str) -> String {
        let shell_name = format!(
            "{}{}_{}",
            self.prefix,
            self.next,
            sanitize_shell_ident(name)
        );
        self.next += 1;
        shell_name
    }
}

pub(super) fn mangle_function_locals(program: &Program) -> Program {
    Program::new(
        program
            .statements()
            .iter()
            .map(mangle_top_level_statement)
            .collect(),
        program.statement_lines().to_vec(),
    )
}

fn mangle_top_level_statement(statement: &Statement) -> Statement {
    match statement {
        Statement::Function {
            name,
            override_constructor,
            type_params,
            params,
            return_type,
            body,
        } => {
            let (params, body) = mangle_callable_locals(name, params, body);
            Statement::Function {
                name: name.clone(),
                override_constructor: *override_constructor,
                type_params: type_params.clone(),
                params,
                return_type: return_type.clone(),
                body,
            }
        }
        Statement::Impl {
            trait_name,
            for_type,
            methods,
        } => Statement::Impl {
            trait_name: trait_name.clone(),
            for_type: for_type.clone(),
            methods: methods
                .iter()
                .map(|method| {
                    let (params, body) =
                        mangle_callable_locals(&method.name, &method.params, &method.body);
                    crate::ImplMethod {
                        name: method.name.clone(),
                        params,
                        return_type: method.return_type.clone(),
                        body,
                    }
                })
                .collect(),
        },
        Statement::Block { body } => Statement::Block {
            body: Program::new(
                body.statements()
                    .iter()
                    .map(mangle_top_level_statement)
                    .collect(),
                body.statement_lines().to_vec(),
            ),
        },
        other => other.clone(),
    }
}

fn mangle_callable_locals(name: &str, params: &[Param], body: &Program) -> (Vec<Param>, Program) {
    let mut mangler = LocalMangler::new(name);
    let mut locals = HashMap::new();
    let params = params
        .iter()
        .map(|param| {
            let default = param
                .default
                .as_ref()
                .map(|expr| mangle_local_expr(expr, &locals));
            let mangled_name = param
                .capture_name
                .clone()
                .unwrap_or_else(|| mangler.fresh(&param.name));
            locals.insert(param.name.clone(), mangled_name.clone());
            Param {
                name: mangled_name,
                ty: param.ty.clone(),
                default,
                variadic: param.variadic,
                capture_name: param.capture_name.clone(),
            }
        })
        .collect();
    let body = mangle_local_program(body, &mut mangler, &locals);
    (params, body)
}

fn mangle_local_program(
    program: &Program,
    mangler: &mut LocalMangler,
    local_names: &HashMap<String, String>,
) -> Program {
    let mut block_locals = local_names.clone();
    let mut statements = Vec::new();
    for statement in program.statements() {
        statements.push(mangle_local_statement(
            statement,
            mangler,
            &mut block_locals,
        ));
    }
    Program::new(statements, program.statement_lines().to_vec())
}

pub(super) fn mangle_local_statement(
    statement: &Statement,
    mangler: &mut LocalMangler,
    local_names: &mut HashMap<String, String>,
) -> Statement {
    match statement {
        Statement::Function { .. } | Statement::Impl { .. } => {
            mangle_top_level_statement(statement)
        }
        Statement::Const {
            name,
            annotation,
            expr,
        } => {
            let expr = mangle_local_expr(expr, local_names);
            if name == "_" {
                return Statement::Const {
                    name: name.clone(),
                    annotation: annotation.clone(),
                    expr,
                };
            }
            let mangled_name = mangler.fresh(name);
            local_names.insert(name.clone(), mangled_name.clone());
            Statement::Const {
                name: mangled_name,
                annotation: annotation.clone(),
                expr,
            }
        }
        Statement::Let {
            name,
            annotation,
            expr,
        } => {
            let expr = mangle_local_expr(expr, local_names);
            if name == "_" {
                return Statement::Let {
                    name: name.clone(),
                    annotation: annotation.clone(),
                    expr,
                };
            }
            let mangled_name = mangler.fresh(name);
            local_names.insert(name.clone(), mangled_name.clone());
            Statement::Let {
                name: mangled_name,
                annotation: annotation.clone(),
                expr,
            }
        }
        Statement::Destructure {
            mutable,
            pattern,
            expr,
        } => {
            let expr = mangle_local_expr(expr, local_names);
            let pattern = mangle_local_pattern(pattern, mangler, local_names);
            Statement::Destructure {
                mutable: *mutable,
                pattern,
                expr,
            }
        }
        Statement::Assign { name, expr } => Statement::Assign {
            name: mangle_local_name(name, local_names),
            expr: mangle_local_expr(expr, local_names),
        },
        Statement::Expr(expr) => Statement::Expr(mangle_local_expr(expr, local_names)),
        Statement::TryCommand(command) => {
            Statement::TryCommand(mangle_shell_interpolations(command, local_names))
        }
        Statement::TryCommandResult(command) => {
            Statement::TryCommandResult(mangle_shell_interpolations(command, local_names))
        }
        Statement::TryResult(expr) => Statement::TryResult(mangle_local_expr(expr, local_names)),
        Statement::TryPipeline { input, commands } => Statement::TryPipeline {
            input: input
                .as_ref()
                .map(|input| Box::new(mangle_local_expr(input, local_names))),
            commands: commands
                .iter()
                .map(|command| mangle_shell_interpolations(command, local_names))
                .collect(),
        },
        Statement::TryPipelineResult { input, commands } => Statement::TryPipelineResult {
            input: input
                .as_ref()
                .map(|input| Box::new(mangle_local_expr(input, local_names))),
            commands: commands
                .iter()
                .map(|command| mangle_shell_interpolations(command, local_names))
                .collect(),
        },
        Statement::Command(command) => {
            Statement::Command(mangle_shell_interpolations(command, local_names))
        }
        Statement::Redirect {
            command,
            target,
            stderr,
            append,
        } => Statement::Redirect {
            command: mangle_shell_interpolations(command, local_names),
            target: mangle_shell_interpolations(target, local_names),
            stderr: stderr
                .as_ref()
                .map(|value| mangle_shell_interpolations(value, local_names)),
            append: *append,
        },
        Statement::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let then_locals = local_names.clone();
            let else_locals = local_names.clone();
            Statement::If {
                condition: mangle_local_expr(condition, local_names),
                then_branch: mangle_local_program(then_branch, mangler, &then_locals),
                else_branch: else_branch
                    .as_ref()
                    .map(|branch| mangle_local_program(branch, mangler, &else_locals)),
            }
        }
        Statement::Block { body } => {
            let body_locals = local_names.clone();
            Statement::Block {
                body: mangle_local_program(body, mangler, &body_locals),
            }
        }
        Statement::Defer(statement) => Statement::Defer(Box::new(mangle_local_statement(
            statement,
            mangler,
            local_names,
        ))),
        Statement::While { condition, body } => {
            let body_locals = local_names.clone();
            Statement::While {
                condition: mangle_local_expr(condition, local_names),
                body: mangle_local_program(body, mangler, &body_locals),
            }
        }
        Statement::For {
            binding,
            iterable,
            body,
        } => {
            let mut body_locals = local_names.clone();
            let mangled_binding = mangle_local_for_binding(binding, mangler, &mut body_locals);
            Statement::For {
                binding: mangled_binding,
                iterable: mangle_local_expr(iterable, local_names),
                body: mangle_local_program(body, mangler, &body_locals),
            }
        }
        Statement::Return(expr) => Statement::Return(mangle_local_expr(expr, local_names)),
        other => other.clone(),
    }
}

fn mangle_local_pattern(
    pattern: &BindingPattern,
    mangler: &mut LocalMangler,
    local_names: &mut HashMap<String, String>,
) -> BindingPattern {
    match pattern {
        BindingPattern::Name(name) => {
            BindingPattern::Name(mangle_local_binding_name(name, mangler, local_names))
        }
        BindingPattern::Tuple(names) => BindingPattern::Tuple(
            names
                .iter()
                .map(|pattern| mangle_local_pattern(pattern, mangler, local_names))
                .collect(),
        ),
        BindingPattern::Array { patterns, rest } => BindingPattern::Array {
            patterns: patterns
                .iter()
                .map(|pattern| mangle_local_pattern(pattern, mangler, local_names))
                .collect(),
            rest: rest
                .as_ref()
                .map(|name| mangle_local_binding_name(name, mangler, local_names)),
        },
        BindingPattern::Record(bindings) => BindingPattern::Record(
            bindings
                .iter()
                .map(|(field, name)| {
                    (
                        field.clone(),
                        mangle_local_pattern(name, mangler, local_names),
                    )
                })
                .collect(),
        ),
    }
}

fn mangle_local_for_binding(
    binding: &ForBinding,
    mangler: &mut LocalMangler,
    local_names: &mut HashMap<String, String>,
) -> ForBinding {
    match binding {
        ForBinding::Name(name) => {
            ForBinding::Name(mangle_local_binding_name(name, mangler, local_names))
        }
        ForBinding::Pattern(pattern) => {
            ForBinding::Pattern(mangle_local_pattern(pattern, mangler, local_names))
        }
    }
}

fn mangle_local_binding_name(
    name: &str,
    mangler: &mut LocalMangler,
    local_names: &mut HashMap<String, String>,
) -> String {
    if name == "_" {
        return name.to_string();
    }
    let mangled_name = mangler.fresh(name);
    local_names.insert(name.to_string(), mangled_name.clone());
    mangled_name
}

pub(super) fn mangle_local_expr(expr: &Expr, local_names: &HashMap<String, String>) -> Expr {
    match expr {
        Expr::String(value) => Expr::String(mangle_shell_interpolations(value, local_names)),
        Expr::Command { command, checked } => Expr::Command {
            command: mangle_shell_interpolations(command, local_names),
            checked: *checked,
        },
        Expr::CommandResult { command } => Expr::CommandResult {
            command: mangle_shell_interpolations(command, local_names),
        },
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
                .map(|arg| mangle_local_expr(arg, local_names))
                .collect(),
            result: *result,
            program: program.clone(),
            read_args: read_args.clone(),
            write_args: write_args.clone(),
        },
        Expr::AsyncCommand(command) => {
            Expr::AsyncCommand(mangle_shell_interpolations(command, local_names))
        }
        Expr::Await(name) => Expr::Await(mangle_local_name(name, local_names)),
        Expr::Pipeline { input, commands } => Expr::Pipeline {
            input: input
                .as_ref()
                .map(|input| Box::new(mangle_local_expr(input, local_names))),
            commands: commands
                .iter()
                .map(|command| mangle_shell_interpolations(command, local_names))
                .collect(),
        },
        Expr::TryPipeline { input, commands } => Expr::TryPipeline {
            input: input
                .as_ref()
                .map(|input| Box::new(mangle_local_expr(input, local_names))),
            commands: commands
                .iter()
                .map(|command| mangle_shell_interpolations(command, local_names))
                .collect(),
        },
        Expr::PipelineResult { input, commands } => Expr::PipelineResult {
            input: input
                .as_ref()
                .map(|input| Box::new(mangle_local_expr(input, local_names))),
            commands: commands
                .iter()
                .map(|command| mangle_shell_interpolations(command, local_names))
                .collect(),
        },
        Expr::PathExists(path) => Expr::PathExists(Box::new(mangle_local_expr(path, local_names))),
        Expr::ProcessEnv { name } => Expr::ProcessEnv {
            name: Box::new(mangle_local_expr(name, local_names)),
        },
        Expr::FsIsFile { path } => Expr::FsIsFile {
            path: Box::new(mangle_local_expr(path, local_names)),
        },
        Expr::FsIsDir { path } => Expr::FsIsDir {
            path: Box::new(mangle_local_expr(path, local_names)),
        },
        Expr::FsSize { path } => Expr::FsSize {
            path: Box::new(mangle_local_expr(path, local_names)),
        },
        Expr::FsReadLines { path } => Expr::FsReadLines {
            path: Box::new(mangle_local_expr(path, local_names)),
        },
        Expr::FsList { path } => Expr::FsList {
            path: Box::new(mangle_local_expr(path, local_names)),
        },
        Expr::FsWriteLines { path, lines } => Expr::FsWriteLines {
            path: Box::new(mangle_local_expr(path, local_names)),
            lines: Box::new(mangle_local_expr(lines, local_names)),
        },
        Expr::FsAppendLines { path, lines } => Expr::FsAppendLines {
            path: Box::new(mangle_local_expr(path, local_names)),
            lines: Box::new(mangle_local_expr(lines, local_names)),
        },
        Expr::JsonParse { value } => Expr::JsonParse {
            value: Box::new(mangle_local_expr(value, local_names)),
        },
        Expr::JsonStringify { name } => Expr::JsonStringify {
            name: mangle_local_name(name, local_names),
        },
        Expr::JsonStringifyValue { value } => Expr::JsonStringifyValue {
            value: Box::new(mangle_local_expr(value, local_names)),
        },
        Expr::Range {
            start,
            end,
            inclusive,
        } => Expr::Range {
            start: Box::new(mangle_local_expr(start, local_names)),
            end: Box::new(mangle_local_expr(end, local_names)),
            inclusive: *inclusive,
        },
        Expr::Array(values) => Expr::Array(
            values
                .iter()
                .map(|value| mangle_local_expr(value, local_names))
                .collect(),
        ),
        Expr::Map(entries) => Expr::Map(
            entries
                .iter()
                .map(|(key, value)| {
                    (
                        mangle_local_expr(key, local_names),
                        mangle_local_expr(value, local_names),
                    )
                })
                .collect(),
        ),
        Expr::Record(fields) => Expr::Record(
            fields
                .iter()
                .map(|(name, value)| (name.clone(), mangle_local_expr(value, local_names)))
                .collect(),
        ),
        Expr::RecordPattern(fields) => Expr::RecordPattern(
            fields
                .iter()
                .map(|(name, value)| {
                    (
                        name.clone(),
                        value
                            .as_ref()
                            .map(|value| mangle_local_expr(value, local_names)),
                    )
                })
                .collect(),
        ),
        Expr::Tuple(values) => Expr::Tuple(
            values
                .iter()
                .map(|value| mangle_local_expr(value, local_names))
                .collect(),
        ),
        Expr::Index { name, index } => Expr::Index {
            name: mangle_local_name(name, local_names),
            index: Box::new(mangle_local_expr(index, local_names)),
        },
        Expr::IndexValue { value, index } => Expr::IndexValue {
            value: Box::new(mangle_local_expr(value, local_names)),
            index: Box::new(mangle_local_expr(index, local_names)),
        },
        Expr::Slice { name, start, end } => Expr::Slice {
            name: mangle_local_name(name, local_names),
            start: Box::new(mangle_local_expr(start, local_names)),
            end: Box::new(mangle_local_expr(end, local_names)),
        },
        Expr::ArraySliceValue { value, start, end } => Expr::ArraySliceValue {
            value: Box::new(mangle_local_expr(value, local_names)),
            start: Box::new(mangle_local_expr(start, local_names)),
            end: Box::new(mangle_local_expr(end, local_names)),
        },
        Expr::TupleField { name, field } => Expr::TupleField {
            name: mangle_local_name(name, local_names),
            field: *field,
        },
        Expr::TupleFieldValue { value, field } => Expr::TupleFieldValue {
            value: Box::new(mangle_local_expr(value, local_names)),
            field: *field,
        },
        Expr::FieldValue { value, field } => Expr::FieldValue {
            value: Box::new(mangle_local_expr(value, local_names)),
            field: field.clone(),
        },
        Expr::Field { name, field } => Expr::Field {
            name: mangle_local_name(name, local_names),
            field: field.clone(),
        },
        Expr::NewtypeCtor { name, value } => Expr::NewtypeCtor {
            name: name.clone(),
            value: Box::new(mangle_local_expr(value, local_names)),
        },
        Expr::Variant {
            name,
            args,
            field_types,
        } => Expr::Variant {
            name: name.clone(),
            args: args
                .iter()
                .map(|arg| mangle_local_expr(arg, local_names))
                .collect(),
            field_types: field_types.clone(),
        },
        Expr::Cast { expr, ty } => Expr::Cast {
            expr: Box::new(mangle_local_expr(expr, local_names)),
            ty: ty.clone(),
        },
        Expr::Lambda { params, body } => {
            let mut lambda_names = local_names.clone();
            lambda_names.extend(params.iter().map(|name| (name.clone(), name.clone())));
            Expr::Lambda {
                params: params.clone(),
                body: Box::new(mangle_local_expr(body, &lambda_names)),
            }
        }
        Expr::Closure { name, captures } => Expr::Closure {
            name: name.clone(),
            captures: captures
                .iter()
                .map(|capture| ClosureCapture {
                    source: mangle_local_name(&capture.source, local_names),
                    target: capture.target.clone(),
                    suffixes: capture.suffixes.clone(),
                })
                .collect(),
        },
        Expr::Do { steps, result } => {
            let mut do_names = local_names.clone();
            let steps = steps
                .iter()
                .map(|step| {
                    let step = match step {
                        crate::DoStep::Bind { name, expr } => crate::DoStep::Bind {
                            name: name.clone(),
                            expr: mangle_local_expr(expr, &do_names),
                        },
                        crate::DoStep::Let {
                            name,
                            annotation,
                            expr,
                        } => crate::DoStep::Let {
                            name: name.clone(),
                            annotation: annotation.clone(),
                            expr: mangle_local_expr(expr, &do_names),
                        },
                    };
                    match &step {
                        crate::DoStep::Bind { name, .. } | crate::DoStep::Let { name, .. } => {
                            do_names.insert(name.clone(), name.clone());
                        }
                    }
                    step
                })
                .collect();
            Expr::Do {
                steps,
                result: Box::new(mangle_local_expr(result, &do_names)),
            }
        }
        Expr::LetIn {
            name,
            annotation,
            value,
            body,
        } => {
            let mut body_names = local_names.clone();
            body_names.insert(name.clone(), name.clone());
            Expr::LetIn {
                name: name.clone(),
                annotation: annotation.clone(),
                value: Box::new(mangle_local_expr(value, local_names)),
                body: Box::new(mangle_local_expr(body, &body_names)),
            }
        }
        Expr::Call { name, args } => Expr::Call {
            name: mangle_call_name(name, local_names),
            args: args
                .iter()
                .map(|arg| mangle_local_expr(arg, local_names))
                .collect(),
        },
        Expr::NamedArg { name, value } => Expr::NamedArg {
            name: name.clone(),
            value: Box::new(mangle_local_expr(value, local_names)),
        },
        Expr::Value(name) => Expr::Value(mangle_local_name(name, local_names)),
        Expr::Len(name) => Expr::Len(mangle_local_name(name, local_names)),
        Expr::ArrayLenValue(value) => {
            Expr::ArrayLenValue(Box::new(mangle_local_expr(value, local_names)))
        }
        Expr::MapLenValue(value) => {
            Expr::MapLenValue(Box::new(mangle_local_expr(value, local_names)))
        }
        Expr::IsEmpty(name) => Expr::IsEmpty(mangle_local_name(name, local_names)),
        Expr::ArrayIsEmptyValue(value) => {
            Expr::ArrayIsEmptyValue(Box::new(mangle_local_expr(value, local_names)))
        }
        Expr::MapIsEmptyValue(value) => {
            Expr::MapIsEmptyValue(Box::new(mangle_local_expr(value, local_names)))
        }
        Expr::ArrayFirst(name) => Expr::ArrayFirst(mangle_local_name(name, local_names)),
        Expr::ArrayFirstValue(value) => {
            Expr::ArrayFirstValue(Box::new(mangle_local_expr(value, local_names)))
        }
        Expr::ArrayLast(name) => Expr::ArrayLast(mangle_local_name(name, local_names)),
        Expr::ArrayLastValue(value) => {
            Expr::ArrayLastValue(Box::new(mangle_local_expr(value, local_names)))
        }
        Expr::ArrayReverse(name) => Expr::ArrayReverse(mangle_local_name(name, local_names)),
        Expr::ArrayReverseValue(value) => {
            Expr::ArrayReverseValue(Box::new(mangle_local_expr(value, local_names)))
        }
        Expr::ArraySort(name) => Expr::ArraySort(mangle_local_name(name, local_names)),
        Expr::ArraySortValue(value) => {
            Expr::ArraySortValue(Box::new(mangle_local_expr(value, local_names)))
        }
        Expr::ArrayUnique(name) => Expr::ArrayUnique(mangle_local_name(name, local_names)),
        Expr::ArrayUniqueValue(value) => {
            Expr::ArrayUniqueValue(Box::new(mangle_local_expr(value, local_names)))
        }
        Expr::ArrayMap { name, mapper } => Expr::ArrayMap {
            name: mangle_local_name(name, local_names),
            mapper: Box::new(mangle_local_expr(mapper, local_names)),
        },
        Expr::ArrayMapValue { value, mapper } => Expr::ArrayMapValue {
            value: Box::new(mangle_local_expr(value, local_names)),
            mapper: Box::new(mangle_local_expr(mapper, local_names)),
        },
        Expr::OptionMap { name, mapper } => Expr::OptionMap {
            name: mangle_local_name(name, local_names),
            mapper: Box::new(mangle_local_expr(mapper, local_names)),
        },
        Expr::OptionMapValue { value, mapper } => Expr::OptionMapValue {
            value: Box::new(mangle_local_expr(value, local_names)),
            mapper: Box::new(mangle_local_expr(mapper, local_names)),
        },
        Expr::OptionFlatMap { name, mapper } => Expr::OptionFlatMap {
            name: mangle_local_name(name, local_names),
            mapper: Box::new(mangle_local_expr(mapper, local_names)),
        },
        Expr::OptionFlatMapValue { value, mapper } => Expr::OptionFlatMapValue {
            value: Box::new(mangle_local_expr(value, local_names)),
            mapper: Box::new(mangle_local_expr(mapper, local_names)),
        },
        Expr::ResultMap { name, mapper } => Expr::ResultMap {
            name: mangle_local_name(name, local_names),
            mapper: Box::new(mangle_local_expr(mapper, local_names)),
        },
        Expr::ResultMapValue { value, mapper } => Expr::ResultMapValue {
            value: Box::new(mangle_local_expr(value, local_names)),
            mapper: Box::new(mangle_local_expr(mapper, local_names)),
        },
        Expr::ResultFlatMap { name, mapper } => Expr::ResultFlatMap {
            name: mangle_local_name(name, local_names),
            mapper: Box::new(mangle_local_expr(mapper, local_names)),
        },
        Expr::ResultFlatMapValue { value, mapper } => Expr::ResultFlatMapValue {
            value: Box::new(mangle_local_expr(value, local_names)),
            mapper: Box::new(mangle_local_expr(mapper, local_names)),
        },
        Expr::OptionAp { name, value } => Expr::OptionAp {
            name: mangle_local_name(name, local_names),
            value: Box::new(mangle_local_expr(value, local_names)),
        },
        Expr::OptionApValue { function, value } => Expr::OptionApValue {
            function: Box::new(mangle_local_expr(function, local_names)),
            value: Box::new(mangle_local_expr(value, local_names)),
        },
        Expr::ResultAp { name, value } => Expr::ResultAp {
            name: mangle_local_name(name, local_names),
            value: Box::new(mangle_local_expr(value, local_names)),
        },
        Expr::ResultApValue { function, value } => Expr::ResultApValue {
            function: Box::new(mangle_local_expr(function, local_names)),
            value: Box::new(mangle_local_expr(value, local_names)),
        },
        Expr::OptionOrElse { name, fallback } => Expr::OptionOrElse {
            name: mangle_local_name(name, local_names),
            fallback: Box::new(mangle_local_expr(fallback, local_names)),
        },
        Expr::OptionOrElseValue { value, fallback } => Expr::OptionOrElseValue {
            value: Box::new(mangle_local_expr(value, local_names)),
            fallback: Box::new(mangle_local_expr(fallback, local_names)),
        },
        Expr::OptionOrElseTry { value, fallback } => Expr::OptionOrElseTry {
            value: Box::new(mangle_local_expr(value, local_names)),
            fallback: Box::new(mangle_local_expr(fallback, local_names)),
        },
        Expr::ArrayTake { name, count } => Expr::ArrayTake {
            name: mangle_local_name(name, local_names),
            count: Box::new(mangle_local_expr(count, local_names)),
        },
        Expr::ArrayTakeValue { value, count } => Expr::ArrayTakeValue {
            value: Box::new(mangle_local_expr(value, local_names)),
            count: Box::new(mangle_local_expr(count, local_names)),
        },
        Expr::ArrayDrop { name, count } => Expr::ArrayDrop {
            name: mangle_local_name(name, local_names),
            count: Box::new(mangle_local_expr(count, local_names)),
        },
        Expr::ArrayDropValue { value, count } => Expr::ArrayDropValue {
            value: Box::new(mangle_local_expr(value, local_names)),
            count: Box::new(mangle_local_expr(count, local_names)),
        },
        Expr::Join { name, separator } => Expr::Join {
            name: mangle_local_name(name, local_names),
            separator: Box::new(mangle_local_expr(separator, local_names)),
        },
        Expr::JoinValue { value, separator } => Expr::JoinValue {
            value: Box::new(mangle_local_expr(value, local_names)),
            separator: Box::new(mangle_local_expr(separator, local_names)),
        },
        Expr::ArrayPush { name, value } => Expr::ArrayPush {
            name: mangle_local_name(name, local_names),
            value: Box::new(mangle_local_expr(value, local_names)),
        },
        Expr::ArrayPop { name } => Expr::ArrayPop {
            name: mangle_local_name(name, local_names),
        },
        Expr::MapSet { name, key, value } => Expr::MapSet {
            name: mangle_local_name(name, local_names),
            key: Box::new(mangle_local_expr(key, local_names)),
            value: Box::new(mangle_local_expr(value, local_names)),
        },
        Expr::MapRemove { name, key } => Expr::MapRemove {
            name: mangle_local_name(name, local_names),
            key: Box::new(mangle_local_expr(key, local_names)),
        },
        Expr::ArrayContains { name, value } => Expr::ArrayContains {
            name: mangle_local_name(name, local_names),
            value: Box::new(mangle_local_expr(value, local_names)),
        },
        Expr::ArrayContainsValue { value, item } => Expr::ArrayContainsValue {
            value: Box::new(mangle_local_expr(value, local_names)),
            item: Box::new(mangle_local_expr(item, local_names)),
        },
        Expr::ArrayIndexOf { name, value } => Expr::ArrayIndexOf {
            name: mangle_local_name(name, local_names),
            value: Box::new(mangle_local_expr(value, local_names)),
        },
        Expr::ArrayIndexOfValue { value, item } => Expr::ArrayIndexOfValue {
            value: Box::new(mangle_local_expr(value, local_names)),
            item: Box::new(mangle_local_expr(item, local_names)),
        },
        Expr::MapKeys(name) => Expr::MapKeys(mangle_local_name(name, local_names)),
        Expr::MapKeysValue(value) => {
            Expr::MapKeysValue(Box::new(mangle_local_expr(value, local_names)))
        }
        Expr::MapValues(name) => Expr::MapValues(mangle_local_name(name, local_names)),
        Expr::MapValuesValue(value) => {
            Expr::MapValuesValue(Box::new(mangle_local_expr(value, local_names)))
        }
        Expr::MapHas { name, key } => Expr::MapHas {
            name: mangle_local_name(name, local_names),
            key: Box::new(mangle_local_expr(key, local_names)),
        },
        Expr::MapHasValue { value, key } => Expr::MapHasValue {
            value: Box::new(mangle_local_expr(value, local_names)),
            key: Box::new(mangle_local_expr(key, local_names)),
        },
        Expr::StringContains { name, needle } => Expr::StringContains {
            name: mangle_local_name(name, local_names),
            needle: Box::new(mangle_local_expr(needle, local_names)),
        },
        Expr::StringContainsValue { value, needle } => Expr::StringContainsValue {
            value: Box::new(mangle_local_expr(value, local_names)),
            needle: Box::new(mangle_local_expr(needle, local_names)),
        },
        Expr::StringIndexOf { name, needle } => Expr::StringIndexOf {
            name: mangle_local_name(name, local_names),
            needle: Box::new(mangle_local_expr(needle, local_names)),
        },
        Expr::StringIndexOfValue { value, needle } => Expr::StringIndexOfValue {
            value: Box::new(mangle_local_expr(value, local_names)),
            needle: Box::new(mangle_local_expr(needle, local_names)),
        },
        Expr::StringStartsWith { name, prefix } => Expr::StringStartsWith {
            name: mangle_local_name(name, local_names),
            prefix: Box::new(mangle_local_expr(prefix, local_names)),
        },
        Expr::StringStartsWithValue { value, prefix } => Expr::StringStartsWithValue {
            value: Box::new(mangle_local_expr(value, local_names)),
            prefix: Box::new(mangle_local_expr(prefix, local_names)),
        },
        Expr::StringEndsWith { name, suffix } => Expr::StringEndsWith {
            name: mangle_local_name(name, local_names),
            suffix: Box::new(mangle_local_expr(suffix, local_names)),
        },
        Expr::StringEndsWithValue { value, suffix } => Expr::StringEndsWithValue {
            value: Box::new(mangle_local_expr(value, local_names)),
            suffix: Box::new(mangle_local_expr(suffix, local_names)),
        },
        Expr::StringLen(name) => Expr::StringLen(mangle_local_name(name, local_names)),
        Expr::StringLenValue(value) => {
            Expr::StringLenValue(Box::new(mangle_local_expr(value, local_names)))
        }
        Expr::StringIsEmpty(name) => Expr::StringIsEmpty(mangle_local_name(name, local_names)),
        Expr::StringIsEmptyValue(value) => {
            Expr::StringIsEmptyValue(Box::new(mangle_local_expr(value, local_names)))
        }
        Expr::StringSlice { name, start, end } => Expr::StringSlice {
            name: mangle_local_name(name, local_names),
            start: Box::new(mangle_local_expr(start, local_names)),
            end: Box::new(mangle_local_expr(end, local_names)),
        },
        Expr::StringSliceValue { value, start, end } => Expr::StringSliceValue {
            value: Box::new(mangle_local_expr(value, local_names)),
            start: Box::new(mangle_local_expr(start, local_names)),
            end: Box::new(mangle_local_expr(end, local_names)),
        },
        Expr::StringTrim(name) => Expr::StringTrim(mangle_local_name(name, local_names)),
        Expr::StringTrimValue(value) => {
            Expr::StringTrimValue(Box::new(mangle_local_expr(value, local_names)))
        }
        Expr::StringTrimStart(name) => Expr::StringTrimStart(mangle_local_name(name, local_names)),
        Expr::StringTrimStartValue(value) => {
            Expr::StringTrimStartValue(Box::new(mangle_local_expr(value, local_names)))
        }
        Expr::StringTrimEnd(name) => Expr::StringTrimEnd(mangle_local_name(name, local_names)),
        Expr::StringTrimEndValue(value) => {
            Expr::StringTrimEndValue(Box::new(mangle_local_expr(value, local_names)))
        }
        Expr::StringToUpper(name) => Expr::StringToUpper(mangle_local_name(name, local_names)),
        Expr::StringToUpperValue(value) => {
            Expr::StringToUpperValue(Box::new(mangle_local_expr(value, local_names)))
        }
        Expr::StringToLower(name) => Expr::StringToLower(mangle_local_name(name, local_names)),
        Expr::StringToLowerValue(value) => {
            Expr::StringToLowerValue(Box::new(mangle_local_expr(value, local_names)))
        }
        Expr::StringRepeat { name, count } => Expr::StringRepeat {
            name: mangle_local_name(name, local_names),
            count: Box::new(mangle_local_expr(count, local_names)),
        },
        Expr::StringRepeatValue { value, count } => Expr::StringRepeatValue {
            value: Box::new(mangle_local_expr(value, local_names)),
            count: Box::new(mangle_local_expr(count, local_names)),
        },
        Expr::StringSplit { name, separator } => Expr::StringSplit {
            name: mangle_local_name(name, local_names),
            separator: Box::new(mangle_local_expr(separator, local_names)),
        },
        Expr::StringSplitValue { value, separator } => Expr::StringSplitValue {
            value: Box::new(mangle_local_expr(value, local_names)),
            separator: Box::new(mangle_local_expr(separator, local_names)),
        },
        Expr::StringReplace { name, from, to } => Expr::StringReplace {
            name: mangle_local_name(name, local_names),
            from: Box::new(mangle_local_expr(from, local_names)),
            to: Box::new(mangle_local_expr(to, local_names)),
        },
        Expr::StringReplaceValue { value, from, to } => Expr::StringReplaceValue {
            value: Box::new(mangle_local_expr(value, local_names)),
            from: Box::new(mangle_local_expr(from, local_names)),
            to: Box::new(mangle_local_expr(to, local_names)),
        },
        Expr::PathBasename(name) => Expr::PathBasename(mangle_local_name(name, local_names)),
        Expr::PathBasenameValue(value) => {
            Expr::PathBasenameValue(Box::new(mangle_local_expr(value, local_names)))
        }
        Expr::PathDirname(name) => Expr::PathDirname(mangle_local_name(name, local_names)),
        Expr::PathDirnameValue(value) => {
            Expr::PathDirnameValue(Box::new(mangle_local_expr(value, local_names)))
        }
        Expr::PathStem(name) => Expr::PathStem(mangle_local_name(name, local_names)),
        Expr::PathStemValue(value) => {
            Expr::PathStemValue(Box::new(mangle_local_expr(value, local_names)))
        }
        Expr::PathExtname(name) => Expr::PathExtname(mangle_local_name(name, local_names)),
        Expr::PathExtnameValue(value) => {
            Expr::PathExtnameValue(Box::new(mangle_local_expr(value, local_names)))
        }
        Expr::PathIsAbsolute(name) => Expr::PathIsAbsolute(mangle_local_name(name, local_names)),
        Expr::PathIsAbsoluteValue(value) => {
            Expr::PathIsAbsoluteValue(Box::new(mangle_local_expr(value, local_names)))
        }
        Expr::IfElse {
            condition,
            then_expr,
            else_expr,
        } => Expr::IfElse {
            condition: Box::new(mangle_local_expr(condition, local_names)),
            then_expr: Box::new(mangle_local_expr(then_expr, local_names)),
            else_expr: Box::new(mangle_local_expr(else_expr, local_names)),
        },
        Expr::Match { value, arms } => Expr::Match {
            value: Box::new(mangle_local_expr(value, local_names)),
            arms: arms
                .iter()
                .map(|arm| MatchArm {
                    pattern: arm
                        .pattern
                        .as_ref()
                        .map(|pattern| mangle_local_expr(pattern, local_names)),
                    guard: arm
                        .guard
                        .as_ref()
                        .map(|guard| mangle_local_expr(guard, local_names)),
                    expr: mangle_local_expr(&arm.expr, local_names),
                })
                .collect(),
        },
        Expr::MatchGuardResult(value) => {
            Expr::MatchGuardResult(Box::new(mangle_local_expr(value, local_names)))
        }
        Expr::Some(value) => Expr::Some(Box::new(mangle_local_expr(value, local_names))),
        Expr::Ok(value) => Expr::Ok(Box::new(mangle_local_expr(value, local_names))),
        Expr::Err(value) => Expr::Err(Box::new(mangle_local_expr(value, local_names))),
        Expr::ResultOption(value) => {
            Expr::ResultOption(Box::new(mangle_local_expr(value, local_names)))
        }
        Expr::TryResult(value) => Expr::TryResult(Box::new(mangle_local_expr(value, local_names))),
        Expr::Default { value, fallback } => Expr::Default {
            value: Box::new(mangle_local_expr(value, local_names)),
            fallback: Box::new(mangle_local_expr(fallback, local_names)),
        },
        Expr::DefaultTry { value, fallback } => Expr::DefaultTry {
            value: Box::new(mangle_local_expr(value, local_names)),
            fallback: Box::new(mangle_local_expr(fallback, local_names)),
        },
        Expr::Not(expr) => Expr::Not(Box::new(mangle_local_expr(expr, local_names))),
        Expr::BitNot(expr) => Expr::BitNot(Box::new(mangle_local_expr(expr, local_names))),
        Expr::Ident(name) => Expr::Ident(mangle_local_name(name, local_names)),
        Expr::Binary { left, op, right } => Expr::Binary {
            left: Box::new(mangle_local_expr(left, local_names)),
            op: *op,
            right: Box::new(mangle_local_expr(right, local_names)),
        },
        other => other.clone(),
    }
}

pub(super) fn mangle_local_name(name: &str, local_names: &HashMap<String, String>) -> String {
    local_names
        .get(name)
        .cloned()
        .unwrap_or_else(|| name.to_string())
}

pub(super) fn mangle_call_name(name: &str, local_names: &HashMap<String, String>) -> String {
    if let Some(mapped) = local_names.get(name) {
        return mapped.clone();
    }
    let Some((receiver, method)) = name.rsplit_once('.') else {
        return name.to_string();
    };
    if let Some(mapped) = local_names.get(receiver) {
        format!("{mapped}.{method}")
    } else {
        name.to_string()
    }
}

pub(super) fn mangle_shell_interpolations(
    value: &str,
    local_names: &HashMap<String, String>,
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
        out.push_str(&mangle_interpolation_ref(value, local_names));
        out.push('}');
        rest = &after_start[end + 1..];
    }
    out.push_str(rest);
    out
}

fn mangle_interpolation_ref(value: &str, local_names: &HashMap<String, String>) -> String {
    if let Some((base, index)) = value
        .strip_suffix(']')
        .and_then(|value| value.split_once('['))
    {
        return format!("{}[{index}]", mangle_local_name(base, local_names));
    }
    if let Some((base, field)) = value.split_once('.') {
        let separator = if field.starts_with('_') { "" } else { "_" };
        return format!("{}{separator}{field}", mangle_local_name(base, local_names));
    }
    mangle_local_name(value, local_names)
}

pub(super) fn sanitize_shell_ident(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch == '_' || ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push('_');
    }
    out
}
