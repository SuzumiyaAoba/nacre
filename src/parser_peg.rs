use crate::{
    AssignTarget, BinaryOp, BindingPattern, CompileError, DoStep, Expr, ForBinding, ImplConst,
    ImplMethod, MatchArm, Param, Program, Statement, TraitMethod, Type, TypeParam, UseItem,
    VariantDecl,
};

#[derive(Debug)]
struct ParsedStatement {
    offset: usize,
    statement: Statement,
}

#[derive(Debug)]
enum BindingTarget {
    Name(String, Option<Type>),
    Pattern(BindingPattern),
}

#[derive(Debug)]
struct FunctionHead {
    name: String,
    override_constructor: bool,
    type_params: Vec<TypeParam>,
    params: Vec<Param>,
    return_type: Type,
}

#[derive(Debug)]
enum TypeDeclaration {
    Alias(Type),
    Sum(Vec<VariantDecl>),
}

#[derive(Debug)]
enum ImplItem {
    Const(ImplConst),
    Method(ImplMethod),
}

#[derive(Debug)]
enum Postfix {
    Call(Vec<Expr>),
    Method(String, Vec<Expr>),
    Index(Expr),
    Field(String),
    TupleField(usize),
    ResultOption,
    TryResult,
}

#[derive(Debug)]
enum CollectionEntry {
    Record(String, Expr),
    Map(Expr, Expr),
}

#[derive(Debug)]
enum DoItem {
    Bind(String, Expr),
    Let(String, Option<Type>, Expr),
    Expr(Expr),
}

pub(crate) fn parse(source: &str) -> Result<Program, CompileError> {
    let parsed =
        nacre_grammar::program_root(source).map_err(|error| program_parse_error(source, error))?;
    let mut statements = Vec::with_capacity(parsed.len());
    let mut lines = Vec::with_capacity(parsed.len());
    for parsed in parsed {
        statements.push(parsed.statement);
        lines.push(
            source[..parsed.offset]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
                + 1,
        );
    }
    let program = Program::new(statements, lines);
    validate_program(&program)?;
    Ok(program)
}

fn program_parse_error(
    source: &str,
    error: peg::error::ParseError<peg::str::LineCol>,
) -> CompileError {
    let trimmed = source.trim();
    if source.trim_start().starts_with("raw {") && !source.trim_end().ends_with('}') {
        return CompileError::new(error.location.line, "unterminated raw block".to_string());
    }
    if let Some((_, default)) = source.split_once("??") {
        if !default.trim_start().starts_with('"') {
            return CompileError::new(error.location.line, "expected quoted string".to_string());
        }
    }
    if trimmed.ends_with("$sh") {
        return CompileError::new(error.location.line, "expected quoted string".to_string());
    }
    if source.contains("$sh ") {
        return CompileError::new(error.location.line, "expected quoted string".to_string());
    }
    if let Some(arguments) = trimmed.strip_prefix("require(") {
        if !arguments.trim_start().starts_with('"') {
            return CompileError::new(error.location.line, "expected quoted string".to_string());
        }
        if let Some((_, optional)) = arguments.split_once(',') {
            let optional = optional.trim_start();
            if optional.starts_with('"') {
                return CompileError::new(
                    error.location.line,
                    "version must use `version = \"...\"`".to_string(),
                );
            }
            if !optional.starts_with("version") {
                return CompileError::new(
                    error.location.line,
                    "optional argument must be `version`".to_string(),
                );
            }
        }
    }
    if trimmed == "requireOneOf([])" {
        return CompileError::new(
            error.location.line,
            "requireOneOf expects at least one command".to_string(),
        );
    }
    if trimmed.starts_with("requireOneOf([") {
        return CompileError::new(
            error.location.line,
            "requireOneOf expects an array of quoted strings".to_string(),
        );
    }
    if let Some((_, shell)) = source.split_once("$sh{") {
        return CompileError::new(
            error.location.line,
            if shell.contains('}') {
                "unexpected text after shell command"
            } else {
                "unterminated shell command"
            }
            .to_string(),
        );
    }
    if let Some((_, shell)) = source.split_once("$sh\"") {
        if !shell.contains('"') {
            return CompileError::new(
                error.location.line,
                "unterminated quoted string in shell command".to_string(),
            );
        }
    }
    if source.matches('"').count() % 2 == 1 {
        return CompileError::new(
            error.location.line,
            "unterminated quoted string".to_string(),
        );
    }
    if trimmed.starts_with("export fn") && trimmed.contains('{') {
        return CompileError::new(
            error.location.line,
            "external function declarations must not include bodies".to_string(),
        );
    }
    if trimmed.starts_with("for  ") {
        return CompileError::new(error.location.line, "expected assignment".to_string());
    }
    if let Some(signature) = trimmed.strip_prefix("fn ") {
        let first_line = signature.lines().next().unwrap_or_default();
        if first_line
            .find(':')
            .is_some_and(|colon| first_line.find('(').is_none_or(|paren| colon < paren))
        {
            return CompileError::new(
                error.location.line,
                "expected function parameters".to_string(),
            );
        }
        if first_line.contains("): {") {
            return CompileError::new(error.location.line, "expected type name".to_string());
        }
        if first_line.contains("[]") {
            return CompileError::new(error.location.line, "expected array element".to_string());
        }
        if first_line.contains('[') && !first_line.contains(']') {
            return CompileError::new(
                error.location.line,
                "unterminated function type parameters".to_string(),
            );
        }
    }
    if let Some(declaration) = trimmed.strip_prefix("trait ") {
        let declaration = declaration.split("\nimpl ").next().unwrap_or(declaration);
        let first_line = declaration.lines().next().unwrap_or_default();
        if !first_line.contains('[') {
            return CompileError::new(error.location.line, "expected type parameters".to_string());
        }
        if first_line
            .split_once('[')
            .and_then(|(_, rest)| rest.split_once(']'))
            .is_some_and(|(params, _)| params.contains(','))
        {
            return CompileError::new(
                error.location.line,
                "trait requires exactly one type parameter".to_string(),
            );
        }
        if declaration
            .lines()
            .skip(1)
            .any(|line| line.trim_start().starts_with("fn ") && line.contains('['))
        {
            return CompileError::new(
                error.location.line,
                "trait methods cannot declare type parameters".to_string(),
            );
        }
        if declaration
            .lines()
            .skip(1)
            .any(|line| line.trim_start().starts_with("fn ") && line.contains(" = "))
        {
            return CompileError::new(
                error.location.line,
                "trait methods cannot declare default parameters".to_string(),
            );
        }
        if declaration
            .lines()
            .skip(1)
            .any(|line| line.trim_start().starts_with("fn ") && line.trim_end().ends_with('{'))
        {
            return CompileError::new(
                error.location.line,
                "trait method signatures must not include bodies".to_string(),
            );
        }
        if declaration.lines().skip(1).any(|line| {
            let line = line.trim();
            !line.is_empty() && line != "}" && !line.starts_with("fn ")
        }) {
            return CompileError::new(
                error.location.line,
                "trait bodies support method signatures only".to_string(),
            );
        }
    }
    for declaration in source
        .lines()
        .filter_map(|line| line.trim().strip_prefix("impl "))
    {
        if !declaration.contains('[') {
            return CompileError::new(
                error.location.line,
                "expected trait implementation".to_string(),
            );
        }
        if declaration
            .split_once('[')
            .and_then(|(_, rest)| rest.split_once(']'))
            .is_some_and(|(types, _)| types.contains(','))
        {
            return CompileError::new(
                error.location.line,
                "trait implementation requires exactly one type".to_string(),
            );
        }
    }
    if source.lines().any(|line| {
        line.trim_start().starts_with("fn ")
            && line.contains('[')
            && source
                .lines()
                .any(|candidate| candidate.trim_start().starts_with("impl "))
    }) {
        return CompileError::new(
            error.location.line,
            "impl methods cannot declare type parameters".to_string(),
        );
    }
    if source
        .lines()
        .any(|line| line.trim_start().starts_with("impl "))
        && source
            .lines()
            .any(|line| line.trim_start().starts_with("const "))
    {
        return CompileError::new(
            error.location.line,
            "impl bodies support method definitions only".to_string(),
        );
    }
    if trimmed == "not an assignment" {
        return CompileError::new(error.location.line, "expected assignment".to_string());
    }
    if let Some((target, _)) = trimmed.split_once(" = ") {
        if target.contains('\\') {
            return CompileError::new(
                error.location.line,
                format!("invalid variable name `{target}`"),
            );
        }
    }
    if trimmed.starts_with("const [") && trimmed.contains("...") {
        if let Some((_, rest)) = trimmed.split_once("...") {
            if rest.split('=').next().unwrap_or_default().contains(',') {
                return CompileError::new(
                    error.location.line,
                    "array rest destructuring must be last".to_string(),
                );
            }
        }
    }
    if trimmed.ends_with("= {") {
        return CompileError::new(error.location.line, "unterminated map literal".to_string());
    }
    if let Some((_, annotation)) = trimmed.split_once(": (") {
        if let Some((tuple, _)) = annotation.split_once(')') {
            if !tuple.contains(',') {
                return CompileError::new(
                    error.location.line,
                    "tuple type requires at least two elements".to_string(),
                );
            }
        }
    }
    if let Some((_, tuple)) = trimmed.split_once("= (") {
        if tuple.contains(',') && !tuple.contains(')') {
            return CompileError::new(
                error.location.line,
                "unterminated tuple literal".to_string(),
            );
        }
    }
    if trimmed.ends_with('[') || (trimmed.contains("= [") && trimmed.ends_with(',')) {
        return CompileError::new(
            error.location.line,
            "unterminated array literal".to_string(),
        );
    }
    if trimmed.ends_with(",]") {
        return CompileError::new(error.location.line, "expected array element".to_string());
    }
    if source.split_whitespace().any(|token| {
        token.strip_prefix("0x").is_some_and(|digits| {
            digits.is_empty() || !digits.chars().all(|ch| ch.is_ascii_hexdigit())
        }) || token.strip_prefix("0b").is_some_and(|digits| {
            digits.is_empty() || !digits.chars().all(|ch| matches!(ch, '0' | '1'))
        })
    }) {
        return CompileError::new(error.location.line, "invalid integer literal".to_string());
    }
    if source.matches('{').count() > source.matches('}').count() {
        return CompileError::new(error.location.line, "unterminated block".to_string());
    }
    if trimmed.starts_with("const x = {") && trimmed.ends_with('}') {
        let contents = trimmed
            .strip_prefix("const x = {")
            .and_then(|value| value.strip_suffix('}'))
            .unwrap_or_default()
            .trim();
        if !contents.contains(':') {
            return CompileError::new(error.location.line, "expected `:` in map entry".to_string());
        }
        if contents.ends_with(':') {
            return CompileError::new(
                error.location.line,
                "expected map key and value".to_string(),
            );
        }
    }
    let line = source
        .lines()
        .nth(error.location.line.saturating_sub(1))
        .unwrap_or_default()
        .trim();
    for keyword in ["const", "let"] {
        if let Some(rest) = line.strip_prefix(keyword) {
            let target = rest.split('=').next().unwrap_or_default().trim();
            if target.is_empty() {
                return CompileError::new(
                    error.location.line,
                    "expected variable name".to_string(),
                );
            }
            if target.contains('-') {
                return CompileError::new(
                    error.location.line,
                    format!("invalid variable name `{target}`"),
                );
            }
        }
    }
    CompileError::with_span(
        error.location.line,
        error.location.column,
        error.location.line,
        error.location.column + 1,
        format!(
            "invalid syntax at column {}: expected {}",
            error.location.column, error.expected
        ),
    )
}

