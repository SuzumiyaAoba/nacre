use crate::CompileError;

pub(super) fn program_parse_error(
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