fn validate_program(program: &Program) -> Result<(), CompileError> {
    for (statement, line) in program.statements().iter().zip(program.statement_lines()) {
        match statement {
            Statement::Function {
                params,
                return_type,
                body,
                ..
            } => {
                for param in params {
                    validate_type(&param.ty, *line)?;
                }
                validate_type(return_type, *line)?;
                validate_program(body)?;
            }
            Statement::Block { body }
            | Statement::While { body, .. }
            | Statement::For { body, .. } => validate_program(body)?,
            Statement::Trait { methods, .. } => {
                for method in methods {
                    for param in &method.params {
                        validate_type(&param.ty, *line)?;
                    }
                    validate_type(&method.return_type, *line)?;
                }
            }
            Statement::Impl {
                for_type, methods, ..
            } => {
                validate_type(for_type, *line)?;
                for method in methods {
                    for param in &method.params {
                        validate_type(&param.ty, *line)?;
                    }
                    validate_type(&method.return_type, *line)?;
                    validate_program(&method.body)?;
                }
            }
            Statement::InherentImpl {
                for_type,
                consts,
                methods,
            } => {
                validate_type(for_type, *line)?;
                for value in consts {
                    if let Some(annotation) = &value.annotation {
                        validate_type(annotation, *line)?;
                    }
                    validate_expr(&value.expr, *line)?;
                }
                for method in methods {
                    for param in &method.params {
                        validate_type(&param.ty, *line)?;
                    }
                    validate_type(&method.return_type, *line)?;
                    validate_program(&method.body)?;
                }
            }
            Statement::TypeAlias { ty, .. } => validate_type(ty, *line)?,
            Statement::SumType { variants, .. } => {
                for variant in variants {
                    for field in &variant.fields {
                        validate_type(field, *line)?;
                    }
                }
            }
            Statement::Newtype { base, .. } => validate_type(base, *line)?,
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                validate_expr(condition, *line)?;
                validate_program(then_branch)?;
                if let Some(else_branch) = else_branch {
                    validate_program(else_branch)?;
                }
            }
            Statement::Const {
                annotation, expr, ..
            }
            | Statement::Let {
                annotation, expr, ..
            } => {
                if let Some(annotation) = annotation {
                    validate_type(annotation, *line)?;
                }
                validate_expr(expr, *line)?;
            }
            Statement::Destructure { expr, .. }
            | Statement::Assign { expr, .. }
            | Statement::Expr(expr)
            | Statement::TryResult(expr)
            | Statement::Return(expr) => validate_expr(expr, *line)?,
            Statement::TryCommand(_)
            | Statement::TryCommandResult(_)
            | Statement::TryPipeline { .. }
            | Statement::TryPipelineResult { .. }
            | Statement::Command(_)
            | Statement::Redirect { .. } => {
                return Err(CompileError::new(
                    *line,
                    "$sh commands and shell pipelines are disabled; use a policy-approved `run.<group>.<command>(...)` call"
                        .to_string(),
                ));
            }
            Statement::Raw(_) => {
                return Err(CompileError::new(
                    *line,
                    "raw Bash blocks are disabled in the safe language profile".to_string(),
                ));
            }
            Statement::Require { .. } | Statement::RequireOneOf { .. } => {
                return Err(CompileError::new(
                    *line,
                    "`require` is disabled; declare every executable in the external policy and call it through `run.<group>.<command>(...)`".to_string(),
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

fn validate_type(ty: &Type, line: usize) -> Result<(), CompileError> {
    match ty {
        Type::Applied(name, args) if name == "Map" && args.len() != 2 => Err(CompileError::new(
            line,
            "Map type requires key and value types".to_string(),
        )),
        Type::Future(value) | Type::Array(value) | Type::Brand { base: value, .. } => {
            validate_type(value, line)
        }
        Type::Map(key, value) => {
            validate_type(key, line)?;
            validate_type(value, line)
        }
        Type::Record(fields) => {
            for (_, field) in fields {
                validate_type(field, line)?;
            }
            Ok(())
        }
        Type::Tuple(types)
        | Type::Union(types)
        | Type::Intersection(types)
        | Type::Applied(_, types) => {
            for ty in types {
                validate_type(ty, line)?;
            }
            Ok(())
        }
        Type::Function(params, result) => {
            for param in params {
                validate_type(param, line)?;
            }
            validate_type(result, line)
        }
        _ => Ok(()),
    }
}

fn validate_expr(expr: &Expr, line: usize) -> Result<(), CompileError> {
    match expr {
        Expr::Default { value, fallback } => {
            if matches!(value.as_ref(), Expr::Env(_)) {
                return Err(CompileError::new(
                    line,
                    "expected quoted string".to_string(),
                ));
            }
            validate_expr(value, line)?;
            validate_expr(fallback, line)
        }
        Expr::Binary {
            left,
            op: BinaryOp::Shr,
            ..
        } if matches!(left.as_ref(), Expr::Command { .. } | Expr::Pipeline { .. }) => {
            Err(CompileError::new(
                line,
                "redirect target must be `write(...)` or `append(...)`".to_string(),
            ))
        }
        Expr::Binary { left, right, .. } => {
            validate_expr(left, line)?;
            validate_expr(right, line)
        }
        Expr::Array(values) | Expr::Tuple(values) => {
            for value in values {
                validate_expr(value, line)?;
            }
            Ok(())
        }
        Expr::Map(values) => {
            for (key, value) in values {
                validate_expr(key, line)?;
                validate_expr(value, line)?;
            }
            Ok(())
        }
        Expr::Record(values) => {
            for (_, value) in values {
                validate_expr(value, line)?;
            }
            Ok(())
        }
        Expr::Call { args, .. }
        | Expr::Variant { args, .. }
        | Expr::AllowedCommand { args, .. } => {
            for arg in args {
                validate_expr(arg, line)?;
            }
            Ok(())
        }
        Expr::Command { .. }
        | Expr::CommandResult { .. }
        | Expr::AsyncCommand(_)
        | Expr::Pipeline { .. }
        | Expr::TryPipeline { .. }
        | Expr::PipelineResult { .. } => Err(CompileError::new(
            line,
            "$sh commands and shell pipelines are disabled; use a policy-approved `run.<group>.<command>(...)` call"
                .to_string(),
        )),
        Expr::Lambda { params, body } => {
            for (index, param) in params.iter().enumerate() {
                if params[..index].contains(param) {
                    return Err(CompileError::new(
                        line,
                        format!("lambda parameter `{param}` is already defined"),
                    ));
                }
            }
            validate_expr(body, line)
        }
        Expr::IfElse {
            condition,
            then_expr,
            else_expr,
        } => {
            if matches!(condition.as_ref(), Expr::IfElse { .. }) {
                return Err(CompileError::new(
                    line,
                    "unexpected text after if expression".to_string(),
                ));
            }
            validate_expr(condition, line)?;
            validate_expr(then_expr, line)?;
            validate_expr(else_expr, line)
        }
        Expr::Match { value, arms } => {
            validate_expr(value, line)?;
            for arm in arms {
                if let Some(pattern) = &arm.pattern {
                    validate_expr(pattern, line)?;
                }
                if let Some(guard) = &arm.guard {
                    validate_expr(guard, line)?;
                }
                validate_expr(&arm.expr, line)?;
            }
            Ok(())
        }
        Expr::Do { steps, result } => {
            for step in steps {
                match step {
                    DoStep::Bind { expr, .. } | DoStep::Let { expr, .. } => {
                        validate_expr(expr, line)?;
                    }
                }
            }
            validate_expr(result, line)
        }
        _ => Ok(()),
    }
}

pub(crate) fn parse_expr(input: &str, line: usize) -> Result<Expr, CompileError> {
    let expr = nacre_grammar::expression_root(input).map_err(|error| {
        CompileError::new(
            line + error.location.line.saturating_sub(1),
            format!(
                "invalid expression syntax at column {}: expected {}",
                error.location.column, error.expected
            ),
        )
    })?;
    if matches!(
        &expr,
        Expr::Default {
            value,
            ..
        } if matches!(value.as_ref(), Expr::Env(_))
    ) {
        return Err(CompileError::new(
            line,
            "expected quoted string".to_string(),
        ));
    }
    Ok(expr)
}

#[cfg(test)]
pub(crate) fn parse_type(input: &str, line: usize) -> Result<Type, CompileError> {
    let ty = nacre_grammar::type_root(input).map_err(|error| {
        CompileError::new(
            line + error.location.line.saturating_sub(1),
            format!("invalid type syntax at column {}", error.location.column),
        )
    })?;
    if matches!(&ty, Type::Applied(name, args) if name == "Map" && args.len() != 2) {
        return Err(CompileError::new(
            line,
            "Map type requires key and value types".to_string(),
        ));
    }
    Ok(ty)
}

fn named_type(name: String) -> Type {
    match name.as_str() {
        "Int" => Type::Int,
        "Float" => Type::Float,
        "Bool" => Type::Bool,
        "String" => Type::String,
        "Path" => Type::Path,
        "ExitCode" => Type::ExitCode,
        "Unit" => Type::Unit,
        _ => Type::Named(name),
    }
}

fn applied_type(name: String, args: Vec<Type>) -> Type {
    match (name.as_str(), args.as_slice()) {
        ("Future", [value]) => Type::Future(Box::new(value.clone())),
        ("Map", [key, value]) => Type::Map(Box::new(key.clone()), Box::new(value.clone())),
        _ => Type::Applied(name, args),
    }
}

fn flatten_union(left: Type, right: Type) -> Type {
    let mut types = match left {
        Type::Union(types) => types,
        other => vec![other],
    };
    match right {
        Type::Union(right) => types.extend(right),
        other => types.push(other),
    }
    Type::Union(types)
}

fn flatten_intersection(left: Type, right: Type) -> Type {
    let mut types = match left {
        Type::Intersection(types) => types,
        other => vec![other],
    };
    match right {
        Type::Intersection(right) => types.extend(right),
        other => types.push(other),
    }
    Type::Intersection(types)
}

fn decode_escaped(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            result.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => result.push('\n'),
            Some('r') => result.push('\r'),
            Some('t') => result.push('\t'),
            Some(other) => result.push(other),
            None => result.push('\\'),
        }
    }
    result
}

fn string_expr(value: String) -> Result<Expr, String> {
    if !value.contains("${") {
        return Ok(Expr::String(value));
    }

    let mut parts = Vec::new();
    let mut rest = value.as_str();
    while let Some(start) = rest.find("${") {
        if start > 0 {
            parts.push(Expr::String(rest[..start].to_string()));
        }
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find('}') else {
            return Ok(Expr::String(value));
        };
        let source = &after_start[..end];
        let expr =
            parse_expr(source, 1).map_err(|_| "string interpolation expression".to_string())?;
        parts.push(Expr::Cast {
            expr: Box::new(expr),
            ty: Type::String,
        });
        rest = &after_start[end + 1..];
    }
    if !rest.is_empty() {
        parts.push(Expr::String(rest.to_string()));
    }

    let mut parts = parts.into_iter();
    let Some(first) = parts.next() else {
        return Ok(Expr::String(String::new()));
    };
    Ok(parts.fold(first, |left, right| binary(left, BinaryOp::Concat, right)))
}

fn binary(left: Expr, op: BinaryOp, right: Expr) -> Expr {
    Expr::Binary {
        left: Box::new(left),
        op,
        right: Box::new(right),
    }
}

fn compound_assignment(name: String, op: BinaryOp, right: Expr) -> Statement {
    Statement::Assign {
        target: AssignTarget::Name(name.clone()),
        expr: binary(Expr::Ident(name), op, right),
    }
}

fn default_expr(left: Expr, right: Expr) -> Expr {
    Expr::Default {
        value: Box::new(left),
        fallback: Box::new(right),
    }
}

fn ident_name(value: &Expr) -> Option<&str> {
    if let Expr::Ident(name) = value {
        Some(name)
    } else {
        None
    }
}

fn call_expr(name: String, mut args: Vec<Expr>) -> Expr {
    if let Some((group, command, result)) = allowed_command_name(&name) {
        return Expr::AllowedCommand {
            group,
            command,
            args,
            result,
            program: None,
            read_args: Vec::new(),
            write_args: Vec::new(),
        };
    }
    match (name.as_str(), args.len()) {
        ("Some", 1) => Expr::Some(Box::new(args.remove(0))),
        ("Ok", 1) => Expr::Ok(Box::new(args.remove(0))),
        ("Err", 1) => Expr::Err(Box::new(args.remove(0))),
        ("hasCommand", 1) => match args.remove(0) {
            Expr::String(command) => Expr::HasCommand(command),
            value => Expr::Call {
                name,
                args: vec![value],
            },
        },
        ("pathExists", 1) => Expr::PathExists(Box::new(args.remove(0))),
        ("process.args", 0) => Expr::ProcessArgs,
        ("process.env", 1) => Expr::ProcessEnv {
            name: Box::new(args.remove(0)),
        },
        ("fs.isFile", 1) => Expr::FsIsFile {
            path: Box::new(args.remove(0)),
        },
        ("fs.isDir", 1) => Expr::FsIsDir {
            path: Box::new(args.remove(0)),
        },
        ("fs.size", 1) => Expr::FsSize {
            path: Box::new(args.remove(0)),
        },
        ("fs.readLines", 1) => Expr::FsReadLines {
            path: Box::new(args.remove(0)),
        },
        ("fs.list", 1) => Expr::FsList {
            path: Box::new(args.remove(0)),
        },
        ("fs.writeLines", 2) => Expr::FsWriteLines {
            path: Box::new(args.remove(0)),
            lines: Box::new(args.remove(0)),
        },
        ("fs.appendLines", 2) => Expr::FsAppendLines {
            path: Box::new(args.remove(0)),
            lines: Box::new(args.remove(0)),
        },
        ("cli.parse", 0) => Expr::CliParse,
        ("json.parse", 1) => Expr::JsonParse {
            value: Box::new(args.remove(0)),
        },
        ("json.stringify", 1) => match args.remove(0) {
            Expr::Ident(name) => Expr::JsonStringify { name },
            value => Expr::JsonStringifyValue {
                value: Box::new(value),
            },
        },
        _ if name
            .rsplit('.')
            .next()
            .is_some_and(|part| part.starts_with(|ch: char| ch.is_ascii_uppercase()))
            && args.len() == 1 =>
        {
            Expr::NewtypeCtor {
                name,
                value: Box::new(args.remove(0)),
            }
        }
        _ => Expr::Call { name, args },
    }
}

fn allowed_command_name(name: &str) -> Option<(String, String, bool)> {
    let mut parts = name.split('.');
    if parts.next()? != "run" {
        return None;
    }
    let first = parts.next()?;
    let (group, result) = if first == "result" {
        (parts.next()?, true)
    } else {
        (first, false)
    };
    let command = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    Some((group.to_string(), command.to_string(), result))
}

fn named_or_value<T>(
    receiver: Expr,
    named: impl FnOnce(String) -> T,
    value: impl FnOnce(Expr) -> T,
) -> T {
    match receiver {
        Expr::Ident(name) => named(name),
        other => value(other),
    }
}

fn qualified_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(name) => Some(name.clone()),
        Expr::Field { name, field } => Some(format!("{name}.{field}")),
        Expr::FieldValue { value, field } => {
            qualified_name(value).map(|name| format!("{name}.{field}"))
        }
        _ => None,
    }
}

fn invalid_method_arity(
    receiver: Expr,
    method: &str,
    args: Vec<Expr>,
    message: &'static str,
) -> Result<Expr, &'static str> {
    if let Some(name) = qualified_name(&receiver) {
        return Ok(call_expr(format!("{name}.{method}"), args));
    }
    Err(message)
}

fn method_expr(receiver: Expr, method: String, mut args: Vec<Expr>) -> Result<Expr, &'static str> {
    let arity = args.len();
    let result = match (method.as_str(), arity) {
        ("wait", 0) => named_or_value(receiver, Expr::Await, |value| Expr::Call {
            name: "wait".into(),
            args: vec![value],
        }),
        ("value", 0) => named_or_value(receiver, Expr::Value, |value| Expr::FieldValue {
            value: Box::new(value),
            field: "value".into(),
        }),
        ("len", 0) => named_or_value(receiver, Expr::Len, |value| match value {
            Expr::Array(_) => Expr::ArrayLenValue(Box::new(value)),
            Expr::Map(_) => Expr::MapLenValue(Box::new(value)),
            _ => Expr::StringLenValue(Box::new(value)),
        }),
        ("isEmpty", 0) => named_or_value(receiver, Expr::IsEmpty, |value| match value {
            Expr::Array(_) => Expr::ArrayIsEmptyValue(Box::new(value)),
            Expr::Map(_) => Expr::MapIsEmptyValue(Box::new(value)),
            _ => Expr::StringIsEmptyValue(Box::new(value)),
        }),
        ("first", 0) => named_or_value(receiver, Expr::ArrayFirst, |value| {
            Expr::ArrayFirstValue(Box::new(value))
        }),
        ("last", 0) => named_or_value(receiver, Expr::ArrayLast, |value| {
            Expr::ArrayLastValue(Box::new(value))
        }),
        ("reverse", 0) => named_or_value(receiver, Expr::ArrayReverse, |value| {
            Expr::ArrayReverseValue(Box::new(value))
        }),
        ("sort", 0) => named_or_value(receiver, Expr::ArraySort, |value| {
            Expr::ArraySortValue(Box::new(value))
        }),
        ("unique", 0) => named_or_value(receiver, Expr::ArrayUnique, |value| {
            Expr::ArrayUniqueValue(Box::new(value))
        }),
        ("keys", 0) => named_or_value(receiver, Expr::MapKeys, |value| {
            Expr::MapKeysValue(Box::new(value))
        }),
        ("values", 0) => named_or_value(receiver, Expr::MapValues, |value| {
            Expr::MapValuesValue(Box::new(value))
        }),
        ("pop", 0) => {
            let Some(name) = ident_name(&receiver) else {
                return Err("pop requires a named array");
            };
            Expr::ArrayPop {
                name: name.to_string(),
            }
        }
        ("trim", 0) => named_or_value(receiver, Expr::StringTrim, |value| {
            Expr::StringTrimValue(Box::new(value))
        }),
        ("trimStart", 0) => named_or_value(receiver, Expr::StringTrimStart, |value| {
            Expr::StringTrimStartValue(Box::new(value))
        }),
        ("trimEnd", 0) => named_or_value(receiver, Expr::StringTrimEnd, |value| {
            Expr::StringTrimEndValue(Box::new(value))
        }),
        ("toUpper", 0) => named_or_value(receiver, Expr::StringToUpper, |value| {
            Expr::StringToUpperValue(Box::new(value))
        }),
        ("toLower", 0) => named_or_value(receiver, Expr::StringToLower, |value| {
            Expr::StringToLowerValue(Box::new(value))
        }),
        ("isAbsolute", 0) => named_or_value(receiver, Expr::PathIsAbsolute, |value| {
            Expr::PathIsAbsoluteValue(Box::new(value))
        }),
        ("basename", 0) => named_or_value(receiver, Expr::PathBasename, |value| {
            Expr::PathBasenameValue(Box::new(value))
        }),
        ("dirname", 0) => named_or_value(receiver, Expr::PathDirname, |value| {
            Expr::PathDirnameValue(Box::new(value))
        }),
        ("stem", 0) => named_or_value(receiver, Expr::PathStem, |value| {
            Expr::PathStemValue(Box::new(value))
        }),
        ("extname", 0) => named_or_value(receiver, Expr::PathExtname, |value| {
            Expr::PathExtnameValue(Box::new(value))
        }),
        ("map", 1) => {
            let mapper = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::ArrayMap {
                    name,
                    mapper: mapper.clone(),
                },
                |value| Expr::ArrayMapValue {
                    value: Box::new(value),
                    mapper: mapper.clone(),
                },
            )
        }
        ("flatMap", 1) => {
            let mapper = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::OptionFlatMap {
                    name,
                    mapper: mapper.clone(),
                },
                |value| Expr::OptionFlatMapValue {
                    value: Box::new(value),
                    mapper: mapper.clone(),
                },
            )
        }
        ("ap", 1) => {
            let value = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::OptionAp {
                    name,
                    value: value.clone(),
                },
                |function| Expr::OptionApValue {
                    function: Box::new(function),
                    value: value.clone(),
                },
            )
        }
        ("orElse", 1) => {
            let fallback = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::OptionOrElse {
                    name,
                    fallback: fallback.clone(),
                },
                |value| Expr::OptionOrElseValue {
                    value: Box::new(value),
                    fallback: fallback.clone(),
                },
            )
        }
        ("join", 1) => {
            let separator = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::Join {
                    name,
                    separator: separator.clone(),
                },
                |value| Expr::JoinValue {
                    value: Box::new(value),
                    separator: separator.clone(),
                },
            )
        }
        ("take", 1) => {
            let count = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::ArrayTake {
                    name,
                    count: count.clone(),
                },
                |value| Expr::ArrayTakeValue {
                    value: Box::new(value),
                    count: count.clone(),
                },
            )
        }
        ("drop", 1) => {
            let count = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::ArrayDrop {
                    name,
                    count: count.clone(),
                },
                |value| Expr::ArrayDropValue {
                    value: Box::new(value),
                    count: count.clone(),
                },
            )
        }
        ("push", 1) => {
            let Some(name) = ident_name(&receiver) else {
                return Err("push requires a named array");
            };
            Expr::ArrayPush {
                name: name.to_string(),
                value: Box::new(args.remove(0)),
            }
        }
        ("contains", 1) => {
            let item = args.remove(0);
            named_or_value(
                receiver,
                |name| Expr::StringContains {
                    name,
                    needle: Box::new(item.clone()),
                },
                |value| match value {
                    Expr::Array(_) => Expr::ArrayContainsValue {
                        value: Box::new(value),
                        item: Box::new(item.clone()),
                    },
                    _ => Expr::StringContainsValue {
                        value: Box::new(value),
                        needle: Box::new(item.clone()),
                    },
                },
            )
        }
        ("indexOf", 1) => {
            let item = args.remove(0);
            named_or_value(
                receiver,
                |name| Expr::ArrayIndexOf {
                    name,
                    value: Box::new(item.clone()),
                },
                |value| match value {
                    Expr::Array(_) => Expr::ArrayIndexOfValue {
                        value: Box::new(value),
                        item: Box::new(item.clone()),
                    },
                    _ => Expr::StringIndexOfValue {
                        value: Box::new(value),
                        needle: Box::new(item.clone()),
                    },
                },
            )
        }
        ("startsWith", 1) => {
            let prefix = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::StringStartsWith {
                    name,
                    prefix: prefix.clone(),
                },
                |value| Expr::StringStartsWithValue {
                    value: Box::new(value),
                    prefix: prefix.clone(),
                },
            )
        }
        ("endsWith", 1) => {
            let suffix = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::StringEndsWith {
                    name,
                    suffix: suffix.clone(),
                },
                |value| Expr::StringEndsWithValue {
                    value: Box::new(value),
                    suffix: suffix.clone(),
                },
            )
        }
        ("repeat", 1) => {
            let count = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::StringRepeat {
                    name,
                    count: count.clone(),
                },
                |value| Expr::StringRepeatValue {
                    value: Box::new(value),
                    count: count.clone(),
                },
            )
        }
        ("split", 1) => {
            let separator = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::StringSplit {
                    name,
                    separator: separator.clone(),
                },
                |value| Expr::StringSplitValue {
                    value: Box::new(value),
                    separator: separator.clone(),
                },
            )
        }
        ("has", 1) => {
            let key = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::MapHas {
                    name,
                    key: key.clone(),
                },
                |value| Expr::MapHasValue {
                    value: Box::new(value),
                    key: key.clone(),
                },
            )
        }
        ("remove", 1) => {
            let Some(name) = ident_name(&receiver) else {
                return Err("remove requires a named map");
            };
            Expr::MapRemove {
                name: name.to_string(),
                key: Box::new(args.remove(0)),
            }
        }
        ("slice", 2) => {
            let start = Box::new(args.remove(0));
            let end = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::Slice {
                    name,
                    start: start.clone(),
                    end: end.clone(),
                },
                |value| match value {
                    Expr::Array(_) => Expr::ArraySliceValue {
                        value: Box::new(value),
                        start: start.clone(),
                        end: end.clone(),
                    },
                    _ => Expr::StringSliceValue {
                        value: Box::new(value),
                        start: start.clone(),
                        end: end.clone(),
                    },
                },
            )
        }
        ("set", 2) => {
            let Some(name) = ident_name(&receiver) else {
                return Err("set requires a named map");
            };
            Expr::MapSet {
                name: name.to_string(),
                key: Box::new(args.remove(0)),
                value: Box::new(args.remove(0)),
            }
        }
        ("replace", 2) => {
            let from = Box::new(args.remove(0));
            let to = Box::new(args.remove(0));
            named_or_value(
                receiver,
                |name| Expr::StringReplace {
                    name,
                    from: from.clone(),
                    to: to.clone(),
                },
                |value| Expr::StringReplaceValue {
                    value: Box::new(value),
                    from: from.clone(),
                    to: to.clone(),
                },
            )
        }
        ("len" | "isEmpty" | "trim" | "trimStart" | "trimEnd" | "toUpper" | "toLower", _) => {
            let message = match method.as_str() {
                "len" => "len expects no arguments",
                "isEmpty" => "isEmpty expects no arguments",
                "trim" => "trim expects no arguments",
                "trimStart" => "trimStart expects no arguments",
                "trimEnd" => "trimEnd expects no arguments",
                "toUpper" => "toUpper expects no arguments",
                _ => "toLower expects no arguments",
            };
            return invalid_method_arity(receiver, &method, args, message);
        }
        ("basename", _) => {
            return invalid_method_arity(receiver, &method, args, "basename expects no arguments");
        }
        ("contains", _) => {
            return invalid_method_arity(
                receiver,
                &method,
                args,
                "contains expects one needle argument",
            );
        }
        ("indexOf", _) => {
            return invalid_method_arity(
                receiver,
                &method,
                args,
                "indexOf expects one needle argument",
            );
        }
        ("startsWith", _) => {
            return invalid_method_arity(
                receiver,
                &method,
                args,
                "startsWith expects one prefix argument",
            );
        }
        ("endsWith", _) => {
            return invalid_method_arity(
                receiver,
                &method,
                args,
                "endsWith expects one suffix argument",
            );
        }
        ("repeat", _) => {
            return invalid_method_arity(
                receiver,
                &method,
                args,
                "repeat expects one count argument",
            );
        }
        ("slice", _) => {
            return invalid_method_arity(
                receiver,
                &method,
                args,
                "slice expects start and end arguments",
            );
        }
        ("replace", _) => {
            return invalid_method_arity(
                receiver,
                &method,
                args,
                "replace expects search and replacement arguments",
            );
        }
        _ => {
            if let Some(name) = qualified_name(&receiver) {
                return Ok(call_expr(format!("{name}.{method}"), args));
            }
            let mut source_args = Vec::with_capacity(args.len() + 1);
            source_args.push(receiver);
            source_args.extend(args);
            return Ok(call_expr(method, source_args));
        }
    };
    Ok(result)
}

fn apply_postfix(mut value: Expr, suffixes: Vec<Postfix>) -> Result<Expr, &'static str> {
    for suffix in suffixes {
        value = match suffix {
            Postfix::Call(args) => {
                let Expr::Ident(name) = value else {
                    return Err("only named functions can be called");
                };
                call_expr(name, args)
            }
            Postfix::Method(name, args) => method_expr(value, name, args)?,
            Postfix::Index(index) => match value {
                Expr::Ident(name) => Expr::Index {
                    name,
                    index: Box::new(index),
                },
                value => Expr::IndexValue {
                    value: Box::new(value),
                    index: Box::new(index),
                },
            },
            Postfix::Field(field) => match value {
                Expr::Ident(name) if field == "value" => Expr::Value(name),
                Expr::Ident(name) => Expr::Field { name, field },
                value => Expr::FieldValue {
                    value: Box::new(value),
                    field,
                },
            },
            Postfix::TupleField(field) => match value {
                Expr::Ident(name) => Expr::TupleField { name, field },
                value => Expr::TupleFieldValue {
                    value: Box::new(value),
                    field,
                },
            },
            Postfix::ResultOption => Expr::ResultOption(Box::new(value)),
            Postfix::TryResult => Expr::TryResult(Box::new(value)),
        };
    }
    Ok(value)
}

fn collection(entries: Vec<CollectionEntry>) -> Result<Expr, &'static str> {
    if entries.is_empty() {
        return Ok(Expr::Map(Vec::new()));
    }
    if entries
        .iter()
        .all(|entry| matches!(entry, CollectionEntry::Record(_, _)))
    {
        return Ok(Expr::Record(
            entries
                .into_iter()
                .map(|entry| match entry {
                    CollectionEntry::Record(name, value) => (name, value),
                    CollectionEntry::Map(_, _) => unreachable!(),
                })
                .collect(),
        ));
    }
    if entries
        .iter()
        .all(|entry| matches!(entry, CollectionEntry::Map(_, _)))
    {
        return Ok(Expr::Map(
            entries
                .into_iter()
                .map(|entry| match entry {
                    CollectionEntry::Map(key, value) => (key, value),
                    CollectionEntry::Record(_, _) => unreachable!(),
                })
                .collect(),
        ));
    }
    Err("cannot mix record and map entries")
}

fn do_expression(items: Vec<DoItem>) -> Result<Expr, &'static str> {
    let Some((last, preceding)) = items.split_last() else {
        return Err("do expression requires a result expression");
    };
    let DoItem::Expr(result) = last else {
        return Err("do expression must end with a result expression");
    };
    let mut steps = Vec::with_capacity(preceding.len());
    for item in preceding {
        match item {
            DoItem::Bind(name, expr) => steps.push(DoStep::Bind {
                name: name.clone(),
                expr: expr.clone(),
            }),
            DoItem::Let(name, annotation, expr) => steps.push(DoStep::Let {
                name: name.clone(),
                annotation: annotation.clone(),
                expr: expr.clone(),
            }),
            DoItem::Expr(_) => return Err("only the final do item may be an expression"),
        }
    }
    Ok(Expr::Do {
        steps,
        result: Box::new(result.clone()),
    })
}

fn mark_checked_command(expr: &mut Expr) -> bool {
    match expr {
        Expr::Command { checked, .. } => {
            *checked = true;
            true
        }
        Expr::Pipeline { input, commands } => {
            *expr = Expr::TryPipeline {
                input: input.take(),
                commands: std::mem::take(commands),
            };
            true
        }
        Expr::StringContainsValue { value, .. }
        | Expr::StringIndexOfValue { value, .. }
        | Expr::StringStartsWithValue { value, .. }
        | Expr::StringEndsWithValue { value, .. }
        | Expr::StringSliceValue { value, .. }
        | Expr::StringRepeatValue { value, .. }
        | Expr::StringSplitValue { value, .. }
        | Expr::StringReplaceValue { value, .. }
        | Expr::StringLenValue(value)
        | Expr::StringIsEmptyValue(value)
        | Expr::StringTrimValue(value)
        | Expr::StringTrimStartValue(value)
        | Expr::StringTrimEndValue(value)
        | Expr::StringToUpperValue(value)
        | Expr::StringToLowerValue(value)
        | Expr::PathBasenameValue(value)
        | Expr::PathDirnameValue(value)
        | Expr::PathStemValue(value)
        | Expr::PathExtnameValue(value)
        | Expr::PathIsAbsoluteValue(value)
        | Expr::FieldValue { value, .. } => mark_checked_command(value),
        _ => false,
    }
}

fn pipeline(mut first: Expr, commands: Vec<String>, checked: bool) -> Expr {
    if commands.is_empty() {
        if !checked {
            return first;
        }
        if mark_checked_command(&mut first) {
            return first;
        }
        return Expr::TryResult(Box::new(first));
    }

    let (input, mut stages) = match first {
        Expr::Command { command, .. } => (None, vec![command]),
        other => (Some(Box::new(other)), Vec::new()),
    };
    stages.extend(commands);
    if checked {
        Expr::TryPipeline {
            input,
            commands: stages,
        }
    } else {
        Expr::Pipeline {
            input,
            commands: stages,
        }
    }
}

fn match_constructor(name: String, mut args: Vec<Expr>) -> Result<Expr, &'static str> {
    match (name.as_str(), args.len()) {
        ("Some", 1) => Ok(Expr::Some(Box::new(args.remove(0)))),
        ("Ok", 1) => Ok(Expr::Ok(Box::new(args.remove(0)))),
        ("Err", 1) => Ok(Expr::Err(Box::new(args.remove(0)))),
        ("Some" | "Ok" | "Err", _) => Err("constructor pattern arity"),
        _ => Ok(Expr::Call { name, args }),
    }
}

fn trait_method(head: FunctionHead) -> Result<TraitMethod, &'static str> {
    if !head.type_params.is_empty() {
        return Err("trait methods cannot declare type parameters");
    }
    if head.params.iter().any(|param| param.default.is_some()) {
        return Err("trait methods cannot declare default parameters");
    }
    Ok(TraitMethod {
        name: head.name,
        params: head.params,
        return_type: head.return_type,
    })
}

fn impl_method(head: FunctionHead, body: Program) -> Result<ImplMethod, &'static str> {
    if !head.type_params.is_empty() {
        return Err("impl methods cannot declare type parameters");
    }
    Ok(ImplMethod {
        name: head.name,
        params: head.params,
        return_type: head.return_type,
        body,
    })
}

fn expression_statement(expr: Expr) -> Statement {
    match expr {
        Expr::Command {
            command,
            checked: true,
        } => Statement::TryCommand(command),
        Expr::Command {
            command,
            checked: false,
        } => Statement::Command(command),
        Expr::TryPipeline { input, commands } => Statement::TryPipeline { input, commands },
        Expr::Pipeline {
            input: None,
            commands,
        } => Statement::Command(commands.join(" | ")),
        Expr::Pipeline { input, commands } => Statement::Expr(Expr::Pipeline { input, commands }),
        Expr::TryResult(value) => Statement::TryResult(*value),
        other => Statement::Expr(other),
    }
}

fn binding_statement(mutable: bool, target: BindingTarget, expr: Expr) -> Statement {
    match target {
        BindingTarget::Pattern(pattern) => Statement::Destructure {
            mutable,
            pattern,
            expr,
        },
        BindingTarget::Name(name, annotation) if mutable => Statement::Let {
            name,
            annotation,
            expr,
        },
        BindingTarget::Name(name, annotation) => Statement::Const {
            name,
            annotation,
            expr,
        },
    }
}

peg::parser! {
    grammar nacre_grammar() for str {
        rule ws() = quiet!{[' ' | '\t' | '\r' | '\n']*}
        rule comma() = ws() "," ws()

        rule identifier() -> String
            = value:$(['A'..='Z' | 'a'..='z' | '_']
                ['A'..='Z' | 'a'..='z' | '0'..='9' | '_']*)
            { value.to_string() }

        rule language_keyword()
            = ("as" / "async" / "await" / "break" / "const" / "continue" /
               "defer" / "do" / "else" / "export" / "false" / "fn" / "fn!" / "for" /
               "if" / "impl" / "in" / "let" / "match" / "newtype" / "raw" /
               "requireOneOf" / "require" / "return" / "spawn" / "trait" /
               "true" / "try" / "type" / "use" / "while") !identifier_continue()

        rule type_identifier() -> String
            = value:$(['A'..='Z']
                ['A'..='Z' | 'a'..='z' | '0'..='9' | '_']*)
            { value.to_string() }

        rule type_name() -> String
            = head:identifier() tail:(ws() "." ws() part:identifier() { part })+
            {
                let mut parts = Vec::with_capacity(tail.len() + 1);
                parts.push(head);
                parts.extend(tail);
                parts.join(".")
            }
            / name:type_identifier() { name }

        pub rule program_root() -> Vec<ParsedStatement>
            = file_trivia() statements:program_items() file_trivia() ![_] { statements }

        rule program_items() -> Vec<ParsedStatement>
            = statements:(located_statement() ** statement_separator()) { statements }

        rule located_statement() -> ParsedStatement
            = hws() offset:position!() statement:statement()
                { ParsedStatement { offset, statement } }

        rule statement() -> Statement
            = export_statement()
            / external_function_statement()
            / function_statement()
            / trait_statement()
            / impl_statement()
            / raw_statement()
            / if_statement()
            / while_statement()
            / for_statement()
            / defer_statement()
            / block_statement()
            / use_statement()
            / newtype_statement()
            / type_statement()
            / binding_statement_rule()
            / return_statement()
            / break_statement()
            / continue_statement()
            / require_one_of_statement()
            / require_statement()
            / redirect_statement()
            / assignment_statement()
            / expression_statement_rule()

        rule statement_separator()
            = line_tail() newline()+ file_trivia()

        rule file_trivia()
            = ((hws() comment()? newline()+) / shebang_line())*

        rule line_tail() = hws() comment()?
        rule newline() = "\r\n" / "\n"
        rule comment() = "##" (!newline() [_])*
        rule shebang_line() = hws() "#!" (!newline() [_])* newline()+

        rule block_body() -> Program
            = line_tail() newline()+ file_trivia()
              statements:program_items()? file_trivia() hws()
            {
                let statements = statements.unwrap_or_default();
                let lines = vec![1; statements.len()];
                Program::new(
                    statements.into_iter().map(|item| item.statement).collect(),
                    lines,
                )
            }

        rule external_function_statement() -> Statement
            = "export" hws1() "fn" hws1() head:function_signature()
            {
                Statement::ExternalFunction {
                    name: head.name,
                    type_params: head.type_params,
                    params: head.params,
                    return_type: head.return_type,
                }
            }

        rule export_statement() -> Statement
            = "export" hws1() statement:exportable_statement()
            { Statement::Export(Box::new(statement)) }

        rule exportable_statement() -> Statement
            = function_statement()
            / trait_statement()
            / impl_statement()
            / use_statement_inner(true)
            / newtype_statement()
            / type_statement()
            / binding_statement_rule()

        rule function_statement() -> Statement
            = marker:("fn!" { true } / "fn" { false }) hws1()
              head:function_signature_with_marker(marker) hws() "{"
              body:block_body() "}"
            {
                Statement::Function {
                    name: head.name,
                    override_constructor: head.override_constructor,
                    type_params: head.type_params,
                    params: head.params,
                    return_type: head.return_type,
                    body,
                }
            }

        rule function_signature_with_marker(override_constructor: bool) -> FunctionHead
            = name:identifier()
              type_params:type_params()?
              hws() "(" hws() params:(parameter() ** comma()) hws() ")"
              hws() ":" hws() return_type:type_expr()
            {
                FunctionHead {
                    name,
                    override_constructor,
                    type_params: type_params.unwrap_or_default(),
                    params,
                    return_type,
                }
            }

        rule function_signature() -> FunctionHead
            = function_signature_with_marker(false)

        rule type_params() -> Vec<TypeParam>
            = "[" ws() values:(type_param() ++ comma()) ws() "]" { values }

        rule type_param() -> TypeParam
            = name:type_identifier()
              bounds:(hws() ":" hws() values:(type_name() ++ (hws() "+" hws())) { values })?
            { TypeParam { name, bounds: bounds.unwrap_or_default() } }

        rule parameter() -> Param
            = name:identifier() hws() ":" hws()
              variadic:("..." { true })? ty:type_expr()
              default:(hws() "=" hws() value:expression() { value })?
            {
                Param {
                    name,
                    ty: if variadic.unwrap_or(false) {
                        Type::Array(Box::new(ty))
                    } else {
                        ty
                    },
                    default,
                    variadic: variadic.unwrap_or(false),
                    capture_name: None,
                }
            }

        rule trait_statement() -> Statement
            = "trait" hws1() name:type_identifier() hws()
              "[" hws() type_param:type_identifier() hws() "]"
              hws() "{" line_tail() newline()+ file_trivia()
              methods:(trait_method() ** statement_separator())? file_trivia() hws() "}"
            {
                Statement::Trait {
                    name,
                    type_param,
                    methods: methods.unwrap_or_default(),
                }
            }

        rule trait_method() -> TraitMethod
            = hws() "fn" hws1() head:function_signature()
            {? super::trait_method(head) }

        rule impl_statement() -> Statement
            = "impl" hws1() trait_name:type_identifier() hws()
              "[" hws() for_type:type_expr() hws() "]"
              hws() "{" line_tail() newline()+ file_trivia()
              methods:(impl_method() ** statement_separator())? file_trivia() hws() "}"
            {
                Statement::Impl {
                    trait_name,
                    for_type,
                    methods: methods.unwrap_or_default(),
                }
            }
            / "impl" hws1() for_type:type_expr()
              hws() "{" line_tail() newline()+ file_trivia()
              items:(impl_item() ** statement_separator())? file_trivia() hws() "}"
            {
                let mut consts = Vec::new();
                let mut methods = Vec::new();
                for item in items.unwrap_or_default() {
                    match item {
                        ImplItem::Const(value) => consts.push(value),
                        ImplItem::Method(value) => methods.push(value),
                    }
                }
                Statement::InherentImpl {
                    for_type,
                    consts,
                    methods,
                }
            }

        rule impl_item() -> ImplItem
            = value:impl_const() { ImplItem::Const(value) }
            / value:impl_method() { ImplItem::Method(value) }

        rule impl_const() -> ImplConst
            = hws() "const" hws1() name:identifier()
              annotation:(hws() ":" hws() ty:type_expr() { ty })?
              hws() "=" hws() expr:expression()
            { ImplConst { name, annotation, expr } }

        rule impl_method() -> ImplMethod
            = hws() "fn" hws1() head:function_signature() hws() "{"
              body:block_body() "}"
            {? super::impl_method(head, body) }

        rule raw_statement() -> Statement
            = "raw" hws() "{" line_tail() newline()
              value:$(raw_content()*) hws() "}"
            { Statement::Raw(value.to_string()) }

        rule raw_content()
            = raw_nested()
            / !(hws() "}" line_tail() (&newline() / ![_])) raw_line()

        rule raw_nested()
            = hws() "raw" hws() "{" line_tail() newline()
              raw_content()* hws() "}" line_tail() newline()?

        rule raw_line()
            = (!newline() [_])+ newline()?
            / newline()

        rule if_statement() -> Statement
            = "if" hws1() condition:expression() hws() "{"
              then_branch:block_body() "}"
              else_branch:else_statement_clause()?
            { Statement::If { condition, then_branch, else_branch } }

        rule else_statement_clause() -> Program
            = ((line_tail() newline()+ file_trivia() hws()) / hws())
              "else" hws()
              statement:(if_statement() / ("{" body:block_body() "}" { Statement::Block { body } }))
            {
                match statement {
                    Statement::Block { body } => body,
                    statement => Program::new(vec![statement], vec![1]),
                }
            }

        rule while_statement() -> Statement
            = "while" hws1() condition:expression() hws() "{"
              body:block_body() "}"
            { Statement::While { condition, body } }

        rule for_statement() -> Statement
            = "for" hws1() binding:for_binding() hws1() "in" hws1()
              iterable:expression() hws() "{" body:block_body() "}"
            { Statement::For { binding, iterable, body } }

        rule for_binding() -> ForBinding
            = pattern:binding_pattern() { ForBinding::Pattern(pattern) }
            / name:identifier() { ForBinding::Name(name) }

        rule block_statement() -> Statement
            = "{" body:block_body() "}" { Statement::Block { body } }

        rule defer_statement() -> Statement
            = "defer" hws1() statement:(block_statement() / expression_statement_rule())
            { Statement::Defer(Box::new(statement)) }

        rule use_statement() -> Statement
            = use_statement_inner(false)

        rule use_statement_inner(re_export: bool) -> Statement
            = "use" hws1() path:(identifier() ++ (hws() "." hws()))
              items:(hws() "." hws() "{" ws() items:(use_item() ++ comma()) ws() "}" { items })?
              alias:(hws1() "as" hws1() name:identifier() { name })?
            { Statement::Use { path, alias, items: items.unwrap_or_default(), re_export } }

        rule use_item() -> UseItem
            = name:identifier() alias:(hws1() "as" hws1() alias:identifier() { alias })?
            { UseItem { name, alias } }

        rule newtype_statement() -> Statement
            = "newtype" hws1() name:type_identifier()
              type_params:("[" ws() values:(type_identifier() ++ comma()) ws() "]" { values })?
              hws() "=" hws() base:type_expr()
            { Statement::Newtype { name, type_params: type_params.unwrap_or_default(), base } }

        rule type_statement() -> Statement
            = "type" hws1() name:type_identifier()
              type_params:("[" ws() values:(type_identifier() ++ comma()) ws() "]" { values })?
              hws() "=" ws() declaration:type_declaration()
            {
                match declaration {
                    TypeDeclaration::Alias(ty) => Statement::TypeAlias {
                        name,
                        type_params: type_params.unwrap_or_default(),
                        ty,
                    },
                    TypeDeclaration::Sum(variants) => Statement::SumType {
                        name,
                        type_params: type_params.unwrap_or_default(),
                        variants,
                    },
                }
            }

        rule type_declaration() -> TypeDeclaration
            = variants:sum_variants() { TypeDeclaration::Sum(variants) }
            / ty:type_expr() { TypeDeclaration::Alias(ty) }

        rule sum_variants() -> Vec<VariantDecl>
            = "|" ws() first:variant_decl() ws() "|" ws() second:variant_decl()
              rest:(ws() "|" ws() value:variant_decl() { value })*
            {
                let mut variants = Vec::with_capacity(rest.len() + 2);
                variants.push(first);
                variants.push(second);
                variants.extend(rest);
                variants
            }
            / first:variant_decl() ws() "|" ws() second:variant_decl()
              rest:(ws() "|" ws() value:variant_decl() { value })*
            {
                let mut variants = Vec::with_capacity(rest.len() + 2);
                variants.push(first);
                variants.push(second);
                variants.extend(rest);
                variants
            }
            / value:variant_decl_with_fields() { vec![value] }

        rule variant_decl() -> VariantDecl
            = variant_decl_with_fields()
            / name:type_identifier() { VariantDecl { name, fields: Vec::new() } }

        rule variant_decl_with_fields() -> VariantDecl
            = name:type_identifier() hws() "(" ws() fields:(type_expr() ** comma()) ws() ")"
                { VariantDecl { name, fields } }

        rule binding_statement_rule() -> Statement
            = mutable:("const" { false } / "let" { true }) hws1()
              target:binding_target() hws() "=" hws() value:expression()
            { super::binding_statement(mutable, target, value) }

        rule binding_target() -> BindingTarget
            = pattern:binding_pattern() { BindingTarget::Pattern(pattern) }
            / name:identifier()
              annotation:(hws() ":" hws() ty:type_expr() { ty })?
                { BindingTarget::Name(name, annotation) }

        rule binding_pattern() -> BindingPattern
            = binding_pattern_compound()

        rule binding_pattern_value() -> BindingPattern
            = binding_pattern_compound()
            / name:identifier() { BindingPattern::Name(name) }

        rule binding_pattern_compound() -> BindingPattern
            = "(" ws() values:(binding_pattern_value() ++ comma()) ws() ")"
                { BindingPattern::Tuple(values) }
            / "{" ws() fields:(binding_record_field() ++ comma()) ws() "}"
                { BindingPattern::Record(fields) }
            / "[" ws() patterns:(binding_pattern_value() ** comma())
              rest:(comma() "..." name:identifier() { name })? ws() "]"
                { BindingPattern::Array { patterns, rest } }

        rule binding_record_field() -> (String, BindingPattern)
            = field:identifier()
              value:(ws() ":" ws() value:binding_pattern_value() { value })?
                {
                    (field.clone(), value.unwrap_or(BindingPattern::Name(field)))
                }

        rule return_statement() -> Statement
            = "return" hws1() value:expression() { Statement::Return(value) }

        rule break_statement() -> Statement
            = "break" !identifier_continue() { Statement::Break }

        rule continue_statement() -> Statement
            = "continue" !identifier_continue() { Statement::Continue }

        rule require_statement() -> Statement
            = "require" hws() "(" ws() command:string_value()
              version:(comma() "version" hws() "=" hws() value:string_value() { value })?
              ws() ")"
            { Statement::Require { command, version } }

        rule require_one_of_statement() -> Statement
            = "requireOneOf" hws() "(" ws() "[" ws()
              commands:(string_value() ++ comma()) ws() "]" ws() ")"
            { Statement::RequireOneOf { commands } }

        rule redirect_statement() -> Statement
            = first:shell_command()
              rest:(hws() "|>" hws() command:shell_command() { command })*
              hws() ">>" hws()
              append:("write" { false } / "append" { true })
              hws() "(" ws() target:string_value()
              stderr:(comma() "stderr" hws() "=" hws() value:string_value() { value })?
              ws() ")"
            {
                let mut commands = vec![first];
                commands.extend(rest);
                Statement::Redirect {
                    command: commands.join(" | "),
                    target,
                    stderr,
                    append,
                }
            }

        rule assignment_statement() -> Statement
            = !language_keyword() name:identifier() hws() op:compound_assignment_op()
              hws() value:expression()
                { super::compound_assignment(name, op, value) }
            / target:assignment_target() hws() "=" !"=" hws() value:expression()
                { Statement::Assign { target, expr: value } }

        rule assignment_target() -> AssignTarget
            = !language_keyword() name:identifier() ws() "." ws() field:identifier()
              ws() "[" ws() index:expression() ws() "]"
                { AssignTarget::FieldIndex { name, field, index } }
            / !language_keyword() name:identifier() ws() "[" ws() index:expression() ws() "]"
                { AssignTarget::Index { name, index } }
            / !language_keyword() name:identifier() ws() "." ws() "_" field:$(['1'..='9']['0'..='9']*)
                {? field.parse().map(|field| AssignTarget::TupleField { name, field }).or(Err("tuple field")) }
            / !language_keyword() name:identifier() ws() "." ws() field:identifier()
                { AssignTarget::Field { name, field } }
            / !language_keyword() name:identifier()
                { AssignTarget::Name(name) }

        rule compound_assignment_op() -> BinaryOp
            = "++=" { BinaryOp::Concat }
            / "<<=" { BinaryOp::Shl }
            / ">>=" { BinaryOp::Shr }
            / "+=" { BinaryOp::Add }
            / "-=" { BinaryOp::Sub }
            / "*=" { BinaryOp::Mul }
            / "/=" { BinaryOp::Div }
            / "%=" { BinaryOp::Mod }
            / "&=" { BinaryOp::BitAnd }
            / "|=" { BinaryOp::BitOr }
            / "^=" { BinaryOp::BitXor }

        rule expression_statement_rule() -> Statement
            = value:expression() { super::expression_statement(value) }

        pub rule type_root() -> Type
            = ws() value:type_expr() ws() ![_] { value }

        pub rule expression_root() -> Expr
            = ws() value:expression() ws() ![_] { value }

        rule expression() -> Expr
            = lambda()

        rule lambda() -> Expr
            = params:lambda_params() ws() "=>" ws() body:expression()
                { Expr::Lambda { params, body: Box::new(body) } }
            / pipeline()

        rule lambda_params() -> Vec<String>
            = name:identifier() { vec![name] }
            / "(" ws() names:(identifier() ** comma()) ws() ")" { names }

        rule pipeline() -> Expr
            = checked:("try" ws1() { true })? first:operator_expr()
              stages:(ws() "|>" ws() command:shell_command() { command })*
            { super::pipeline(first, stages, checked.unwrap_or(false)) }

        rule operator_expr() -> Expr = precedence! {
            left:(@) ws() "??" ws() right:@ {
                match (left, right) {
                    (Expr::Env(name), Expr::String(default)) =>
                        Expr::EnvDefault { name, default },
                    (left, right) => super::default_expr(left, right),
                }
            }
            --
            left:(@) ws() "<|>" ws() right:@ {
                named_or_value(
                    left,
                    |name| Expr::OptionOrElse {
                        name,
                        fallback: Box::new(right.clone()),
                    },
                    |value| Expr::OptionOrElseValue {
                        value: Box::new(value),
                        fallback: Box::new(right.clone()),
                    },
                )
            }
            --
            left:(@) ws() ">>=" ws() right:@ {
                named_or_value(
                    left,
                    |name| Expr::OptionFlatMap {
                        name,
                        mapper: Box::new(right.clone()),
                    },
                    |value| Expr::OptionFlatMapValue {
                        value: Box::new(value),
                        mapper: Box::new(right.clone()),
                    },
                )
            }
            --
            left:(@) ws() "<*>" ws() right:@ {
                named_or_value(
                    left,
                    |name| Expr::OptionAp {
                        name,
                        value: Box::new(right.clone()),
                    },
                    |function| Expr::OptionApValue {
                        function: Box::new(function),
                        value: Box::new(right.clone()),
                    },
                )
            }
            --
            left:(@) ws() "<$>" ws() right:@ {
                named_or_value(
                    left,
                    |name| Expr::ArrayMap {
                        name,
                        mapper: Box::new(right.clone()),
                    },
                    |value| Expr::ArrayMapValue {
                        value: Box::new(value),
                        mapper: Box::new(right.clone()),
                    },
                )
            }
            --
            left:(@) ws() "..=" ws() right:@ {
                Expr::Range {
                    start: Box::new(left),
                    end: Box::new(right),
                    inclusive: true,
                }
            }
            left:(@) ws() ".." !"." ws() right:@ {
                Expr::Range {
                    start: Box::new(left),
                    end: Box::new(right),
                    inclusive: false,
                }
            }
            --
            left:(@) ws() "||" ws() right:@ { super::binary(left, BinaryOp::Or, right) }
            --
            left:(@) ws() "&&" ws() right:@ { super::binary(left, BinaryOp::And, right) }
            --
            left:(@) ws() "==" ws() right:@ { super::binary(left, BinaryOp::Eq, right) }
            left:(@) ws() "!=" ws() right:@ { super::binary(left, BinaryOp::Ne, right) }
            left:(@) ws() "<=" ws() right:@ { super::binary(left, BinaryOp::Le, right) }
            left:(@) ws() ">=" ws() right:@ { super::binary(left, BinaryOp::Ge, right) }
            left:(@) ws() "<" !("<" / "$" / "|" / "*") ws() right:@
                { super::binary(left, BinaryOp::Lt, right) }
            left:(@) ws() ">" !(">" / "=") ws() right:@
                { super::binary(left, BinaryOp::Gt, right) }
            --
            left:(@) ws() "|" !("|" / ">") ws() right:@
                { super::binary(left, BinaryOp::BitOr, right) }
            --
            left:(@) ws() "^" ws() right:@ { super::binary(left, BinaryOp::BitXor, right) }
            --
            left:(@) ws() "&" !"&" ws() right:@
                { super::binary(left, BinaryOp::BitAnd, right) }
            --
            left:(@) ws() "<<" ws() right:@ { super::binary(left, BinaryOp::Shl, right) }
            left:(@) ws() ">>" !"=" ws() right:@ { super::binary(left, BinaryOp::Shr, right) }
            --
            left:(@) ws() "++" ws() right:@ { super::binary(left, BinaryOp::Concat, right) }
            --
            value:(@) ws1() "as" ws1() ty:type_expr()
                { Expr::Cast { expr: Box::new(value), ty } }
            --
            left:(@) ws() "+" !"+" ws() right:@ { super::binary(left, BinaryOp::Add, right) }
            left:(@) ws() "-" ws() right:@ { super::binary(left, BinaryOp::Sub, right) }
            --
            left:(@) ws() "*" !">" ws() right:@ { super::binary(left, BinaryOp::Mul, right) }
            left:(@) ws() "/" ws() right:@ { super::binary(left, BinaryOp::Div, right) }
            left:(@) ws() "%" ws() right:@ { super::binary(left, BinaryOp::Mod, right) }
            --
            "!" ws() value:@ { Expr::Not(Box::new(value)) }
            "~" ws() value:@ { Expr::BitNot(Box::new(value)) }
            --
            value:postfix() { value }
        }

        rule postfix() -> Expr
            = value:primary() suffixes:postfix_suffix()*
            {? super::apply_postfix(value, suffixes) }

        rule postfix_suffix() -> Postfix
            = ws() "(" ws() args:(call_arg() ** comma()) ws() ")"
                { Postfix::Call(args) }
            / ws() "." ws() method:identifier() ws() "(" ws()
              args:(call_arg() ** comma()) ws() ")"
                { Postfix::Method(method, args) }
            / ws() "[" ws() index:expression() ws() "]" { Postfix::Index(index) }
            / ws() "." ws() "_" field:$(['1'..='9']['0'..='9']*)
                {? field.parse().map(Postfix::TupleField).or(Err("tuple field")) }
            / ws() "." ws() field:identifier() { Postfix::Field(field) }
            / ws() "?" !"?" { Postfix::ResultOption }
            / ws() "!" !"=" { Postfix::TryResult }

        rule primary() -> Expr
            = do_expr()
            / if_expr()
            / match_expr()
            / "async" ws1() command:shell_command() { Expr::AsyncCommand(command) }
            / "spawn" ws1() command:shell_command() { Expr::AsyncCommand(command) }
            / "await" ws1() name:identifier() { Expr::Await(name) }
            / command:shell_command() { Expr::Command { command, checked: false } }
            / triple_string()
            / raw_string()
            / string()
            / float()
            / integer()
            / "true" !identifier_continue() { Expr::Bool(true) }
            / "false" !identifier_continue() { Expr::Bool(false) }
            / "None" !identifier_continue() { Expr::None }
            / "env" "." name:env_identifier() ws() "??" ws() default:string_value()
                { Expr::EnvDefault { name, default } }
            / "env" "." name:env_identifier() { Expr::Env(name) }
            / "env" "." identifier() {? Err("invalid environment name") }
            / "[" ws() values:(expression() ** comma()) ws() "]" { Expr::Array(values) }
            / "{" ws() entries:(collection_entry() ** comma()) ws() "}"
                {? super::collection(entries) }
            / "(" ws() ")" { Expr::Unit }
            / "(" ws() first:expression() ws() "," ws()
              rest:(expression() ** comma()) ws() ")"
            {
                let mut values = Vec::with_capacity(rest.len() + 1);
                values.push(first);
                values.extend(rest);
                Expr::Tuple(values)
            }
            / "(" ws() value:expression() ws() ")" { value }
            / !language_keyword() !("env" ws() ".") name:identifier() { Expr::Ident(name) }

        rule call_arg() -> Expr
            = name:identifier() ws() "=" !"=" ws() value:expression()
                { Expr::NamedArg { name, value: Box::new(value) } }
            / expression()

        rule do_expr() -> Expr
            = "do" ws() "{" ws() items:(do_item() ** do_separator()) ws() "}"
                {? super::do_expression(items) }

        rule do_separator()
            = hws() ";" ws()
            / hws() "\n"+ ws()

        rule do_item() -> DoItem
            = name:identifier() ws() "<-" ws() value:expression()
                { DoItem::Bind(name, value) }
            / ("const" / "let") ws1() name:identifier()
              annotation:(ws() ":" ws() ty:type_expr() { ty })?
              ws() "=" ws() value:expression()
                { DoItem::Let(name, annotation, value) }
            / value:expression() { DoItem::Expr(value) }

        rule if_expr() -> Expr
            = "if" ws1() condition:expression() ws() "{" ws()
              then_expr:expression() ws() "}" ws() "else" ws()
              else_expr:(if_expr() / ("{" ws() value:expression() ws() "}" { value }))
            {
                Expr::IfElse {
                    condition: Box::new(condition),
                    then_expr: Box::new(then_expr),
                    else_expr: Box::new(else_expr),
                }
            }

        rule match_expr() -> Expr
            = "match" ws1() value:expression() ws() "{" ws()
              arms:(match_arm() ** comma()) ws() "}"
            { Expr::Match { value: Box::new(value), arms } }

        rule match_arm() -> MatchArm
            = pattern:match_pattern()
              guard:(ws1() "if" ws1() value:pipeline() { value })?
              ws() "=>" ws() value:expression()
            {
                MatchArm {
                    pattern,
                    guard,
                    expr: value,
                }
            }

        rule match_pattern() -> Option<Expr>
            = "_" !identifier_continue() { None }
            / value:match_pattern_value()
              alias:(ws1() "as" ws1() name:identifier() { name })?
            {
                Some(match alias {
                    Some(alias) => Expr::AliasPattern {
                        pattern: Box::new(value),
                        alias,
                    },
                    None => value,
                })
            }

        rule match_pattern_value() -> Expr
            = "{" ws() fields:(record_pattern_field() ++ comma()) ws() "}"
                { Expr::RecordPattern(fields) }
            / "[" ws() patterns:(match_pattern_value() ** comma())
              rest:(comma() "..." name:identifier() { name })? ws() "]"
                { Expr::ArrayPattern { patterns, rest } }
            / name:identifier() ws() "(" ws()
              args:(match_pattern_value() ** comma()) ws() ")"
                {? super::match_constructor(name, args) }
            / "(" ws() first:match_pattern_value() ws() "," ws()
              rest:(match_pattern_value() ** comma()) ws() ")"
            {
                let mut values = Vec::with_capacity(rest.len() + 1);
                values.push(first);
                values.extend(rest);
                Expr::Tuple(values)
            }
            / "(" ws() value:match_pattern_value() ws() ")" { value }
            / "_" !identifier_continue() { Expr::Ident("_".into()) }
            / triple_string()
            / raw_string()
            / string()
            / float()
            / integer()
            / "true" !identifier_continue() { Expr::Bool(true) }
            / "false" !identifier_continue() { Expr::Bool(false) }
            / "None" !identifier_continue() { Expr::None }
            / name:identifier() { Expr::Ident(name) }

        rule record_pattern_field() -> (String, Option<Expr>)
            = name:identifier()
              value:(ws() ":" ws() value:match_pattern_value() { value })?
                { (name, value) }

        rule collection_entry() -> CollectionEntry
            = name:identifier() ws() ":" ws() value:expression()
                { CollectionEntry::Record(name, value) }
            / key:expression() ws() ":" ws() value:expression()
                { CollectionEntry::Map(key, value) }

        rule integer() -> Expr
            = value:$("-"? "0x" ['0'..='9' | 'a'..='f' | 'A'..='F']+)
            {? i64::from_str_radix(value.trim_start_matches('-').trim_start_matches("0x"), 16)
                .map(|number| Expr::Int(if value.starts_with('-') { -number } else { number }))
                .or(Err("hex integer")) }
            / value:$("-"? "0b" ['0' | '1']+)
            {? i64::from_str_radix(value.trim_start_matches('-').trim_start_matches("0b"), 2)
                .map(|number| Expr::Int(if value.starts_with('-') { -number } else { number }))
                .or(Err("binary integer")) }
            / value:$("-"? ['0'..='9']+) !("." !"." / identifier_continue())
                {? value.parse().map(Expr::Int).or(Err("integer")) }

        rule float() -> Expr
            = value:$("-"? ['0'..='9']+ "." !"." ['0'..='9']+)
              !identifier_continue() { Expr::Float(value.to_string()) }

        rule triple_string() -> Expr
            = "\"\"\"" value:$((!"\"\"\"" [_])*) "\"\"\""
                {? super::string_expr(value.to_string()).or(Err("string")) }

        rule string() -> Expr
            = value:string_value() {? super::string_expr(value).or(Err("string")) }

        rule string_value() -> String
            = "\"" value:$((("\\" [_]) / (!"\"" [_]))*) "\""
                { super::decode_escaped(value) }
            / "'" value:$((("\\" [_]) / (!"'" [_]))*) "'"
                { super::decode_escaped(value) }

        rule raw_string() -> Expr
            = "r\"" value:$((!"\"" [_])*) "\"" { Expr::RawString(value.to_string()) }
            / "r'" value:$((!"'" [_])*) "'" { Expr::RawString(value.to_string()) }

        rule shell_command() -> String
            = "$sh" ws() value:shell_body() { value }

        rule shell_body() -> String
            = "\"" value:$((("\\" [_]) / (!"\"" [_]))*) "\""
                { super::decode_escaped(value) }
            / "'" value:$((("\\" [_]) / (!"'" [_]))*) "'"
                { super::decode_escaped(value) }
            / "{" value:$(shell_piece()*) "}" { value.trim().to_string() }

        rule shell_piece()
            = shell_quoted()
            / "{" shell_piece()* "}"
            / !"}" [_]

        rule shell_quoted()
            = "\"" (("\\" [_]) / (!"\"" [_]))* "\""
            / "'" (("\\" [_]) / (!"'" [_]))* "'"

        rule env_identifier() -> String
            = value:$(['A'..='Z' | '_']['A'..='Z' | '0'..='9' | '_']*)
                { value.to_string() }

        rule identifier_continue()
            = ['A'..='Z' | 'a'..='z' | '0'..='9' | '_']

        rule ws1() = quiet!{[' ' | '\t' | '\r' | '\n']+}
        rule hws() = quiet!{[' ' | '\t' | '\r']*}
        rule hws1() = quiet!{[' ' | '\t' | '\r']+}

        rule type_expr() -> Type
            = params:function_params() ws() "=>" ws() result:type_expr()
                { Type::Function(params, Box::new(result)) }
            / type_result()

        rule function_params() -> Vec<Type>
            = "(" ws() ")" { Vec::new() }
            / "(" ws() values:(type_expr() ++ comma()) ws() ")"
                {? if values.len() >= 2 { Ok(values) } else { Err("at least two tuple parameters") } }
            / value:type_result() { vec![value] }

        rule type_result() -> Type
            = first:type_union() rest:(ws() "\\/" ws() value:type_union() { value })*
            {
                rest.into_iter().fold(first, |left, right| {
                    Type::Applied("Result".to_string(), vec![left, right])
                })
            }

        rule type_union() -> Type
            = first:type_intersection() rest:(ws() "|" ws() value:type_intersection() { value })*
            { rest.into_iter().fold(first, super::flatten_union) }

        rule type_intersection() -> Type
            = first:type_postfix() rest:(ws() "&" ws() value:type_postfix() { value })*
            { rest.into_iter().fold(first, super::flatten_intersection) }

        rule type_postfix() -> Type
            = value:type_atom() suffixes:(ws() "?" { () })*
            {
                suffixes.into_iter().fold(value, |value, ()| {
                    Type::Applied("Option".to_string(), vec![value])
                })
            }

        rule type_atom() -> Type
            = "[" ws() value:type_expr() ws() "]" { Type::Array(Box::new(value)) }
            / "{" ws() fields:(record_type_field() ** comma()) ws() "}"
                { Type::Record(fields) }
            / "(" ws() values:(type_expr() ++ comma()) ws() ")"
                {? if values.len() >= 2 { Ok(Type::Tuple(values)) } else { Err("at least two tuple elements") } }
            / name:type_name() ws() "[" ws() args:(type_expr() ++ comma()) ws() "]"
                { super::applied_type(name, args) }
            / name:type_name() { super::named_type(name) }

        rule record_type_field() -> (String, Type)
            = name:identifier() ws() ":" ws() value:type_expr() { (name, value) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_type_grammar() {
        assert_eq!(parse_type("String", 1).unwrap(), Type::String);
        assert_eq!(
            parse_type("Map[String, [Int?]]", 1).unwrap(),
            Type::Map(
                Box::new(Type::String),
                Box::new(Type::Array(Box::new(Type::Applied(
                    "Option".into(),
                    vec![Type::Int],
                )))),
            )
        );
        assert_eq!(
            parse_type("(String, Int) => String \\/ CmdError", 1).unwrap(),
            Type::Function(
                vec![Type::String, Type::Int],
                Box::new(Type::Applied(
                    "Result".into(),
                    vec![Type::String, Type::Named("CmdError".into())],
                )),
            )
        );
    }

    #[test]
    fn expression_grammar_parses_representative_syntax() {
        let cases = [
            "1 + 2 * 3",
            "1 - 2",
            "1 / 2",
            "1 % 2",
            "1 != 2",
            "1 <= 2",
            "1 > 2",
            "1 >= 2",
            "1 & 3",
            "1 | 2",
            "1 ^ 3",
            "1 << 2",
            "4 >> 1",
            "(1 + 2) * 3",
            "true || false && true",
            r#""a" ++ "b""#,
            "value as UserId",
            "Some(1)",
            r#"env.HOME ?? "/tmp""#,
            "Ok(1)?",
            "source()!",
            "[1, 2, 3]",
            r#"{ name: "Ada", age: 36 }"#,
            r#"{ "PORT": "8080" }"#,
            r#"("host", 8080)"#,
            "names[0]",
            "pair._2",
            "user.name",
            "names.map(x => x + 1)",
            r#"("nacre").slice(1, 4)"#,
            r#"run.inspect.status("--short")"#,
            "if true { 1 } else { 2 }",
            r#"match code { 0 => "ok", _ => "error" }"#,
            "do {\nvalue <- Some(1)\npure(value)\n}",
            "x => x + 1",
        ];
        for source in cases {
            parse_expr(source, 1)
                .unwrap_or_else(|error| panic!("failed to parse `{source}`: {error}"));
        }
    }

    #[test]
    fn program_grammar_accepts_indented_blocks() {
        parse(
            r#"
fn classify(value: Int): String {
    if value > 0 {
        return "positive"
    } else {
        return "zero"
    }
}

trait Show[T] {
    fn show(value: T): String
}

impl Show[Int] {
    fn show(value: Int): String {
        return classify(value)
    }
}

for value in [1, 0] {
    const label = classify(value)
}
"#,
        )
        .unwrap();
    }

    #[test]
    fn program_grammar_parses_repository_sources() {
        let roots = ["docs/examples", "std"];
        for root in roots {
            for entry in std::fs::read_dir(root).unwrap() {
                let path = entry.unwrap().path();
                if path.extension().and_then(|value| value.to_str()) != Some("ncr") {
                    continue;
                }
                let source = std::fs::read_to_string(&path).unwrap();
                parse(&source)
                    .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()));
            }
        }
    }

    #[test]
    fn documentation_nacre_blocks_parse() {
        for root in ["docs/en/src", "docs/ja/src"] {
            for entry in std::fs::read_dir(root).unwrap() {
                let path = entry.unwrap().path();
                if path.extension().and_then(|value| value.to_str()) != Some("md") {
                    continue;
                }
                let markdown = std::fs::read_to_string(&path).unwrap();
                let mut block = None::<String>;
                let mut block_line = 0;

                for (index, line) in markdown.lines().enumerate() {
                    if line == "```nacre" {
                        assert!(block.is_none(), "nested Nacre fence in {}", path.display());
                        block = Some(String::new());
                        block_line = index + 2;
                    } else if line == "```" && block.is_some() {
                        let source = block.take().unwrap();
                        if !source.contains("{{#include") {
                            parse(&source).unwrap_or_else(|error| {
                                panic!(
                                    "failed to parse Nacre block at {}:{block_line}: {error}",
                                    path.display()
                                )
                            });
                        }
                    } else if let Some(source) = &mut block {
                        source.push_str(line);
                        source.push('\n');
                    }
                }

                assert!(
                    block.is_none(),
                    "unterminated Nacre fence in {}",
                    path.display()
                );
            }
        }
    }
}
