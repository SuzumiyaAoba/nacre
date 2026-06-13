use std::collections::HashSet;

use crate::{
    BinaryOp, BindingPattern, CompileError, DoStep, Expr, ImplMethod, MatchArm, Param, Program,
    Statement, TraitMethod, Type, TypeParam, VariantDecl,
};

pub fn parse(source: &str) -> Result<Program, CompileError> {
    let mut statements = Vec::new();
    let mut statement_lines = Vec::new();
    let logical_lines = collect_logical_lines(source)?;
    let mut lines = logical_lines
        .iter()
        .map(|(line, source)| (*line, source.as_str()))
        .peekable();

    while let Some((line_no, line)) = lines.next() {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with("##") || trimmed.starts_with("#!") {
            continue;
        }

        if let Some((name, type_params, params, return_type)) =
            parse_external_function(trimmed, line_no)?
        {
            statements.push(Statement::ExternalFunction {
                name,
                type_params,
                params,
                return_type,
            });
            statement_lines.push(line_no);
            continue;
        }

        if trimmed == "{" {
            let (body_source, _) = collect_block(&mut lines, line_no)?;
            statements.push(Statement::Block {
                body: parse(&body_source)?,
            });
            statement_lines.push(line_no);
            continue;
        }

        if trimmed == "raw {" {
            let mut raw = String::new();
            let mut depth = 1usize;
            for (_raw_index, raw_line) in lines.by_ref() {
                match raw_line.trim() {
                    "raw {" => depth += 1,
                    "}" => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                raw.push_str(raw_line);
                raw.push('\n');
            }
            if depth != 0 {
                return Err(CompileError::new(
                    line_no,
                    "unterminated raw block".to_string(),
                ));
            }
            statements.push(Statement::Raw(raw));
            statement_lines.push(line_no);
            continue;
        }

        if let Some((name, override_constructor, type_params, params, return_type)) =
            parse_function_header(trimmed, line_no)?
        {
            let (body_source, _) = collect_block(&mut lines, line_no)?;
            statements.push(Statement::Function {
                name,
                override_constructor,
                type_params,
                params,
                return_type,
                body: parse(&body_source)?,
            });
            statement_lines.push(line_no);
            continue;
        }

        if let Some((name, type_param)) = parse_trait_header(trimmed, line_no)? {
            let (body_source, _) = collect_block(&mut lines, line_no)?;
            statements.push(Statement::Trait {
                name,
                type_param,
                methods: parse_trait_methods(&body_source, line_no)?,
            });
            statement_lines.push(line_no);
            continue;
        }

        if let Some((trait_name, for_type)) = parse_impl_header(trimmed, line_no)? {
            let (body_source, _) = collect_block(&mut lines, line_no)?;
            statements.push(Statement::Impl {
                trait_name,
                for_type,
                methods: parse_impl_methods(&body_source, line_no)?,
            });
            statement_lines.push(line_no);
            continue;
        }

        if let Some(condition) = parse_block_header(trimmed, "if") {
            let (then_source, inline_else) = collect_block(&mut lines, line_no)?;
            let else_branch = parse_else_branch(&mut lines, line_no, inline_else)?;
            statements.push(Statement::If {
                condition: parse_expr(condition.trim(), line_no)?,
                then_branch: parse(&then_source)?,
                else_branch,
            });
            statement_lines.push(line_no);
            continue;
        }

        if let Some(condition) = parse_block_header(trimmed, "while") {
            let (body_source, _) = collect_block(&mut lines, line_no)?;
            statements.push(Statement::While {
                condition: parse_expr(condition.trim(), line_no)?,
                body: parse(&body_source)?,
            });
            statement_lines.push(line_no);
            continue;
        }

        if let Some((name, iterable)) = parse_for_header(trimmed) {
            let (body_source, _) = collect_block(&mut lines, line_no)?;
            statements.push(Statement::For {
                name: parse_name(name.trim(), line_no)?,
                iterable: parse_expr(iterable.trim(), line_no)?,
                body: parse(&body_source)?,
            });
            statement_lines.push(line_no);
            continue;
        }

        statements.push(parse_statement(trimmed, line_no)?);
        statement_lines.push(line_no);
    }

    Ok(Program::new(statements, statement_lines))
}

fn collect_logical_lines(source: &str) -> Result<Vec<(usize, String)>, CompileError> {
    let lines = source.lines().enumerate().collect::<Vec<_>>();
    let mut logical = Vec::new();
    let mut index = 0usize;

    while index < lines.len() {
        let (raw_index, line) = lines[index];
        let line_no = raw_index + 1;
        let stripped = strip_inline_comment(line);
        if has_unterminated_quoted_shell_command(&stripped) {
            let mut joined = line.to_string();
            index += 1;
            while has_unterminated_quoted_shell_command(&joined) && index < lines.len() {
                let (_, next_line) = lines[index];
                joined.push('\n');
                joined.push_str(next_line);
                index += 1;
            }
            if has_unterminated_quoted_shell_command(&joined) {
                return Err(CompileError::new(
                    line_no,
                    "unterminated quoted string in shell command".to_string(),
                ));
            }
            logical.push((line_no, joined));
            continue;
        }
        if stripped.trim_start().starts_with("type ") && stripped.contains('=') {
            let mut joined = stripped;
            index += 1;
            while index < lines.len() {
                let (_, next_line) = lines[index];
                let next = strip_inline_comment(next_line);
                if !next.trim_start().starts_with('|') {
                    break;
                }
                joined.push(' ');
                joined.push_str(next.trim());
                index += 1;
            }
            logical.push((line_no, joined));
            continue;
        }
        let trimmed = stripped.trim_start();
        if trimmed.starts_with("match ")
            || trimmed.starts_with("return match ")
            || trimmed.contains("= match ")
        {
            if let Some(open) = find_last_top_level_char(&stripped, '{') {
                let mut joined = stripped;
                let mut depth = brace_depth(&joined[open..]);
                index += 1;
                while depth > 0 && index < lines.len() {
                    let (_, next_line) = lines[index];
                    let next = strip_inline_comment(next_line);
                    joined.push('\n');
                    joined.push_str(&next);
                    depth += brace_depth(&next);
                    index += 1;
                }
                if depth != 0 {
                    return Err(CompileError::new(
                        line_no,
                        "unterminated match expression".to_string(),
                    ));
                }
                logical.push((line_no, joined));
                continue;
            }
        }
        if let Some(open) = find_do_open(&stripped) {
            let mut joined = stripped;
            let mut depth = brace_depth(&joined[open..]);
            index += 1;
            while depth > 0 && index < lines.len() {
                let (_, next_line) = lines[index];
                let next = strip_inline_comment(next_line);
                joined.push('\n');
                joined.push_str(&next);
                depth += brace_depth(&next);
                index += 1;
            }
            if depth != 0 {
                return Err(CompileError::new(
                    line_no,
                    "unterminated do expression".to_string(),
                ));
            }
            logical.push((line_no, joined));
            continue;
        }
        if stripped.trim() == "raw {" {
            logical.push((line_no, stripped));
            index += 1;
            let mut depth = 1usize;
            while index < lines.len() {
                let (raw_index, raw_line) = lines[index];
                match raw_line.trim() {
                    "raw {" => depth += 1,
                    "}" => depth = depth.saturating_sub(1),
                    _ => {}
                }
                logical.push((raw_index + 1, raw_line.to_string()));
                index += 1;
                if depth == 0 {
                    break;
                }
            }
            continue;
        }

        let Some(start) = line.find("\"\"\"") else {
            if let Some(else_if) = split_inline_else_if(stripped.trim()) {
                logical.push((line_no, "}".to_string()));
                logical.push((line_no, else_if));
                index += 1;
                continue;
            }
            logical.push((line_no, stripped));
            index += 1;
            continue;
        };

        let mut joined = String::new();
        joined.push_str(&line[..start]);
        joined.push('"');
        let rest = &line[start + 3..];
        if let Some(end) = rest.find("\"\"\"") {
            push_quoted_multiline_segment(&mut joined, &rest[..end]);
            joined.push('"');
            joined.push_str(&rest[end + 3..]);
            logical.push((line_no, strip_inline_comment(&joined)));
            index += 1;
            continue;
        }

        push_quoted_multiline_segment(&mut joined, rest);
        index += 1;
        let mut closed = false;
        while index < lines.len() {
            joined.push_str("\\n");
            let (_, next_line) = lines[index];
            if let Some(end) = next_line.find("\"\"\"") {
                push_quoted_multiline_segment(&mut joined, &next_line[..end]);
                joined.push('"');
                joined.push_str(&next_line[end + 3..]);
                joined = strip_inline_comment(&joined);
                index += 1;
                closed = true;
                break;
            }
            push_quoted_multiline_segment(&mut joined, next_line);
            index += 1;
        }
        if !closed {
            return Err(CompileError::new(
                line_no,
                "unterminated multi-line string".to_string(),
            ));
        }
        logical.push((line_no, joined));
    }

    Ok(logical)
}

fn has_unterminated_quoted_shell_command(input: &str) -> bool {
    let mut quote = None;
    let mut escaped = false;
    let mut index = 0usize;
    while index < input.len() {
        let ch = input[index..].chars().next().unwrap();
        let ch_len = ch.len_utf8();
        if escaped {
            escaped = false;
            index += ch_len;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            index += ch_len;
            continue;
        }
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            index += ch_len;
            continue;
        }
        if input[index..].starts_with("$sh\"") || input[index..].starts_with("$sh'") {
            let shell_quote = input[index + 3..].chars().next().unwrap();
            let mut shell_escaped = false;
            index += 4;
            let mut closed = false;
            while index < input.len() {
                let shell_ch = input[index..].chars().next().unwrap();
                let shell_ch_len = shell_ch.len_utf8();
                if shell_escaped {
                    shell_escaped = false;
                    index += shell_ch_len;
                    continue;
                }
                if shell_ch == '\\' {
                    shell_escaped = true;
                    index += shell_ch_len;
                    continue;
                }
                if shell_ch == shell_quote {
                    closed = true;
                    index += shell_ch_len;
                    break;
                }
                index += shell_ch_len;
            }
            if !closed {
                return true;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
        }
        index += ch_len;
    }
    false
}

fn find_do_open(input: &str) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    for (index, ch) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        if input[index..].starts_with("do") {
            let before = input[..index].chars().last();
            let prefix = input[..index].trim_end();
            let rest = &input[index + 2..];
            let after = rest.trim_start();
            if !before.is_some_and(|value| value == '_' || value.is_ascii_alphanumeric())
                && (prefix.is_empty() || prefix == "return" || prefix.ends_with('='))
                && after.starts_with('{')
            {
                return Some(index + 2 + rest.len() - after.len());
            }
        }
    }
    None
}

fn brace_depth(input: &str) -> isize {
    let mut quote = None;
    let mut escaped = false;
    let mut depth = 0isize;
    for ch in input.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        match ch {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
    }
    depth
}

fn split_inline_else_if(input: &str) -> Option<String> {
    let rest = input.strip_prefix("} else if")?.trim_start();
    if rest.strip_suffix('{')?.trim_end().is_empty() {
        return None;
    }
    Some(format!("else if {rest}"))
}

fn push_quoted_multiline_segment(out: &mut String, value: &str) {
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
}

fn strip_inline_comment(input: &str) -> String {
    let mut quote = None;
    let mut escaped = false;
    let mut shell_depth = 0usize;
    let mut index = 0usize;
    while index < input.len() {
        let ch = input[index..].chars().next().unwrap();
        let ch_len = ch.len_utf8();

        if escaped {
            escaped = false;
            index += ch_len;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            index += ch_len;
            continue;
        }
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            index += ch_len;
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            index += ch_len;
            continue;
        }
        if shell_depth == 0 && input[index..].starts_with("$sh{") {
            shell_depth = 1;
            index += "$sh{".len();
            continue;
        }
        if shell_depth > 0 {
            match ch {
                '{' => shell_depth += 1,
                '}' => shell_depth = shell_depth.saturating_sub(1),
                _ => {}
            }
            index += ch_len;
            continue;
        }
        if input[index..].starts_with("##") {
            return input[..index].trim_end().to_string();
        }

        index += ch_len;
    }

    input.to_string()
}

fn parse_block_header<'a>(input: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = input.strip_prefix(keyword)?.trim_start();
    let condition = rest.strip_suffix('{')?.trim_end();
    if condition.is_empty() {
        None
    } else {
        Some(condition)
    }
}

fn parse_for_header(input: &str) -> Option<(&str, &str)> {
    let rest = input.strip_prefix("for ")?.trim_start();
    let rest = rest.strip_suffix('{')?.trim_end();
    let (name, iterable) = rest.split_once(" in ")?;
    Some((name, iterable))
}

fn parse_function_header(
    input: &str,
    line: usize,
) -> Result<Option<(String, bool, Vec<TypeParam>, Vec<Param>, Type)>, CompileError> {
    let (rest, override_constructor) = if let Some(rest) = input.strip_prefix("fn! ") {
        (rest, true)
    } else if let Some(rest) = input.strip_prefix("fn ") {
        (rest, false)
    } else {
        return Ok(None);
    };
    let rest = rest
        .strip_suffix('{')
        .ok_or_else(|| CompileError::new(line, "unterminated function header".to_string()))?
        .trim_end();
    let (name, after_name) = rest
        .split_once('(')
        .ok_or_else(|| CompileError::new(line, "expected function parameters".to_string()))?;
    let (params, after_params) = after_name
        .rsplit_once(')')
        .ok_or_else(|| CompileError::new(line, "expected function parameters".to_string()))?;
    let return_type = after_params
        .trim()
        .strip_prefix(':')
        .ok_or_else(|| CompileError::new(line, "expected function return type".to_string()))?;
    let (name, type_params) = parse_function_name(name.trim(), line)?;
    Ok(Some((
        name,
        override_constructor,
        type_params,
        parse_params(params.trim(), line)?,
        parse_type(return_type.trim(), line)?,
    )))
}

fn parse_external_function(
    input: &str,
    line: usize,
) -> Result<Option<(String, Vec<TypeParam>, Vec<Param>, Type)>, CompileError> {
    let Some(rest) = input.strip_prefix("export fn ") else {
        return Ok(None);
    };
    if rest.trim_end().ends_with('{') {
        return Err(CompileError::new(
            line,
            "external function declarations must not include bodies".to_string(),
        ));
    }
    let (name, after_name) = rest
        .split_once('(')
        .ok_or_else(|| CompileError::new(line, "expected function parameters".to_string()))?;
    let (params, after_params) = after_name
        .rsplit_once(')')
        .ok_or_else(|| CompileError::new(line, "expected function parameters".to_string()))?;
    let return_type = after_params
        .trim()
        .strip_prefix(':')
        .ok_or_else(|| CompileError::new(line, "expected function return type".to_string()))?;
    let (name, type_params) = parse_function_name(name.trim(), line)?;
    Ok(Some((
        name,
        type_params,
        parse_params(params.trim(), line)?,
        parse_type(return_type.trim(), line)?,
    )))
}

fn parse_trait_header(input: &str, line: usize) -> Result<Option<(String, String)>, CompileError> {
    let Some(rest) = input.strip_prefix("trait ") else {
        return Ok(None);
    };
    let rest = rest
        .strip_suffix('{')
        .ok_or_else(|| CompileError::new(line, "unterminated trait header".to_string()))?
        .trim_end();
    let (name, params) = parse_type_head(rest, line)?;
    if params.len() != 1 {
        return Err(CompileError::new(
            line,
            "trait requires exactly one type parameter".to_string(),
        ));
    }
    Ok(Some((name, params[0].clone())))
}

fn parse_impl_header(input: &str, line: usize) -> Result<Option<(String, Type)>, CompileError> {
    let Some(rest) = input.strip_prefix("impl ") else {
        return Ok(None);
    };
    let rest = rest
        .strip_suffix('{')
        .ok_or_else(|| CompileError::new(line, "unterminated impl header".to_string()))?
        .trim_end();
    let Some((name, args)) = parse_type_application(rest, line)? else {
        return Err(CompileError::new(
            line,
            "expected trait implementation".to_string(),
        ));
    };
    if args.len() != 1 {
        return Err(CompileError::new(
            line,
            "trait implementation requires exactly one type".to_string(),
        ));
    }
    Ok(Some((name, args.into_iter().next().unwrap())))
}

fn parse_trait_methods(body: &str, start_line: usize) -> Result<Vec<TraitMethod>, CompileError> {
    let mut methods = Vec::new();
    for (offset, line) in body.lines().enumerate() {
        let line_no = start_line + offset + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("##") {
            continue;
        }
        let Some((name, type_params, params, return_type)) =
            parse_function_signature(trimmed, line_no)?
        else {
            return Err(CompileError::new(
                line_no,
                "trait bodies support method signatures only".to_string(),
            ));
        };
        if !type_params.is_empty() {
            return Err(CompileError::new(
                line_no,
                "trait methods cannot declare type parameters".to_string(),
            ));
        }
        if params.iter().any(|param| param.default.is_some()) {
            return Err(CompileError::new(
                line_no,
                "trait methods cannot declare default parameters".to_string(),
            ));
        }
        methods.push(TraitMethod {
            name,
            params,
            return_type,
        });
    }
    Ok(methods)
}

fn parse_impl_methods(body: &str, start_line: usize) -> Result<Vec<ImplMethod>, CompileError> {
    let program = parse(body)?;
    let mut methods = Vec::new();
    for (statement, line) in program.statements().iter().zip(program.statement_lines()) {
        match statement {
            Statement::Function {
                name,
                override_constructor,
                type_params,
                params,
                return_type,
                body,
            } => {
                if *override_constructor {
                    return Err(CompileError::new(
                        start_line + line,
                        "impl methods cannot override newtype constructors".to_string(),
                    ));
                }
                if !type_params.is_empty() {
                    return Err(CompileError::new(
                        start_line + line,
                        "impl methods cannot declare type parameters".to_string(),
                    ));
                }
                methods.push(ImplMethod {
                    name: name.clone(),
                    params: params.clone(),
                    return_type: return_type.clone(),
                    body: body.clone(),
                });
            }
            _ => {
                return Err(CompileError::new(
                    start_line + line,
                    "impl bodies support method definitions only".to_string(),
                ));
            }
        }
    }
    Ok(methods)
}

fn parse_function_signature(
    input: &str,
    line: usize,
) -> Result<Option<(String, Vec<TypeParam>, Vec<Param>, Type)>, CompileError> {
    let Some(rest) = input.strip_prefix("fn ") else {
        return Ok(None);
    };
    if rest.trim_end().ends_with('{') {
        return Err(CompileError::new(
            line,
            "trait method signatures must not include bodies".to_string(),
        ));
    }
    let (name, after_name) = rest
        .split_once('(')
        .ok_or_else(|| CompileError::new(line, "expected function parameters".to_string()))?;
    let (params, after_params) = after_name
        .rsplit_once(')')
        .ok_or_else(|| CompileError::new(line, "expected function parameters".to_string()))?;
    let return_type = after_params
        .trim()
        .strip_prefix(':')
        .ok_or_else(|| CompileError::new(line, "expected function return type".to_string()))?;
    let (name, type_params) = parse_function_name(name.trim(), line)?;
    Ok(Some((
        name,
        type_params,
        parse_params(params.trim(), line)?,
        parse_type(return_type.trim(), line)?,
    )))
}

fn parse_function_name(input: &str, line: usize) -> Result<(String, Vec<TypeParam>), CompileError> {
    let Some((name, rest)) = input.split_once('[') else {
        return Ok((parse_name(input, line)?, Vec::new()));
    };
    let Some(inner) = rest.strip_suffix(']') else {
        return Err(CompileError::new(
            line,
            "unterminated function type parameters".to_string(),
        ));
    };
    let mut type_params = Vec::new();
    for item in split_comma_separated(inner.trim(), line)? {
        type_params.push(parse_type_param(item.trim(), line)?);
    }
    Ok((parse_name(name.trim(), line)?, type_params))
}

fn parse_type_param(input: &str, line: usize) -> Result<TypeParam, CompileError> {
    let (name, bounds) = if let Some((name, bounds)) = input.split_once(':') {
        let bounds = bounds
            .split('+')
            .map(|bound| parse_type_name(bound.trim(), line))
            .collect::<Result<Vec<_>, _>>()?;
        (name.trim(), bounds)
    } else {
        (input.trim(), Vec::new())
    };
    Ok(TypeParam {
        name: parse_type_name(name, line)?,
        bounds,
    })
}

fn parse_type_head(input: &str, line: usize) -> Result<(String, Vec<String>), CompileError> {
    let Some((name, rest)) = input.split_once('[') else {
        return Err(CompileError::new(
            line,
            "expected type parameters".to_string(),
        ));
    };
    let Some(inner) = rest.strip_suffix(']') else {
        return Err(CompileError::new(
            line,
            "unterminated type parameters".to_string(),
        ));
    };
    let mut params = Vec::new();
    for item in split_comma_separated(inner.trim(), line)? {
        params.push(parse_type_name(item.trim(), line)?);
    }
    Ok((parse_type_name(name.trim(), line)?, params))
}

fn parse_type_alias_name(input: &str, line: usize) -> Result<(String, Vec<String>), CompileError> {
    let Some((name, rest)) = input.split_once('[') else {
        return Ok((parse_type_name(input, line)?, Vec::new()));
    };
    let Some(inner) = rest.strip_suffix(']') else {
        return Err(CompileError::new(
            line,
            "unterminated type parameters".to_string(),
        ));
    };
    let mut type_params = Vec::new();
    for item in split_comma_separated(inner.trim(), line)? {
        type_params.push(parse_type_name(item.trim(), line)?);
    }
    Ok((parse_type_name(name.trim(), line)?, type_params))
}

fn parse_params(input: &str, line: usize) -> Result<Vec<Param>, CompileError> {
    if input.is_empty() {
        return Ok(Vec::new());
    }
    let mut params = Vec::new();
    let items = split_comma_separated(input, line)?;
    for (index, item) in items.iter().enumerate() {
        let (decl, default) = if let Some(index) = find_assignment_equals(item) {
            (
                item[..index].trim(),
                Some(parse_expr(item[index + 1..].trim(), line)?),
            )
        } else {
            (item.trim(), None)
        };
        let Some((name, annotation)) = decl.split_once(':') else {
            return Err(CompileError::new(
                line,
                "function parameter requires type annotation".to_string(),
            ));
        };
        let annotation = annotation.trim();
        let (ty, variadic) = if let Some(element) = annotation.strip_prefix("...") {
            if index + 1 != items.len() {
                return Err(CompileError::new(
                    line,
                    "rest parameter must be last".to_string(),
                ));
            }
            if default.is_some() {
                return Err(CompileError::new(
                    line,
                    "rest parameter cannot have a default".to_string(),
                ));
            }
            (
                Type::Array(Box::new(parse_type(element.trim(), line)?)),
                true,
            )
        } else {
            (parse_type(annotation, line)?, false)
        };
        params.push(Param {
            name: parse_name(name.trim(), line)?,
            ty,
            default,
            variadic,
            capture_name: None,
        });
    }
    Ok(params)
}

fn collect_block<'a, I>(
    lines: &mut std::iter::Peekable<I>,
    start_line: usize,
) -> Result<(String, bool), CompileError>
where
    I: Iterator<Item = (usize, &'a str)>,
{
    let mut body = String::new();
    let mut depth = 1usize;
    for (_index, line) in lines.by_ref() {
        let trimmed = line.trim();
        if depth == 1 && trimmed == "}" {
            return Ok((body, false));
        }
        if depth == 1 && trimmed == "} else {" {
            return Ok((body, true));
        }

        body.push_str(line);
        body.push('\n');

        if trimmed == "} else {" {
            continue;
        }
        if trimmed.ends_with('{') {
            depth += 1;
        }
        if trimmed == "}" {
            depth -= 1;
        }
    }
    Err(CompileError::new(
        start_line,
        "unterminated block".to_string(),
    ))
}

fn parse_else_branch<'a, I>(
    lines: &mut std::iter::Peekable<I>,
    line_no: usize,
    inline_else: bool,
) -> Result<Option<Program>, CompileError>
where
    I: Iterator<Item = (usize, &'a str)>,
{
    if inline_else {
        return Ok(Some(parse(&collect_block(lines, line_no)?.0)?));
    }

    let Some((else_line, next)) = lines.peek().copied() else {
        return Ok(None);
    };
    let trimmed = next.trim();
    if trimmed == "else {" {
        lines.next();
        return Ok(Some(parse(&collect_block(lines, else_line)?.0)?));
    }
    let Some(condition) = parse_block_header(trimmed, "else if") else {
        return Ok(None);
    };

    lines.next();
    let (then_source, inline_else) = collect_block(lines, else_line)?;
    let else_branch = parse_else_branch(lines, else_line, inline_else)?;
    Ok(Some(Program::new(
        vec![Statement::If {
            condition: parse_expr(condition.trim(), else_line)?,
            then_branch: parse(&then_source)?,
            else_branch,
        }],
        vec![else_line],
    )))
}

fn parse_statement(input: &str, line: usize) -> Result<Statement, CompileError> {
    if input == "break" {
        return Ok(Statement::Break);
    }
    if input == "continue" {
        return Ok(Statement::Continue);
    }
    if let Some(expr) = input.strip_prefix("return ") {
        return Ok(Statement::Return(parse_expr(expr.trim(), line)?));
    }

    if let Some(rest) = input.strip_prefix("use ") {
        return Ok(Statement::Use {
            path: parse_module_path(rest.trim(), line)?,
        });
    }

    if let Some(rest) = input.strip_prefix("newtype ") {
        let (name, base) = split_assignment(rest, line)?;
        return Ok(Statement::Newtype {
            name: parse_type_name(name.trim(), line)?,
            base: parse_type(base.trim(), line)?,
        });
    }

    if let Some(rest) = input.strip_prefix("type ") {
        let (name, ty) = split_assignment(rest, line)?;
        let (name, type_params) = parse_type_alias_name(name.trim(), line)?;
        if type_params.is_empty() {
            if let Some(variants) = parse_sum_type_variants(ty.trim(), line)? {
                return Ok(Statement::SumType { name, variants });
            }
        }
        return Ok(Statement::TypeAlias {
            name,
            type_params,
            ty: parse_type(ty.trim(), line)?,
        });
    }

    if let Some(rest) = input.strip_prefix("const ") {
        let (name, expr) = split_assignment(rest, line)?;
        if let Some(pattern) = parse_binding_pattern(name.trim(), line)? {
            return Ok(Statement::Destructure {
                mutable: false,
                pattern,
                expr: parse_expr(expr, line)?,
            });
        }
        let (name, annotation) = split_annotation(name, line)?;
        return Ok(Statement::Const {
            name: parse_name(name, line)?,
            annotation,
            expr: parse_expr(expr, line)?,
        });
    }

    if let Some(rest) = input.strip_prefix("let ") {
        let (name, expr) = split_assignment(rest, line)?;
        if let Some(pattern) = parse_binding_pattern(name.trim(), line)? {
            return Ok(Statement::Destructure {
                mutable: true,
                pattern,
                expr: parse_expr(expr, line)?,
            });
        }
        let (name, annotation) = split_annotation(name, line)?;
        return Ok(Statement::Let {
            name: parse_name(name, line)?,
            annotation,
            expr: parse_expr(expr, line)?,
        });
    }

    if let Some((command, version)) = parse_require(input, line)? {
        return Ok(Statement::Require { command, version });
    }

    if let Some(commands) = parse_require_one_of(input, line)? {
        return Ok(Statement::RequireOneOf { commands });
    }

    if let Some(statement) = parse_redirect(input, line)? {
        return Ok(statement);
    }

    if let Some(rest) = input.strip_prefix("try ") {
        let rest = strip_wrapped_parens(rest.trim());
        if let Some(Expr::Pipeline {
            input: pipeline_input,
            commands,
        }) = parse_pipeline_expr(rest, line)?
        {
            return Ok(Statement::TryPipeline {
                input: pipeline_input,
                commands,
            });
        }
        if let Some(command) = parse_command(rest, "$sh", line)? {
            return Ok(Statement::TryCommand(command));
        }
        return Ok(Statement::TryResult(parse_expr(rest, line)?));
    }

    if strip_top_level_postfix(input, "!").is_some() {
        let expr = parse_expr(input, line)?;
        if let Expr::TryResult(value) = expr {
            return Ok(Statement::TryResult(*value));
        }
        return Ok(Statement::Expr(expr));
    }

    if let Some(command) = parse_pipeline_command(input, line)? {
        return Ok(Statement::Command(command));
    }

    if let Some(command) = parse_command(input, "$sh", line)? {
        return Ok(Statement::Command(command));
    }

    if split_pipeline(input, line)?.is_some() {
        return Ok(Statement::Expr(parse_expr(input, line)?));
    }

    if find_assignment_equals(input).is_some() {
        let (name, expr) = split_assignment(input, line)?;
        return Ok(Statement::Assign {
            name: parse_name(name, line)?,
            expr: parse_expr(expr, line)?,
        });
    }

    if split_call(input).is_some() {
        return Ok(Statement::Expr(parse_expr(input, line)?));
    }

    let (name, expr) = split_assignment(input, line)?;
    Ok(Statement::Assign {
        name: parse_name(name, line)?,
        expr: parse_expr(expr, line)?,
    })
}

fn parse_sum_type_variants(
    input: &str,
    line: usize,
) -> Result<Option<Vec<VariantDecl>>, CompileError> {
    let input = input
        .trim()
        .strip_prefix('|')
        .unwrap_or(input.trim())
        .trim();
    let mut parts = Vec::new();
    let mut rest = input;
    while let Some((left, right)) = split_top_level(rest, "|") {
        parts.push(right.trim());
        rest = left.trim();
    }
    parts.push(rest);
    parts.reverse();
    if parts.len() < 2 {
        return Ok(None);
    }

    let mut variants = Vec::new();
    let mut has_payload = false;
    for part in parts {
        if let Some((name, args)) = split_call(part) {
            has_payload = true;
            variants.push(VariantDecl {
                name: parse_type_name(name.trim(), line)?,
                fields: split_comma_separated(args.trim(), line)?
                    .into_iter()
                    .map(|field| parse_type(field.trim(), line))
                    .collect::<Result<Vec<_>, _>>()?,
            });
        } else {
            variants.push(VariantDecl {
                name: parse_type_name(part.trim(), line)?,
                fields: Vec::new(),
            });
        }
    }
    if !has_payload
        && variants
            .iter()
            .all(|variant| is_builtin_type_name(&variant.name))
    {
        return Ok(None);
    }
    Ok(Some(variants))
}

fn is_builtin_type_name(name: &str) -> bool {
    matches!(
        name,
        "Int" | "Float" | "Bool" | "String" | "Path" | "ExitCode" | "Unit" | "CmdError"
    )
}

fn split_assignment(input: &str, line: usize) -> Result<(&str, &str), CompileError> {
    let Some(index) = find_assignment_equals(input) else {
        return Err(CompileError::new(line, "expected assignment".to_string()));
    };
    Ok((input[..index].trim(), input[index + 1..].trim()))
}

fn parse_binding_pattern(input: &str, line: usize) -> Result<Option<BindingPattern>, CompileError> {
    if let Some(inner) = input
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
    {
        let (names, rest) = parse_array_binding_pattern(inner, line)?;
        if names.is_empty() && rest.is_none() {
            return Err(CompileError::new(
                line,
                "array destructuring requires at least one name".to_string(),
            ));
        }
        return Ok(Some(BindingPattern::Array { names, rest }));
    }

    if let Some(inner) = input
        .strip_prefix('(')
        .and_then(|value| value.strip_suffix(')'))
    {
        let names = parse_binding_pattern_names(inner, line)?;
        if names.len() < 2 {
            return Err(CompileError::new(
                line,
                "tuple destructuring requires at least two names".to_string(),
            ));
        }
        return Ok(Some(BindingPattern::Tuple(names)));
    }

    if let Some(inner) = input
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
    {
        let names = parse_binding_pattern_names(inner, line)?;
        if names.is_empty() {
            return Err(CompileError::new(
                line,
                "record destructuring requires at least one name".to_string(),
            ));
        }
        return Ok(Some(BindingPattern::Record(
            names.into_iter().map(|name| (name.clone(), name)).collect(),
        )));
    }

    Ok(None)
}

fn parse_binding_pattern_names(input: &str, line: usize) -> Result<Vec<String>, CompileError> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(Vec::new());
    }
    split_comma_separated(input, line)?
        .into_iter()
        .map(|name| parse_name(name.trim(), line))
        .collect()
}

fn parse_array_binding_pattern(
    input: &str,
    line: usize,
) -> Result<(Vec<String>, Option<String>), CompileError> {
    let input = input.trim();
    if input.is_empty() {
        return Ok((Vec::new(), None));
    }
    let mut names = Vec::new();
    let mut rest = None;
    let parts = split_comma_separated(input, line)?;
    for (index, part) in parts.iter().enumerate() {
        let part = part.trim();
        if let Some(name) = part.strip_prefix("...") {
            if index + 1 != parts.len() {
                return Err(CompileError::new(
                    line,
                    "array rest destructuring must be last".to_string(),
                ));
            }
            if rest.is_some() {
                return Err(CompileError::new(
                    line,
                    "array destructuring can only include one rest binding".to_string(),
                ));
            }
            rest = Some(parse_name(name.trim(), line)?);
        } else {
            names.push(parse_name(part, line)?);
        }
    }
    Ok((names, rest))
}

fn find_assignment_equals(input: &str) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut brace_depth = 0usize;
    for (index, ch) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        match ch {
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            '=' if bracket_depth == 0 && paren_depth == 0 && brace_depth == 0 => {
                let before = input[..index].trim_end().chars().last();
                let after = input[index + 1..].trim_start().chars().next();
                if after != Some('>') && !matches!(before, Some('=' | '!' | '<' | '>')) {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn split_annotation<'a>(
    input: &'a str,
    line: usize,
) -> Result<(&'a str, Option<Type>), CompileError> {
    let Some((name, ty)) = input.split_once(':') else {
        return Ok((input.trim(), None));
    };
    Ok((name.trim(), Some(parse_type(ty.trim(), line)?)))
}

fn parse_type(input: &str, line: usize) -> Result<Type, CompileError> {
    if let Some(index) = find_top_level_arrow(input) {
        let params = input[..index].trim();
        let return_type = input[index + 2..].trim();
        if params.is_empty() || return_type.is_empty() {
            return Err(CompileError::new(
                line,
                "expected function parameter and return types".to_string(),
            ));
        }
        return Ok(Type::Function(
            parse_function_type_params(params, line)?,
            Box::new(parse_type(return_type, line)?),
        ));
    }

    if let Some((left, right)) = split_top_level_type_operator(input, "\\/") {
        return Ok(Type::Applied(
            "Result".to_string(),
            vec![
                parse_type(left.trim(), line)?,
                parse_type(right.trim(), line)?,
            ],
        ));
    }

    if let Some((left, right)) = split_top_level_type_operator(input, "|") {
        return Ok(flatten_type_operator(
            TypeOperator::Union,
            parse_type(left.trim(), line)?,
            parse_type(right.trim(), line)?,
        ));
    }

    if let Some((left, right)) = split_top_level_type_operator(input, "&") {
        return Ok(flatten_type_operator(
            TypeOperator::Intersection,
            parse_type(left.trim(), line)?,
            parse_type(right.trim(), line)?,
        ));
    }

    if let Some(inner) = input.strip_suffix('?') {
        let inner = inner.trim();
        if inner.is_empty() {
            return Err(CompileError::new(line, "expected Option type".to_string()));
        }
        return Ok(Type::Applied(
            "Option".to_string(),
            vec![parse_type(inner, line)?],
        ));
    }

    if let Some(inner) = input
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
    {
        let inner = inner.trim();
        if inner.is_empty() {
            return Ok(Type::Record(Vec::new()));
        }
        let mut fields = Vec::new();
        for item in split_comma_separated(inner, line)? {
            let (name, ty) = item
                .split_once(':')
                .ok_or_else(|| CompileError::new(line, "expected record field type".to_string()))?;
            fields.push((parse_name(name.trim(), line)?, parse_type(ty.trim(), line)?));
        }
        return Ok(Type::Record(fields));
    }

    if let Some(inner) = input
        .strip_prefix("Map[")
        .and_then(|value| value.strip_suffix(']'))
    {
        let items = split_comma_separated(inner.trim(), line)?;
        if items.len() != 2 {
            return Err(CompileError::new(
                line,
                "Map type requires key and value types".to_string(),
            ));
        }
        return Ok(Type::Map(
            Box::new(parse_type(items[0].trim(), line)?),
            Box::new(parse_type(items[1].trim(), line)?),
        ));
    }

    if let Some((name, args)) = parse_type_application(input, line)? {
        return Ok(Type::Applied(name, args));
    }

    if let Some(inner) = input
        .strip_prefix('(')
        .and_then(|value| value.strip_suffix(')'))
    {
        let items = split_comma_separated(inner.trim(), line)?;
        if items.len() < 2 {
            return Err(CompileError::new(
                line,
                "tuple type requires at least two elements".to_string(),
            ));
        }
        let mut elements = Vec::new();
        for item in items {
            elements.push(parse_type(item.trim(), line)?);
        }
        return Ok(Type::Tuple(elements));
    }

    if let Some(inner) = input
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
    {
        return Ok(Type::Array(Box::new(parse_type(inner.trim(), line)?)));
    }

    match input {
        "Int" => Ok(Type::Int),
        "Float" => Ok(Type::Float),
        "Bool" => Ok(Type::Bool),
        "String" => Ok(Type::String),
        "Path" => Ok(Type::Path),
        "ExitCode" => Ok(Type::ExitCode),
        "Unit" => Ok(Type::Unit),
        _ => Ok(Type::Named(parse_type_name(input, line)?)),
    }
}

#[derive(Clone, Copy)]
enum TypeOperator {
    Union,
    Intersection,
}

fn flatten_type_operator(operator: TypeOperator, left: Type, right: Type) -> Type {
    match operator {
        TypeOperator::Union => {
            let mut types = match left {
                Type::Union(types) => types,
                other => vec![other],
            };
            match right {
                Type::Union(right_types) => types.extend(right_types),
                other => types.push(other),
            }
            Type::Union(types)
        }
        TypeOperator::Intersection => {
            let mut types = match left {
                Type::Intersection(types) => types,
                other => vec![other],
            };
            match right {
                Type::Intersection(right_types) => types.extend(right_types),
                other => types.push(other),
            }
            Type::Intersection(types)
        }
    }
}

fn parse_type_application(
    input: &str,
    line: usize,
) -> Result<Option<(String, Vec<Type>)>, CompileError> {
    let Some((name, rest)) = input.split_once('[') else {
        return Ok(None);
    };
    if name.trim().is_empty() {
        return Ok(None);
    }
    let Some(inner) = rest.strip_suffix(']') else {
        return Ok(None);
    };
    let mut args = Vec::new();
    for item in split_comma_separated(inner.trim(), line)? {
        args.push(parse_type(item.trim(), line)?);
    }
    Ok(Some((parse_type_name(name.trim(), line)?, args)))
}

fn parse_function_type_params(input: &str, line: usize) -> Result<Vec<Type>, CompileError> {
    if let Some(inner) = input
        .strip_prefix('(')
        .and_then(|value| value.strip_suffix(')'))
    {
        let inner = inner.trim();
        if inner.is_empty() {
            return Ok(Vec::new());
        }
        return split_comma_separated(inner, line)?
            .into_iter()
            .map(|item| parse_type(item.trim(), line))
            .collect();
    }
    Ok(vec![parse_type(input, line)?])
}

fn parse_type_name(input: &str, line: usize) -> Result<String, CompileError> {
    if input.contains('.') {
        let mut parts = Vec::new();
        let mut iter = input.split('.').peekable();
        while let Some(part) = iter.next() {
            let part = part.trim();
            if iter.peek().is_some() {
                parts.push(parse_name(part, line)?);
            } else {
                parts.push(parse_unqualified_type_name(part, line)?);
            }
        }
        return Ok(parts.join("."));
    }
    parse_unqualified_type_name(input, line)
}

fn parse_unqualified_type_name(input: &str, line: usize) -> Result<String, CompileError> {
    let mut chars = input.chars();
    let first = chars
        .next()
        .ok_or_else(|| CompileError::new(line, "expected type name".to_string()))?;
    let valid_first = first.is_ascii_uppercase();
    let valid_rest = chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric());
    if valid_first && valid_rest {
        Ok(input.to_string())
    } else {
        Err(CompileError::new(
            line,
            format!("invalid type name `{input}`"),
        ))
    }
}

fn is_newtype_constructor_name(input: &str) -> bool {
    let last = input.rsplit('.').next().unwrap_or(input);
    last.chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
}

fn parse_name(input: &str, line: usize) -> Result<String, CompileError> {
    let mut chars = input.chars();
    let first = chars
        .next()
        .ok_or_else(|| CompileError::new(line, "expected variable name".to_string()))?;
    let valid_first = first == '_' || first.is_ascii_alphabetic();
    let valid_rest = chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric());
    if valid_first && valid_rest {
        Ok(input.to_string())
    } else {
        Err(CompileError::new(
            line,
            format!("invalid variable name `{input}`"),
        ))
    }
}

fn parse_qualified_name(input: &str, line: usize) -> Result<String, CompileError> {
    let parts = parse_module_path(input, line)?;
    Ok(parts.join("."))
}

fn parse_module_path(input: &str, line: usize) -> Result<Vec<String>, CompileError> {
    if input.trim().is_empty() {
        return Err(CompileError::new(line, "expected module path".to_string()));
    }
    let mut parts = Vec::new();
    for part in input.split('.') {
        parts.push(parse_name(part.trim(), line)?);
    }
    Ok(parts)
}

fn parse_command(input: &str, prefix: &str, line: usize) -> Result<Option<String>, CompileError> {
    let Some(rest) = input.strip_prefix(prefix) else {
        return Ok(None);
    };
    parse_shell_command(rest.trim(), line).map(Some)
}

fn parse_builtin_string_call(
    input: &str,
    function: &str,
    line: usize,
) -> Result<Option<String>, CompileError> {
    let Some(rest) = input.strip_prefix(function) else {
        return Ok(None);
    };
    let Some(args) = strip_exact_parens(rest.trim()) else {
        return Ok(None);
    };
    parse_quoted(args.trim(), line).map(Some)
}

struct RedirectTarget {
    target: String,
    stderr: Option<String>,
}

fn parse_redirect_target(
    input: &str,
    function: &str,
    line: usize,
) -> Result<Option<RedirectTarget>, CompileError> {
    let Some(rest) = input.strip_prefix(function) else {
        return Ok(None);
    };
    let rest = rest.trim();
    let Some(args) = rest
        .strip_prefix('(')
        .and_then(|value| value.strip_suffix(')'))
    else {
        return Ok(None);
    };
    let parts = split_comma_separated(args.trim(), line)?;
    if parts.is_empty() || parts.len() > 2 {
        return Err(CompileError::new(
            line,
            format!("{function} expects a target string and optional stderr"),
        ));
    }
    let target = parse_quoted(parts[0].trim(), line)?;
    let stderr = if let Some(part) = parts.get(1) {
        let Some(index) = find_assignment_equals(part) else {
            return Err(CompileError::new(
                line,
                "redirect stderr must use `stderr = \"...\"`".to_string(),
            ));
        };
        let name = part[..index].trim();
        if name != "stderr" {
            return Err(CompileError::new(
                line,
                "redirect optional argument must be `stderr`".to_string(),
            ));
        }
        Some(parse_quoted(part[index + 1..].trim(), line)?)
    } else {
        None
    };
    Ok(Some(RedirectTarget { target, stderr }))
}

fn parse_require(
    input: &str,
    line: usize,
) -> Result<Option<(String, Option<String>)>, CompileError> {
    let Some(rest) = input.strip_prefix("require") else {
        return Ok(None);
    };
    let rest = rest.trim();
    let Some(args) = rest
        .strip_prefix('(')
        .and_then(|value| value.strip_suffix(')'))
    else {
        return Ok(None);
    };
    let args = args.trim();
    if args.is_empty() {
        return Err(CompileError::new(
            line,
            "require expects a command string".to_string(),
        ));
    }
    let parts = split_comma_separated(args, line)?;
    if parts.len() > 2 {
        return Err(CompileError::new(
            line,
            "require expects a command string and optional version".to_string(),
        ));
    }
    let command = parse_quoted(parts[0].trim(), line)?;
    let version = if let Some(version) = parts.get(1) {
        let Some(index) = find_assignment_equals(version) else {
            return Err(CompileError::new(
                line,
                "require version must use `version = \"...\"`".to_string(),
            ));
        };
        let name = &version[..index];
        let value = &version[index + 1..];
        if name.trim() != "version" {
            return Err(CompileError::new(
                line,
                "require optional argument must be `version`".to_string(),
            ));
        }
        Some(parse_quoted(value.trim(), line)?)
    } else {
        None
    };
    Ok(Some((command, version)))
}

fn parse_builtin_expr_call(
    input: &str,
    function: &str,
    line: usize,
) -> Result<Option<Expr>, CompileError> {
    let Some(rest) = input.strip_prefix(function) else {
        return Ok(None);
    };
    let Some(args) = strip_exact_parens(rest.trim()) else {
        return Ok(None);
    };
    let args = parse_call_args(args.trim(), line)?;
    if args.len() != 1 {
        return Err(CompileError::new(
            line,
            format!("{function} expects one argument"),
        ));
    }
    Ok(Some(args.into_iter().next().unwrap()))
}

fn parse_require_one_of(input: &str, line: usize) -> Result<Option<Vec<String>>, CompileError> {
    let Some(rest) = input.strip_prefix("requireOneOf") else {
        return Ok(None);
    };
    let rest = rest.trim();
    let Some(args) = rest
        .strip_prefix('(')
        .and_then(|value| value.strip_suffix(')'))
    else {
        return Ok(None);
    };
    let Expr::Array(values) = parse_expr(args.trim(), line)? else {
        return Err(CompileError::new(
            line,
            "requireOneOf expects an array of quoted strings".to_string(),
        ));
    };
    let mut commands = Vec::new();
    for value in values {
        match value {
            Expr::String(command) | Expr::RawString(command) => commands.push(command),
            _ => {
                return Err(CompileError::new(
                    line,
                    "requireOneOf expects an array of quoted strings".to_string(),
                ));
            }
        }
    }
    if commands.is_empty() {
        return Err(CompileError::new(
            line,
            "requireOneOf expects at least one command".to_string(),
        ));
    }
    Ok(Some(commands))
}

fn parse_redirect(input: &str, line: usize) -> Result<Option<Statement>, CompileError> {
    let Some((left, right)) = split_redirect(input, line)? else {
        return Ok(None);
    };
    let Some(command) =
        parse_pipeline_command(left.trim(), line)?.or(parse_command(left.trim(), "$sh", line)?)
    else {
        return Err(CompileError::new(
            line,
            "redirect source must be a `$sh` command or pipeline".to_string(),
        ));
    };
    if let Some(target) = parse_redirect_target(right.trim(), "write", line)? {
        return Ok(Some(Statement::Redirect {
            command,
            target: target.target,
            stderr: target.stderr,
            append: false,
        }));
    }
    if let Some(target) = parse_redirect_target(right.trim(), "append", line)? {
        return Ok(Some(Statement::Redirect {
            command,
            target: target.target,
            stderr: target.stderr,
            append: true,
        }));
    }
    Err(CompileError::new(
        line,
        "redirect target must be `write(...)` or `append(...)`".to_string(),
    ))
}

pub(crate) fn parse_expr(input: &str, line: usize) -> Result<Expr, CompileError> {
    if let Some(expr) = parse_do_expr(input, line)? {
        return Ok(expr);
    }
    if let Some(expr) = parse_try_pipeline_expr(input, line)? {
        return Ok(expr);
    }
    if let Some(expr) = parse_try_result_expr(input, line)? {
        return Ok(expr);
    }
    if let Some(expr) = parse_pipeline_expr(input, line)? {
        return Ok(expr);
    }
    if let Some(expr) = parse_if_expr(input, line)? {
        return Ok(expr);
    }
    if let Some(expr) = parse_match_expr(input, line)? {
        return Ok(expr);
    }
    if let Some(expr) = parse_lambda_expr(input, line)? {
        return Ok(expr);
    }
    parse_default(input, line)
}

fn parse_do_expr(input: &str, line: usize) -> Result<Option<Expr>, CompileError> {
    let Some(rest) = input.strip_prefix("do") else {
        return Ok(None);
    };
    let rest = rest.trim_start();
    if !rest.starts_with('{') {
        return Ok(None);
    }
    let (body, after) = take_braced_expr(rest, line)?;
    if !after.trim().is_empty() {
        return Err(CompileError::new(
            line,
            "unexpected text after do expression".to_string(),
        ));
    }
    let items = split_do_items(body, line)?;
    if items.is_empty() {
        return Err(CompileError::new(
            line,
            "do expression requires a result expression".to_string(),
        ));
    }
    let mut steps = Vec::new();
    for item in &items[..items.len() - 1] {
        let item = item.trim();
        if let Some((name, expr)) = split_top_level(item, "<-") {
            steps.push(DoStep::Bind {
                name: parse_name(name.trim(), line)?,
                expr: parse_expr(expr.trim(), line)?,
            });
            continue;
        }
        let rest = if let Some(rest) = item.strip_prefix("const ") {
            rest
        } else if let Some(rest) = item.strip_prefix("let ") {
            rest
        } else {
            return Err(CompileError::new(
                line,
                "do steps must use `<-`, `const`, or `let`".to_string(),
            ));
        };
        let (name, expr) = split_assignment(rest, line)?;
        let (name, annotation) = split_annotation(name, line)?;
        steps.push(DoStep::Let {
            name: parse_name(name, line)?,
            annotation,
            expr: parse_expr(expr, line)?,
        });
    }
    let result = items.last().unwrap().trim();
    if split_top_level(result, "<-").is_some()
        || result.starts_with("const ")
        || result.starts_with("let ")
    {
        return Err(CompileError::new(
            line,
            "do expression must end with a result expression".to_string(),
        ));
    }
    Ok(Some(Expr::Do {
        steps,
        result: Box::new(parse_expr(result, line)?),
    }))
}

fn split_do_items<'a>(input: &'a str, line: usize) -> Result<Vec<&'a str>, CompileError> {
    let mut items = Vec::new();
    let mut start = 0usize;
    let mut quote = None;
    let mut escaped = false;
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut brace_depth = 0usize;
    for (index, ch) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        match ch {
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            '\n' if bracket_depth == 0 && paren_depth == 0 && brace_depth == 0 => {
                let item = input[start..index].trim();
                if !item.is_empty() {
                    items.push(item);
                }
                start = index + 1;
            }
            _ => {}
        }
    }
    if quote.is_some() || bracket_depth != 0 || paren_depth != 0 || brace_depth != 0 {
        return Err(CompileError::new(
            line,
            "unterminated expression in do block".to_string(),
        ));
    }
    let item = input[start..].trim();
    if !item.is_empty() {
        items.push(item);
    }
    Ok(items)
}

fn parse_lambda_expr(input: &str, line: usize) -> Result<Option<Expr>, CompileError> {
    let Some(index) = find_top_level_arrow(input) else {
        return Ok(None);
    };
    let params = input[..index].trim();
    let body = input[index + 2..].trim();
    if params.is_empty() || body.is_empty() {
        return Err(CompileError::new(
            line,
            "lambda requires parameters and a body".to_string(),
        ));
    }
    let params = if params.starts_with('(') {
        let Some(inner) = params
            .strip_prefix('(')
            .and_then(|params| params.strip_suffix(')'))
        else {
            return Err(CompileError::new(
                line,
                "unterminated lambda parameter list".to_string(),
            ));
        };
        if inner.trim().is_empty() {
            Vec::new()
        } else {
            split_comma_separated(inner.trim(), line)?
                .into_iter()
                .map(|param| parse_name(param.trim(), line))
                .collect::<Result<Vec<_>, _>>()?
        }
    } else {
        vec![parse_name(params, line)?]
    };
    let mut seen = HashSet::new();
    for param in &params {
        if !seen.insert(param) {
            return Err(CompileError::new(
                line,
                format!("lambda parameter `{param}` is already defined"),
            ));
        }
    }
    Ok(Some(Expr::Lambda {
        params,
        body: Box::new(parse_expr(body, line)?),
    }))
}

fn parse_default(input: &str, line: usize) -> Result<Expr, CompileError> {
    if let Some((left, right)) = split_top_level(input, "??") {
        let left = left.trim();
        let right = right.trim();
        if let Some(name) = left.strip_prefix("env.") {
            match parse_quoted(right, line) {
                Ok(default) => {
                    return Ok(Expr::EnvDefault {
                        name: parse_env_name(name.trim(), line)?,
                        default,
                    });
                }
                Err(error)
                    if error
                        .message()
                        .contains("unexpected text after quoted string") =>
                {
                    return parse_logical_or(input, line);
                }
                Err(error) => return Err(error),
            }
        }
        return Ok(Expr::Default {
            value: Box::new(parse_default(left, line)?),
            fallback: Box::new(parse_logical_or(right, line)?),
        });
    }
    parse_or_else(input, line)
}

fn parse_or_else(input: &str, line: usize) -> Result<Expr, CompileError> {
    if let Some((left, right)) = split_top_level(input, "<|>") {
        return Ok(Expr::OptionOrElseValue {
            value: Box::new(parse_or_else(left.trim(), line)?),
            fallback: Box::new(parse_flat_map_operator(right.trim(), line)?),
        });
    }
    parse_flat_map_operator(input, line)
}

fn parse_flat_map_operator(input: &str, line: usize) -> Result<Expr, CompileError> {
    if let Some((left, right)) = split_top_level(input, ">>=") {
        let value = parse_flat_map_operator(left.trim(), line)?;
        let mapper = Box::new(parse_ap_operator(right.trim(), line)?);
        return Ok(match value {
            Expr::Ident(name) => Expr::OptionFlatMap { name, mapper },
            value => Expr::OptionFlatMapValue {
                value: Box::new(value),
                mapper,
            },
        });
    }
    parse_ap_operator(input, line)
}

fn parse_ap_operator(input: &str, line: usize) -> Result<Expr, CompileError> {
    if let Some((left, right)) = split_top_level(input, "<*>") {
        let function = parse_ap_operator(left.trim(), line)?;
        let value = Box::new(parse_map_operator(right.trim(), line)?);
        return Ok(match function {
            Expr::Ident(name) => Expr::OptionAp { name, value },
            function => Expr::OptionApValue {
                function: Box::new(function),
                value,
            },
        });
    }
    parse_map_operator(input, line)
}

fn parse_map_operator(input: &str, line: usize) -> Result<Expr, CompileError> {
    if let Some((left, right)) = split_top_level(input, "<$>") {
        let value = parse_map_operator(left.trim(), line)?;
        let mapper = Box::new(parse_logical_or(right.trim(), line)?);
        return Ok(match value {
            Expr::Ident(name) => Expr::ArrayMap { name, mapper },
            value => Expr::ArrayMapValue {
                value: Box::new(value),
                mapper,
            },
        });
    }
    parse_logical_or(input, line)
}

fn parse_if_expr(input: &str, line: usize) -> Result<Option<Expr>, CompileError> {
    let Some(rest) = input.strip_prefix("if ") else {
        return Ok(None);
    };
    let Some(open_then) = find_top_level_char(rest, '{') else {
        return Err(CompileError::new(
            line,
            "expected `{` in if expression".to_string(),
        ));
    };
    let condition = rest[..open_then].trim();
    let (then_body, after_then) = take_braced_expr(&rest[open_then..], line)?;
    let after_then = after_then.trim_start();
    let Some(after_else) = after_then.strip_prefix("else") else {
        return Err(CompileError::new(
            line,
            "if expression requires else branch".to_string(),
        ));
    };
    let after_else = after_else.trim_start();
    if after_else.starts_with("if ") {
        let Some(else_expr) = parse_if_expr(after_else, line)? else {
            unreachable!("after_else starts with `if `");
        };
        return Ok(Some(Expr::IfElse {
            condition: Box::new(parse_expr(condition, line)?),
            then_expr: Box::new(parse_expr(then_body.trim(), line)?),
            else_expr: Box::new(else_expr),
        }));
    }
    if !after_else.starts_with('{') {
        return Err(CompileError::new(
            line,
            "expected `{` in else branch".to_string(),
        ));
    }
    let (else_body, after_else) = take_braced_expr(after_else, line)?;
    if !after_else.trim().is_empty() {
        return Err(CompileError::new(
            line,
            "unexpected text after if expression".to_string(),
        ));
    }
    Ok(Some(Expr::IfElse {
        condition: Box::new(parse_expr(condition, line)?),
        then_expr: Box::new(parse_expr(then_body.trim(), line)?),
        else_expr: Box::new(parse_expr(else_body.trim(), line)?),
    }))
}

fn take_braced_expr(input: &str, line: usize) -> Result<(&str, &str), CompileError> {
    debug_assert!(input.starts_with('{'));
    let mut quote = None;
    let mut escaped = false;
    let mut depth = 0usize;
    let mut end_index = None;
    for (index, ch) in input.char_indices() {
        if end_index.is_some() {
            continue;
        }
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end_index = Some(index);
                }
            }
            _ => {}
        }
    }
    if let Some(index) = end_index {
        Ok((&input[1..index], &input[index + 1..]))
    } else {
        Err(CompileError::new(
            line,
            "unterminated if expression".to_string(),
        ))
    }
}

fn find_top_level_char(input: &str, needle: char) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;
    for (index, ch) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        if ch == needle && bracket_depth == 0 && paren_depth == 0 {
            return Some(index);
        }
        match ch {
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            _ => {}
        }
    }
    None
}

fn find_last_top_level_char(input: &str, needle: char) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut found = None;
    for (index, ch) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        if ch == needle && bracket_depth == 0 && paren_depth == 0 && brace_depth == 0 {
            found = Some(index);
        }
        match ch {
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            _ => {}
        }
    }
    found
}

fn parse_match_expr(input: &str, line: usize) -> Result<Option<Expr>, CompileError> {
    let Some(rest) = input.strip_prefix("match ") else {
        return Ok(None);
    };
    let Some(open_body) = find_last_top_level_char(rest, '{') else {
        return Err(CompileError::new(
            line,
            "expected `{` in match expression".to_string(),
        ));
    };
    let value = rest[..open_body].trim();
    let (body, after_body) = take_braced_expr(&rest[open_body..], line)?;
    if !after_body.trim().is_empty() {
        return Err(CompileError::new(
            line,
            "unexpected text after match expression".to_string(),
        ));
    }
    let mut arms = Vec::new();
    for item in split_comma_separated(body.trim(), line)? {
        let (pattern, expr) = split_match_arm(item, line)?;
        let (pattern, guard) = split_match_guard(pattern);
        let pattern = if pattern == "_" {
            None
        } else {
            Some(parse_match_pattern(pattern, line)?)
        };
        arms.push(MatchArm {
            pattern,
            guard: guard.map(|guard| parse_expr(guard, line)).transpose()?,
            expr: parse_expr(expr, line)?,
        });
    }
    Ok(Some(Expr::Match {
        value: Box::new(parse_expr(value, line)?),
        arms,
    }))
}

fn parse_match_pattern(input: &str, line: usize) -> Result<Expr, CompileError> {
    if input.starts_with('{') {
        return parse_record_match_pattern(input, line);
    }
    if let Some(value) = parse_builtin_match_pattern_call(input, "Some", line)? {
        return Ok(Expr::Some(Box::new(value)));
    }
    if let Some(value) = parse_builtin_match_pattern_call(input, "Ok", line)? {
        return Ok(Expr::Ok(Box::new(value)));
    }
    if let Some(value) = parse_builtin_match_pattern_call(input, "Err", line)? {
        return Ok(Expr::Err(Box::new(value)));
    }
    parse_expr(input, line)
}

fn parse_builtin_match_pattern_call(
    input: &str,
    expected: &str,
    line: usize,
) -> Result<Option<Expr>, CompileError> {
    let Some((name, args)) = split_call(input) else {
        return Ok(None);
    };
    if name.trim() != expected {
        return Ok(None);
    }
    let args = split_comma_separated(args.trim(), line)?;
    if args.len() != 1 {
        return Err(CompileError::new(
            line,
            format!("{expected} expects exactly one pattern"),
        ));
    }
    Ok(Some(parse_match_pattern(args[0].trim(), line)?))
}

fn parse_record_match_pattern(input: &str, line: usize) -> Result<Expr, CompileError> {
    let Some(inner) = input
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
    else {
        return Err(CompileError::new(
            line,
            "unterminated record match pattern".to_string(),
        ));
    };
    let inner = inner.trim();
    if inner.is_empty() {
        return Err(CompileError::new(
            line,
            "record match pattern requires at least one field".to_string(),
        ));
    }
    let mut fields = Vec::new();
    for item in split_comma_separated(inner, line)? {
        if let Some((name, value)) = item.split_once(':') {
            fields.push((
                parse_name(name.trim(), line)?,
                Some(parse_expr(value.trim(), line)?),
            ));
        } else {
            fields.push((parse_name(item.trim(), line)?, None));
        }
    }
    Ok(Expr::RecordPattern(fields))
}

fn split_match_arm<'a>(input: &'a str, line: usize) -> Result<(&'a str, &'a str), CompileError> {
    let Some(index) = find_top_level_arrow(input) else {
        return Err(CompileError::new(
            line,
            "expected `=>` in match arm".to_string(),
        ));
    };
    let pattern = input[..index].trim();
    let expr = input[index + 2..].trim();
    if pattern.is_empty() || expr.is_empty() {
        return Err(CompileError::new(
            line,
            "expected match pattern and expression".to_string(),
        ));
    }
    Ok((pattern, expr))
}

fn split_match_guard(input: &str) -> (&str, Option<&str>) {
    if let Some((pattern, guard)) = split_top_level_keyword(input, "if") {
        (pattern.trim(), Some(guard.trim()))
    } else {
        (input, None)
    }
}

fn find_top_level_arrow(input: &str) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut index = 0usize;
    while index < input.len() {
        let ch = input[index..].chars().next().unwrap();
        let ch_len = ch.len_utf8();
        if escaped {
            escaped = false;
            index += ch_len;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            index += ch_len;
            continue;
        }
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            index += ch_len;
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            index += ch_len;
            continue;
        }
        if input[index..].starts_with("=>")
            && bracket_depth == 0
            && paren_depth == 0
            && brace_depth == 0
        {
            return Some(index);
        }
        match ch {
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            _ => {}
        }
        index += ch_len;
    }
    None
}

fn parse_pipeline_command(input: &str, line: usize) -> Result<Option<String>, CompileError> {
    let Some(parts) = split_pipeline(input, line)? else {
        return Ok(None);
    };
    let mut commands = Vec::new();
    for (index, part) in parts.iter().enumerate() {
        let Some(command) = parse_command(part.trim(), "$sh", line)? else {
            if index == 0 {
                return Ok(None);
            }
            return Err(CompileError::new(
                line,
                "pipeline stages must be `$sh` commands".to_string(),
            ));
        };
        commands.push(command);
    }
    Ok(Some(commands.join(" | ")))
}

fn parse_pipeline_expr(input: &str, line: usize) -> Result<Option<Expr>, CompileError> {
    let Some(parts) = split_pipeline(input, line)? else {
        return Ok(None);
    };
    let first = parts[0].trim();
    let (input, mut commands) = if let Some(command) = parse_command(first, "$sh", line)? {
        (None, vec![command])
    } else {
        (Some(Box::new(parse_expr(first, line)?)), Vec::new())
    };
    for part in parts.iter().skip(1) {
        let Some(command) = parse_command(part.trim(), "$sh", line)? else {
            return Err(CompileError::new(
                line,
                "pipeline stages must be `$sh` commands".to_string(),
            ));
        };
        commands.push(command);
    }
    Ok(Some(Expr::Pipeline { input, commands }))
}

fn parse_try_pipeline_expr(input: &str, line: usize) -> Result<Option<Expr>, CompileError> {
    let Some(rest) = input.strip_prefix("try ") else {
        return Ok(None);
    };
    let rest = strip_wrapped_parens(rest.trim());
    let Some(Expr::Pipeline { input, commands }) = parse_pipeline_expr(rest, line)? else {
        return Ok(None);
    };
    Ok(Some(Expr::TryPipeline { input, commands }))
}

fn parse_try_result_expr(input: &str, line: usize) -> Result<Option<Expr>, CompileError> {
    let Some(rest) = input.strip_prefix("try ") else {
        return Ok(None);
    };
    let rest = rest.trim();
    if rest.starts_with("$sh") {
        return Ok(None);
    }
    Ok(Some(Expr::TryResult(Box::new(parse_expr(
        strip_wrapped_parens(rest),
        line,
    )?))))
}

fn strip_wrapped_parens(input: &str) -> &str {
    strip_exact_parens(input).map(str::trim).unwrap_or(input)
}

fn strip_exact_parens(input: &str) -> Option<&str> {
    if !input.starts_with('(') {
        return None;
    }
    let mut quote = None;
    let mut escaped = false;
    let mut depth = 0usize;
    for (index, ch) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return (index + ch.len_utf8() == input.len()).then(|| &input[1..index]);
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_logical_or(input: &str, line: usize) -> Result<Expr, CompileError> {
    if let Some((left, right)) = split_top_level(input, "||") {
        return Ok(Expr::Binary {
            left: Box::new(parse_logical_or(left.trim(), line)?),
            op: BinaryOp::Or,
            right: Box::new(parse_logical_and(right.trim(), line)?),
        });
    }
    parse_logical_and(input, line)
}

fn parse_logical_and(input: &str, line: usize) -> Result<Expr, CompileError> {
    if let Some((left, right)) = split_top_level(input, "&&") {
        return Ok(Expr::Binary {
            left: Box::new(parse_logical_and(left.trim(), line)?),
            op: BinaryOp::And,
            right: Box::new(parse_comparison(right.trim(), line)?),
        });
    }
    parse_comparison(input, line)
}

fn parse_comparison(input: &str, line: usize) -> Result<Expr, CompileError> {
    for (op_text, op) in [
        ("==", BinaryOp::Eq),
        ("!=", BinaryOp::Ne),
        ("<=", BinaryOp::Le),
        (">=", BinaryOp::Ge),
        ("<", BinaryOp::Lt),
        (">", BinaryOp::Gt),
    ] {
        if let Some((left, right)) = split_top_level(input, op_text) {
            return Ok(Expr::Binary {
                left: Box::new(parse_bit_or(left.trim(), line)?),
                op,
                right: Box::new(parse_bit_or(right.trim(), line)?),
            });
        }
    }
    parse_bit_or(input, line)
}

fn parse_bit_or(input: &str, line: usize) -> Result<Expr, CompileError> {
    if let Some((left, right)) = split_top_level(input, "|") {
        return Ok(Expr::Binary {
            left: Box::new(parse_bit_or(left.trim(), line)?),
            op: BinaryOp::BitOr,
            right: Box::new(parse_bit_xor(right.trim(), line)?),
        });
    }
    parse_bit_xor(input, line)
}

fn parse_bit_xor(input: &str, line: usize) -> Result<Expr, CompileError> {
    if let Some((left, right)) = split_top_level(input, "^") {
        return Ok(Expr::Binary {
            left: Box::new(parse_bit_xor(left.trim(), line)?),
            op: BinaryOp::BitXor,
            right: Box::new(parse_bit_and(right.trim(), line)?),
        });
    }
    parse_bit_and(input, line)
}

fn parse_bit_and(input: &str, line: usize) -> Result<Expr, CompileError> {
    if let Some((left, right)) = split_top_level(input, "&") {
        return Ok(Expr::Binary {
            left: Box::new(parse_bit_and(left.trim(), line)?),
            op: BinaryOp::BitAnd,
            right: Box::new(parse_shift(right.trim(), line)?),
        });
    }
    parse_shift(input, line)
}

fn parse_shift(input: &str, line: usize) -> Result<Expr, CompileError> {
    if let Some((left, op, right)) =
        split_top_level_any(input, &[("<<", BinaryOp::Shl), (">>", BinaryOp::Shr)])
    {
        return Ok(Expr::Binary {
            left: Box::new(parse_shift(left.trim(), line)?),
            op,
            right: Box::new(parse_concat(right.trim(), line)?),
        });
    }
    parse_concat(input, line)
}

fn parse_concat(input: &str, line: usize) -> Result<Expr, CompileError> {
    if let Some((left, right)) = split_top_level(input, "++") {
        return Ok(Expr::Binary {
            left: Box::new(parse_concat(left.trim(), line)?),
            op: BinaryOp::Concat,
            right: Box::new(parse_cast(right.trim(), line)?),
        });
    }
    parse_cast(input, line)
}

fn parse_cast(input: &str, line: usize) -> Result<Expr, CompileError> {
    if let Some((expr, ty)) = split_top_level_keyword(input, "as") {
        return Ok(Expr::Cast {
            expr: Box::new(parse_cast(expr.trim(), line)?),
            ty: parse_type(ty.trim(), line)?,
        });
    }
    parse_unary(input, line)
}

fn parse_unary(input: &str, line: usize) -> Result<Expr, CompileError> {
    if let Some(rest) = input.strip_prefix('!') {
        return Ok(Expr::Not(Box::new(parse_unary(rest.trim(), line)?)));
    }
    if let Some(rest) = input.strip_prefix('~') {
        return Ok(Expr::BitNot(Box::new(parse_unary(rest.trim(), line)?)));
    }
    parse_arithmetic(input, line)
}

fn parse_arithmetic(input: &str, line: usize) -> Result<Expr, CompileError> {
    if let Some((left, op, right)) =
        split_top_level_any(input, &[("+", BinaryOp::Add), ("-", BinaryOp::Sub)])
    {
        return Ok(Expr::Binary {
            left: Box::new(parse_arithmetic(left.trim(), line)?),
            op,
            right: Box::new(parse_term(right.trim(), line)?),
        });
    }
    parse_term(input, line)
}

fn parse_term(input: &str, line: usize) -> Result<Expr, CompileError> {
    if let Some((left, op, right)) = split_top_level_any(
        input,
        &[
            ("*", BinaryOp::Mul),
            ("/", BinaryOp::Div),
            ("%", BinaryOp::Mod),
        ],
    ) {
        return Ok(Expr::Binary {
            left: Box::new(parse_term(left.trim(), line)?),
            op,
            right: Box::new(parse_postfix(right.trim(), line)?),
        });
    }
    parse_postfix(input, line)
}

fn parse_postfix(input: &str, line: usize) -> Result<Expr, CompileError> {
    if let Some(value) = strip_top_level_postfix(input, "?") {
        return Ok(Expr::ResultOption(Box::new(parse_postfix(value, line)?)));
    }
    if let Some(value) = strip_top_level_postfix(input, "!") {
        return Ok(Expr::TryResult(Box::new(parse_postfix(value, line)?)));
    }
    parse_atom(input, line)
}

fn parse_atom(input: &str, line: usize) -> Result<Expr, CompileError> {
    if let Some(rest) = input.strip_prefix("async $sh") {
        return Ok(Expr::AsyncCommand(parse_shell_command(rest.trim(), line)?));
    }

    if let Some(rest) = input.strip_prefix("spawn $sh") {
        return Ok(Expr::AsyncCommand(parse_shell_command(rest.trim(), line)?));
    }

    if let Some(name) = input.strip_prefix("await ") {
        return Ok(Expr::Await(parse_name(name.trim(), line)?));
    }

    if let Some(rest) = input.strip_prefix("try $sh") {
        if let Some(expr) = parse_shell_string_suffix_expr(rest.trim(), true, line)? {
            return Ok(expr);
        }
        return Ok(Expr::Command {
            command: parse_shell_command(rest.trim(), line)?,
            checked: true,
        });
    }

    if let Some(rest) = input.strip_prefix("$sh") {
        if let Some(expr) = parse_shell_string_suffix_expr(rest.trim(), false, line)? {
            return Ok(expr);
        }
        return Ok(Expr::Command {
            command: parse_shell_command(rest.trim(), line)?,
            checked: false,
        });
    }

    if let Some(command) = parse_builtin_string_call(input, "hasCommand", line)? {
        return Ok(Expr::HasCommand(command));
    }

    if let Some(arg) = parse_builtin_expr_call(input, "pathExists", line)? {
        return Ok(Expr::PathExists(Box::new(arg)));
    }

    if input == "process.args()" {
        return Ok(Expr::ProcessArgs);
    }

    if input == "cli.parse()" {
        return Ok(Expr::CliParse);
    }

    if input == "()" {
        return Ok(Expr::Unit);
    }

    if let Some(value) = parse_builtin_expr_call(input, "Some", line)? {
        return Ok(Expr::Some(Box::new(value)));
    }

    if input == "None" {
        return Ok(Expr::None);
    }

    if let Some(value) = parse_builtin_expr_call(input, "Ok", line)? {
        return Ok(Expr::Ok(Box::new(value)));
    }

    if let Some(value) = parse_builtin_expr_call(input, "Err", line)? {
        return Ok(Expr::Err(Box::new(value)));
    }

    if let Some((receiver, args)) = split_top_level_method_call(input, "split") {
        let args = parse_call_args(args.trim(), line)?;
        if args.len() == 1 {
            let separator = Box::new(args.into_iter().next().unwrap());
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::StringSplit { name, separator });
            }
            return Ok(Expr::StringSplitValue {
                value: Box::new(parse_expr(strip_wrapped_parens(receiver.trim()), line)?),
                separator,
            });
        }
    }

    if let Some((receiver, args)) = split_top_level_method_call(input, "join") {
        let args = parse_call_args(args.trim(), line)?;
        if args.len() == 1 {
            let separator = Box::new(args.into_iter().next().unwrap());
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::Join { name, separator });
            }
            return Ok(Expr::JoinValue {
                value: Box::new(parse_expr(strip_wrapped_parens(receiver.trim()), line)?),
                separator,
            });
        }
    }

    if let Some((receiver, args)) = split_top_level_method_call(input, "first") {
        let args = parse_call_args(args.trim(), line)?;
        if args.is_empty() {
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::ArrayFirst(name));
            }
            return Ok(Expr::ArrayFirstValue(Box::new(parse_expr(
                strip_wrapped_parens(receiver.trim()),
                line,
            )?)));
        }
    }

    if let Some((receiver, args)) = split_top_level_method_call(input, "last") {
        let args = parse_call_args(args.trim(), line)?;
        if args.is_empty() {
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::ArrayLast(name));
            }
            return Ok(Expr::ArrayLastValue(Box::new(parse_expr(
                strip_wrapped_parens(receiver.trim()),
                line,
            )?)));
        }
    }

    if let Some((receiver, args)) = split_top_level_method_call(input, "reverse") {
        let args = parse_call_args(args.trim(), line)?;
        if args.is_empty() {
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::ArrayReverse(name));
            }
            return Ok(Expr::ArrayReverseValue(Box::new(parse_expr(
                strip_wrapped_parens(receiver.trim()),
                line,
            )?)));
        }
    }

    if let Some((receiver, args)) = split_top_level_method_call(input, "map") {
        let args = parse_call_args(args.trim(), line)?;
        if args.len() == 1 {
            let mapper = Box::new(args.into_iter().next().unwrap());
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::ArrayMap { name, mapper });
            }
            return Ok(Expr::ArrayMapValue {
                value: Box::new(parse_expr(strip_wrapped_parens(receiver.trim()), line)?),
                mapper,
            });
        }
    }

    if let Some((receiver, args)) = split_top_level_method_call(input, "flatMap") {
        let args = parse_call_args(args.trim(), line)?;
        if args.len() == 1 {
            let mapper = Box::new(args.into_iter().next().unwrap());
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::OptionFlatMap { name, mapper });
            }
            return Ok(Expr::OptionFlatMapValue {
                value: Box::new(parse_expr(strip_wrapped_parens(receiver.trim()), line)?),
                mapper,
            });
        }
    }

    if let Some((receiver, args)) = split_top_level_method_call(input, "ap") {
        let args = parse_call_args(args.trim(), line)?;
        if args.len() == 1 {
            let value = Box::new(args.into_iter().next().unwrap());
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::OptionAp { name, value });
            }
            return Ok(Expr::OptionApValue {
                function: Box::new(parse_expr(strip_wrapped_parens(receiver.trim()), line)?),
                value,
            });
        }
    }

    if let Some((receiver, args)) = split_top_level_method_call(input, "orElse") {
        let args = parse_call_args(args.trim(), line)?;
        if args.len() == 1 {
            let fallback = Box::new(args.into_iter().next().unwrap());
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::OptionOrElse { name, fallback });
            }
            return Ok(Expr::OptionOrElseValue {
                value: Box::new(parse_expr(strip_wrapped_parens(receiver.trim()), line)?),
                fallback,
            });
        }
    }

    if let Some((receiver, args)) = split_top_level_method_call(input, "sort") {
        let args = parse_call_args(args.trim(), line)?;
        if args.is_empty() {
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::ArraySort(name));
            }
            return Ok(Expr::ArraySortValue(Box::new(parse_expr(
                strip_wrapped_parens(receiver.trim()),
                line,
            )?)));
        }
    }

    if let Some((receiver, args)) = split_top_level_method_call(input, "unique") {
        let args = parse_call_args(args.trim(), line)?;
        if args.is_empty() {
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::ArrayUnique(name));
            }
            return Ok(Expr::ArrayUniqueValue(Box::new(parse_expr(
                strip_wrapped_parens(receiver.trim()),
                line,
            )?)));
        }
    }

    if let Some((receiver, args)) = split_top_level_method_call(input, "contains") {
        let args = parse_call_args(args.trim(), line)?;
        if args.len() == 1 {
            let needle = args.into_iter().next().unwrap();
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::StringContains {
                    name,
                    needle: Box::new(needle),
                });
            }
            let value = parse_expr(strip_wrapped_parens(receiver.trim()), line)?;
            if matches!(value, Expr::Array(_)) {
                return Ok(Expr::ArrayContainsValue {
                    value: Box::new(value),
                    item: Box::new(needle),
                });
            }
            return Ok(Expr::StringContainsValue {
                value: Box::new(value),
                needle: Box::new(needle),
            });
        }
    }
    if let Some((receiver, args)) = split_top_level_method_call(input, "indexOf") {
        let args = parse_call_args(args.trim(), line)?;
        if args.len() == 1 {
            let needle = args.into_iter().next().unwrap();
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::ArrayIndexOf {
                    name,
                    value: Box::new(needle),
                });
            }
            let value = parse_expr(strip_wrapped_parens(receiver.trim()), line)?;
            if matches!(value, Expr::Array(_)) {
                return Ok(Expr::ArrayIndexOfValue {
                    value: Box::new(value),
                    item: Box::new(needle),
                });
            }
            return Ok(Expr::StringIndexOfValue {
                value: Box::new(value),
                needle: Box::new(needle),
            });
        }
    }
    if let Some((receiver, args)) = split_top_level_method_call(input, "startsWith") {
        let args = parse_call_args(args.trim(), line)?;
        if args.len() == 1 {
            let prefix = Box::new(args.into_iter().next().unwrap());
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::StringStartsWith { name, prefix });
            }
            return Ok(Expr::StringStartsWithValue {
                value: Box::new(parse_expr(strip_wrapped_parens(receiver.trim()), line)?),
                prefix,
            });
        }
    }
    if let Some((receiver, args)) = split_top_level_method_call(input, "endsWith") {
        let args = parse_call_args(args.trim(), line)?;
        if args.len() == 1 {
            let suffix = Box::new(args.into_iter().next().unwrap());
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::StringEndsWith { name, suffix });
            }
            return Ok(Expr::StringEndsWithValue {
                value: Box::new(parse_expr(strip_wrapped_parens(receiver.trim()), line)?),
                suffix,
            });
        }
    }
    if let Some((receiver, args)) = split_top_level_method_call(input, "len") {
        let args = parse_call_args(args.trim(), line)?;
        if args.is_empty() {
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::Len(name));
            }
            let value = parse_expr(strip_wrapped_parens(receiver.trim()), line)?;
            if matches!(value, Expr::Array(_)) {
                return Ok(Expr::ArrayLenValue(Box::new(value)));
            }
            if matches!(value, Expr::Map(_)) {
                return Ok(Expr::MapLenValue(Box::new(value)));
            }
            return Ok(Expr::StringLenValue(Box::new(value)));
        }
    }
    if let Some((receiver, args)) = split_top_level_method_call(input, "isEmpty") {
        let args = parse_call_args(args.trim(), line)?;
        if args.is_empty() {
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::IsEmpty(name));
            }
            let value = parse_expr(strip_wrapped_parens(receiver.trim()), line)?;
            if matches!(value, Expr::Array(_)) {
                return Ok(Expr::ArrayIsEmptyValue(Box::new(value)));
            }
            if matches!(value, Expr::Map(_)) {
                return Ok(Expr::MapIsEmptyValue(Box::new(value)));
            }
            return Ok(Expr::StringIsEmptyValue(Box::new(value)));
        }
    }

    if let Some((receiver, args)) = split_top_level_method_call(input, "keys") {
        let args = parse_call_args(args.trim(), line)?;
        if args.is_empty() {
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::MapKeys(name));
            }
            return Ok(Expr::MapKeysValue(Box::new(parse_expr(
                strip_wrapped_parens(receiver.trim()),
                line,
            )?)));
        }
    }

    if let Some((receiver, args)) = split_top_level_method_call(input, "values") {
        let args = parse_call_args(args.trim(), line)?;
        if args.is_empty() {
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::MapValues(name));
            }
            return Ok(Expr::MapValuesValue(Box::new(parse_expr(
                strip_wrapped_parens(receiver.trim()),
                line,
            )?)));
        }
    }

    if let Some((receiver, args)) = split_top_level_method_call(input, "has") {
        let args = parse_call_args(args.trim(), line)?;
        if args.len() == 1 {
            let key = Box::new(args.into_iter().next().unwrap());
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::MapHas { name, key });
            }
            return Ok(Expr::MapHasValue {
                value: Box::new(parse_expr(strip_wrapped_parens(receiver.trim()), line)?),
                key,
            });
        }
    }

    if let Some((receiver, args)) = split_top_level_method_call(input, "slice") {
        let args = parse_call_args(args.trim(), line)?;
        if args.len() == 2 {
            let mut args = args.into_iter();
            let start = args.next().unwrap();
            let end = args.next().unwrap();
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::Slice {
                    name,
                    start: Box::new(start),
                    end: Box::new(end),
                });
            }
            let value = parse_expr(strip_wrapped_parens(receiver.trim()), line)?;
            if matches!(value, Expr::Array(_)) {
                return Ok(Expr::ArraySliceValue {
                    value: Box::new(value),
                    start: Box::new(start),
                    end: Box::new(end),
                });
            }
            return Ok(Expr::StringSliceValue {
                value: Box::new(value),
                start: Box::new(start),
                end: Box::new(end),
            });
        }
    }

    if let Some((receiver, args)) = split_top_level_method_call(input, "take") {
        let args = parse_call_args(args.trim(), line)?;
        if args.len() == 1 {
            let count = Box::new(args.into_iter().next().unwrap());
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::ArrayTake { name, count });
            }
            return Ok(Expr::ArrayTakeValue {
                value: Box::new(parse_expr(strip_wrapped_parens(receiver.trim()), line)?),
                count,
            });
        }
    }

    if let Some((receiver, args)) = split_top_level_method_call(input, "drop") {
        let args = parse_call_args(args.trim(), line)?;
        if args.len() == 1 {
            let count = Box::new(args.into_iter().next().unwrap());
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::ArrayDrop { name, count });
            }
            return Ok(Expr::ArrayDropValue {
                value: Box::new(parse_expr(strip_wrapped_parens(receiver.trim()), line)?),
                count,
            });
        }
    }

    if let Some((receiver, args)) = split_top_level_method_call(input, "trim") {
        let args = parse_call_args(args.trim(), line)?;
        if args.is_empty() {
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::StringTrim(name));
            }
            return Ok(Expr::StringTrimValue(Box::new(parse_expr(
                strip_wrapped_parens(receiver.trim()),
                line,
            )?)));
        }
    }
    if let Some((receiver, args)) = split_top_level_method_call(input, "trimStart") {
        let args = parse_call_args(args.trim(), line)?;
        if args.is_empty() {
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::StringTrimStart(name));
            }
            return Ok(Expr::StringTrimStartValue(Box::new(parse_expr(
                strip_wrapped_parens(receiver.trim()),
                line,
            )?)));
        }
    }
    if let Some((receiver, args)) = split_top_level_method_call(input, "trimEnd") {
        let args = parse_call_args(args.trim(), line)?;
        if args.is_empty() {
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::StringTrimEnd(name));
            }
            return Ok(Expr::StringTrimEndValue(Box::new(parse_expr(
                strip_wrapped_parens(receiver.trim()),
                line,
            )?)));
        }
    }
    if let Some((receiver, args)) = split_top_level_method_call(input, "toUpper") {
        let args = parse_call_args(args.trim(), line)?;
        if args.is_empty() {
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::StringToUpper(name));
            }
            return Ok(Expr::StringToUpperValue(Box::new(parse_expr(
                strip_wrapped_parens(receiver.trim()),
                line,
            )?)));
        }
    }
    if let Some((receiver, args)) = split_top_level_method_call(input, "toLower") {
        let args = parse_call_args(args.trim(), line)?;
        if args.is_empty() {
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::StringToLower(name));
            }
            return Ok(Expr::StringToLowerValue(Box::new(parse_expr(
                strip_wrapped_parens(receiver.trim()),
                line,
            )?)));
        }
    }

    if let Some((receiver, args)) = split_top_level_method_call(input, "repeat") {
        let args = parse_call_args(args.trim(), line)?;
        if args.len() == 1 {
            let count = Box::new(args.into_iter().next().unwrap());
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::StringRepeat { name, count });
            }
            return Ok(Expr::StringRepeatValue {
                value: Box::new(parse_expr(strip_wrapped_parens(receiver.trim()), line)?),
                count,
            });
        }
    }

    if let Some((receiver, args)) = split_top_level_method_call(input, "isAbsolute") {
        let args = parse_call_args(args.trim(), line)?;
        if args.is_empty() {
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::PathIsAbsolute(name));
            }
            return Ok(Expr::PathIsAbsoluteValue(Box::new(parse_expr(
                strip_wrapped_parens(receiver.trim()),
                line,
            )?)));
        }
    }
    if let Some((receiver, args)) = split_top_level_method_call(input, "basename") {
        let args = parse_call_args(args.trim(), line)?;
        if args.is_empty() {
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::PathBasename(name));
            }
            return Ok(Expr::PathBasenameValue(Box::new(parse_expr(
                strip_wrapped_parens(receiver.trim()),
                line,
            )?)));
        }
    }
    if let Some((receiver, args)) = split_top_level_method_call(input, "dirname") {
        let args = parse_call_args(args.trim(), line)?;
        if args.is_empty() {
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::PathDirname(name));
            }
            return Ok(Expr::PathDirnameValue(Box::new(parse_expr(
                strip_wrapped_parens(receiver.trim()),
                line,
            )?)));
        }
    }
    if let Some((receiver, args)) = split_top_level_method_call(input, "stem") {
        let args = parse_call_args(args.trim(), line)?;
        if args.is_empty() {
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::PathStem(name));
            }
            return Ok(Expr::PathStemValue(Box::new(parse_expr(
                strip_wrapped_parens(receiver.trim()),
                line,
            )?)));
        }
    }
    if let Some((receiver, args)) = split_top_level_method_call(input, "extname") {
        let args = parse_call_args(args.trim(), line)?;
        if args.is_empty() {
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::PathExtname(name));
            }
            return Ok(Expr::PathExtnameValue(Box::new(parse_expr(
                strip_wrapped_parens(receiver.trim()),
                line,
            )?)));
        }
    }

    if let Some((receiver, args)) = split_top_level_method_call(input, "replace") {
        let args = parse_call_args(args.trim(), line)?;
        if args.len() == 2 {
            let mut args = args.into_iter();
            let from = Box::new(args.next().unwrap());
            let to = Box::new(args.next().unwrap());
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::StringReplace { name, from, to });
            }
            return Ok(Expr::StringReplaceValue {
                value: Box::new(parse_expr(strip_wrapped_parens(receiver.trim()), line)?),
                from,
                to,
            });
        }
    }

    if let Some((name, index)) = split_index(input) {
        let index = Box::new(parse_expr(index.trim(), line)?);
        if let Ok(name) = parse_name(name.trim(), line) {
            return Ok(Expr::Index { name, index });
        }
        return Ok(Expr::IndexValue {
            value: Box::new(parse_expr(strip_wrapped_parens(name.trim()), line)?),
            index,
        });
    }

    if let Some((name, field)) = split_tuple_field(input) {
        if let Ok(name) = parse_name(name.trim(), line) {
            return Ok(Expr::TupleField { name, field });
        }
        return Ok(Expr::TupleFieldValue {
            value: Box::new(parse_expr(name.trim(), line)?),
            field,
        });
    }

    if let Some((name, field)) = split_field(input) {
        if parse_name(name.trim(), line).is_err() {
            return Ok(Expr::FieldValue {
                value: Box::new(parse_expr(name.trim(), line)?),
                field: parse_name(field.trim(), line)?,
            });
        }
    }

    if input.starts_with('(') {
        return parse_parenthesized_or_tuple(input, line);
    }

    if input.starts_with('{') {
        return parse_map_or_record(input, line);
    }

    if input.starts_with('[') {
        return parse_array(input, line);
    }

    if let Some(rest) = input.strip_prefix("0x") {
        return i64::from_str_radix(rest, 16)
            .map(Expr::Int)
            .map_err(|_| CompileError::new(line, format!("invalid integer literal `{input}`")));
    }

    if let Some(rest) = input.strip_prefix("0b") {
        return i64::from_str_radix(rest, 2)
            .map(Expr::Int)
            .map_err(|_| CompileError::new(line, format!("invalid integer literal `{input}`")));
    }

    if let Ok(value) = input.parse::<i64>() {
        return Ok(Expr::Int(value));
    }

    if is_float_literal(input) {
        return Ok(Expr::Float(input.to_string()));
    }

    if input == "true" {
        return Ok(Expr::Bool(true));
    }
    if input == "false" {
        return Ok(Expr::Bool(false));
    }

    if let Some(rest) = input.strip_prefix("env.") {
        if let Some((name, default)) = rest.split_once("??") {
            return Ok(Expr::EnvDefault {
                name: parse_env_name(name.trim(), line)?,
                default: parse_quoted(default.trim(), line)?,
            });
        }
        return Ok(Expr::Env(parse_env_name(rest.trim(), line)?));
    }

    if input.starts_with("r\"") || input.starts_with("r'") {
        let value = parse_raw_quoted(input.strip_prefix('r').unwrap(), line)?;
        return Ok(Expr::RawString(value));
    }

    if input.starts_with('"') || input.starts_with('\'') {
        return Ok(Expr::String(parse_quoted(input, line)?));
    }

    if let Some(name) = input.strip_suffix(".len()") {
        return Ok(Expr::Len(parse_name(name.trim(), line)?));
    }

    if let Some(name) = input.strip_suffix(".isEmpty()") {
        return Ok(Expr::IsEmpty(parse_name(name.trim(), line)?));
    }

    if let Some(name) = input.strip_suffix(".first()") {
        return Ok(Expr::ArrayFirst(parse_name(name.trim(), line)?));
    }

    if let Some(name) = input.strip_suffix(".last()") {
        return Ok(Expr::ArrayLast(parse_name(name.trim(), line)?));
    }

    if let Some(name) = input.strip_suffix(".reverse()") {
        return Ok(Expr::ArrayReverse(parse_name(name.trim(), line)?));
    }

    if let Some(name) = input.strip_suffix(".sort()") {
        return Ok(Expr::ArraySort(parse_name(name.trim(), line)?));
    }

    if let Some(name) = input.strip_suffix(".unique()") {
        return Ok(Expr::ArrayUnique(parse_name(name.trim(), line)?));
    }

    if let Some(name) = input.strip_suffix(".wait()") {
        return Ok(Expr::Await(parse_name(name.trim(), line)?));
    }

    if let Some(name) = input.strip_suffix(".pop()") {
        return Ok(Expr::ArrayPop {
            name: parse_name(name.trim(), line)?,
        });
    }

    if let Some(name) = input.strip_suffix(".keys()") {
        return Ok(Expr::MapKeys(parse_name(name.trim(), line)?));
    }

    if let Some(name) = input.strip_suffix(".values()") {
        return Ok(Expr::MapValues(parse_name(name.trim(), line)?));
    }

    if let Some(name) = input.strip_suffix(".trim()") {
        return Ok(Expr::StringTrim(parse_name(name.trim(), line)?));
    }

    if let Some(name) = input.strip_suffix(".trimStart()") {
        return Ok(Expr::StringTrimStart(parse_name(name.trim(), line)?));
    }

    if let Some(name) = input.strip_suffix(".trimEnd()") {
        return Ok(Expr::StringTrimEnd(parse_name(name.trim(), line)?));
    }

    if let Some(name) = input.strip_suffix(".toUpper()") {
        return Ok(Expr::StringToUpper(parse_name(name.trim(), line)?));
    }

    if let Some(name) = input.strip_suffix(".toLower()") {
        return Ok(Expr::StringToLower(parse_name(name.trim(), line)?));
    }

    if let Some(name) = input.strip_suffix(".isAbsolute()") {
        return Ok(Expr::PathIsAbsolute(parse_name(name.trim(), line)?));
    }

    if let Some(name) = input.strip_suffix(".basename()") {
        return Ok(Expr::PathBasename(parse_name(name.trim(), line)?));
    }

    if let Some(name) = input.strip_suffix(".dirname()") {
        return Ok(Expr::PathDirname(parse_name(name.trim(), line)?));
    }

    if let Some(name) = input.strip_suffix(".stem()") {
        return Ok(Expr::PathStem(parse_name(name.trim(), line)?));
    }

    if let Some(name) = input.strip_suffix(".extname()") {
        return Ok(Expr::PathExtname(parse_name(name.trim(), line)?));
    }

    if let Some(name) = input.strip_suffix(".value") {
        return Ok(Expr::Value(parse_name(name.trim(), line)?));
    }

    if let Some((name, args)) = split_call(input) {
        let name = name.trim();
        let args = parse_call_args(args.trim(), line)?;
        if let Some(receiver) = name.strip_suffix(".join") {
            if args.len() == 1 {
                let separator = Box::new(args.into_iter().next().unwrap());
                if let Ok(name) = parse_name(receiver.trim(), line) {
                    return Ok(Expr::Join { name, separator });
                }
                return Ok(Expr::JoinValue {
                    value: Box::new(parse_expr(strip_wrapped_parens(receiver.trim()), line)?),
                    separator,
                });
            }
        }
        if let Some(receiver) = name.strip_suffix(".slice") {
            if args.len() == 2 {
                let mut args = args.into_iter();
                return Ok(Expr::Slice {
                    name: parse_name(receiver.trim(), line)?,
                    start: Box::new(args.next().unwrap()),
                    end: Box::new(args.next().unwrap()),
                });
            }
        }
        if let Some(receiver) = name.strip_suffix(".take") {
            if args.len() == 1 {
                return Ok(Expr::ArrayTake {
                    name: parse_name(receiver.trim(), line)?,
                    count: Box::new(args.into_iter().next().unwrap()),
                });
            }
        }
        if let Some(receiver) = name.strip_suffix(".drop") {
            if args.len() == 1 {
                return Ok(Expr::ArrayDrop {
                    name: parse_name(receiver.trim(), line)?,
                    count: Box::new(args.into_iter().next().unwrap()),
                });
            }
        }
        if let Some(receiver) = name.strip_suffix(".push") {
            if args.len() == 1 {
                return Ok(Expr::ArrayPush {
                    name: parse_name(receiver.trim(), line)?,
                    value: Box::new(args.into_iter().next().unwrap()),
                });
            }
        }
        if let Some(receiver) = name.strip_suffix(".map") {
            if args.len() == 1 {
                return Ok(Expr::ArrayMap {
                    name: parse_name(receiver.trim(), line)?,
                    mapper: Box::new(args.into_iter().next().unwrap()),
                });
            }
        }
        if let Some(receiver) = name.strip_suffix(".flatMap") {
            if args.len() == 1 {
                return Ok(Expr::OptionFlatMap {
                    name: parse_name(receiver.trim(), line)?,
                    mapper: Box::new(args.into_iter().next().unwrap()),
                });
            }
        }
        if let Some(receiver) = name.strip_suffix(".ap") {
            if args.len() == 1 {
                return Ok(Expr::OptionAp {
                    name: parse_name(receiver.trim(), line)?,
                    value: Box::new(args.into_iter().next().unwrap()),
                });
            }
        }
        if let Some(receiver) = name.strip_suffix(".orElse") {
            if args.len() == 1 {
                return Ok(Expr::OptionOrElse {
                    name: parse_name(receiver.trim(), line)?,
                    fallback: Box::new(args.into_iter().next().unwrap()),
                });
            }
        }
        if let Some(receiver) = name.strip_suffix(".set") {
            if args.len() == 2 {
                let mut args = args.into_iter();
                return Ok(Expr::MapSet {
                    name: parse_name(receiver.trim(), line)?,
                    key: Box::new(args.next().unwrap()),
                    value: Box::new(args.next().unwrap()),
                });
            }
        }
        if let Some(receiver) = name.strip_suffix(".remove") {
            if args.len() == 1 {
                return Ok(Expr::MapRemove {
                    name: parse_name(receiver.trim(), line)?,
                    key: Box::new(args.into_iter().next().unwrap()),
                });
            }
        }
        if name == "process.env" && args.len() == 1 {
            return Ok(Expr::ProcessEnv {
                name: Box::new(args.into_iter().next().unwrap()),
            });
        }
        if name == "fs.isFile" && args.len() == 1 {
            return Ok(Expr::FsIsFile {
                path: Box::new(args.into_iter().next().unwrap()),
            });
        }
        if name == "fs.isDir" && args.len() == 1 {
            return Ok(Expr::FsIsDir {
                path: Box::new(args.into_iter().next().unwrap()),
            });
        }
        if name == "fs.size" && args.len() == 1 {
            return Ok(Expr::FsSize {
                path: Box::new(args.into_iter().next().unwrap()),
            });
        }
        if name == "fs.readLines" && args.len() == 1 {
            return Ok(Expr::FsReadLines {
                path: Box::new(args.into_iter().next().unwrap()),
            });
        }
        if name == "fs.list" && args.len() == 1 {
            return Ok(Expr::FsList {
                path: Box::new(args.into_iter().next().unwrap()),
            });
        }
        if name == "fs.writeLines" && args.len() == 2 {
            let mut args = args.into_iter();
            return Ok(Expr::FsWriteLines {
                path: Box::new(args.next().unwrap()),
                lines: Box::new(args.next().unwrap()),
            });
        }
        if name == "fs.appendLines" && args.len() == 2 {
            let mut args = args.into_iter();
            return Ok(Expr::FsAppendLines {
                path: Box::new(args.next().unwrap()),
                lines: Box::new(args.next().unwrap()),
            });
        }
        if name == "json.parse" && args.len() == 1 {
            return Ok(Expr::JsonParse {
                value: Box::new(args.into_iter().next().unwrap()),
            });
        }
        if name == "json.stringify" && args.len() == 1 {
            if let Some(Expr::Ident(name)) = args.first() {
                return Ok(Expr::JsonStringify { name: name.clone() });
            }
            return Ok(Expr::JsonStringifyValue {
                value: Box::new(args.into_iter().next().unwrap()),
            });
        }
        if let Some(receiver) = name.strip_suffix(".has") {
            if args.len() == 1 {
                return Ok(Expr::MapHas {
                    name: parse_name(receiver.trim(), line)?,
                    key: Box::new(args.into_iter().next().unwrap()),
                });
            }
        }
        if let Some(receiver) = name.strip_suffix(".contains") {
            if args.len() == 1 {
                return Ok(Expr::StringContains {
                    name: parse_name(receiver.trim(), line)?,
                    needle: Box::new(args.into_iter().next().unwrap()),
                });
            }
        }
        if let Some(receiver) = name.strip_suffix(".indexOf") {
            if args.len() == 1 {
                return Ok(Expr::ArrayIndexOf {
                    name: parse_name(receiver.trim(), line)?,
                    value: Box::new(args.into_iter().next().unwrap()),
                });
            }
        }
        if let Some(receiver) = name.strip_suffix(".startsWith") {
            if args.len() == 1 {
                return Ok(Expr::StringStartsWith {
                    name: parse_name(receiver.trim(), line)?,
                    prefix: Box::new(args.into_iter().next().unwrap()),
                });
            }
        }
        if let Some(receiver) = name.strip_suffix(".endsWith") {
            if args.len() == 1 {
                return Ok(Expr::StringEndsWith {
                    name: parse_name(receiver.trim(), line)?,
                    suffix: Box::new(args.into_iter().next().unwrap()),
                });
            }
        }
        if let Some(receiver) = name.strip_suffix(".repeat") {
            if args.len() == 1 {
                let count = Box::new(args.into_iter().next().unwrap());
                if let Ok(name) = parse_name(receiver.trim(), line) {
                    return Ok(Expr::StringRepeat { name, count });
                }
                return Ok(Expr::StringRepeatValue {
                    value: Box::new(parse_expr(strip_wrapped_parens(receiver.trim()), line)?),
                    count,
                });
            }
        }
        if let Some(receiver) = name.strip_suffix(".split") {
            if args.len() == 1 {
                let separator = Box::new(args.into_iter().next().unwrap());
                if let Ok(name) = parse_name(receiver.trim(), line) {
                    return Ok(Expr::StringSplit { name, separator });
                }
                return Ok(Expr::StringSplitValue {
                    value: Box::new(parse_expr(strip_wrapped_parens(receiver.trim()), line)?),
                    separator,
                });
            }
        }
        if let Some(receiver) = name.strip_suffix(".replace") {
            if args.len() == 2 {
                let mut args = args.into_iter();
                let from = Box::new(args.next().unwrap());
                let to = Box::new(args.next().unwrap());
                if let Ok(name) = parse_name(receiver.trim(), line) {
                    return Ok(Expr::StringReplace { name, from, to });
                }
                return Ok(Expr::StringReplaceValue {
                    value: Box::new(parse_expr(strip_wrapped_parens(receiver.trim()), line)?),
                    from,
                    to,
                });
            }
        }
        if is_newtype_constructor_name(name) {
            if args.len() == 1 {
                return Ok(Expr::NewtypeCtor {
                    name: parse_type_name(name, line)?,
                    value: Box::new(args.into_iter().next().unwrap()),
                });
            }
            return Ok(Expr::Call {
                name: parse_qualified_name(name, line)?,
                args,
            });
        }
        return Ok(Expr::Call {
            name: parse_qualified_name(name, line)?,
            args,
        });
    }

    if let Some((receiver, args)) = split_top_level_method_call(input, "split") {
        let args = parse_call_args(args.trim(), line)?;
        if args.len() == 1 {
            let separator = Box::new(args.into_iter().next().unwrap());
            if let Ok(name) = parse_name(receiver.trim(), line) {
                return Ok(Expr::StringSplit { name, separator });
            }
            return Ok(Expr::StringSplitValue {
                value: Box::new(parse_expr(strip_wrapped_parens(receiver.trim()), line)?),
                separator,
            });
        }
    }

    if let Some((name, field)) = split_field(input) {
        return Ok(Expr::Field {
            name: parse_name(name.trim(), line)?,
            field: parse_name(field.trim(), line)?,
        });
    }

    Ok(Expr::Ident(parse_name(input, line)?))
}

fn split_call(input: &str) -> Option<(&str, &str)> {
    let (name, rest) = input.split_once('(')?;
    let arg = rest.strip_suffix(')')?;
    if name.trim().is_empty() {
        return None;
    }
    Some((name, arg))
}

fn split_top_level_method_call<'a>(input: &'a str, method: &str) -> Option<(&'a str, &'a str)> {
    let marker = format!(".{method}(");
    let mut start = None;
    for (index, _) in input.match_indices(&marker) {
        if is_top_level_at(input, index) {
            start = Some(index);
        }
    }
    let start = start?;
    let args_start = start + marker.len();
    if !input.ends_with(')') || !is_top_level_at(input, args_start - 1) {
        return None;
    }
    let receiver = input[..start].trim();
    let args = &input[args_start..input.len() - 1];
    if receiver.is_empty() {
        None
    } else {
        Some((receiver, args))
    }
}

fn parse_call_args(input: &str, line: usize) -> Result<Vec<Expr>, CompileError> {
    if input.is_empty() {
        return Ok(Vec::new());
    }
    let mut args = Vec::new();
    for item in split_comma_separated(input, line)? {
        args.push(parse_expr(item.trim(), line)?);
    }
    Ok(args)
}

fn split_tuple_field(input: &str) -> Option<(&str, usize)> {
    let (name, field) = input.rsplit_once("._")?;
    let field = field.parse::<usize>().ok()?;
    if name.trim().is_empty() || field == 0 {
        return None;
    }
    Some((name, field))
}

fn split_field(input: &str) -> Option<(&str, &str)> {
    let (name, field) = input.rsplit_once('.')?;
    if name.trim().is_empty()
        || field.trim().is_empty()
        || parse_name(field.trim(), 0).is_err()
        || field.trim().ends_with("()")
        || field.trim().starts_with('_')
    {
        return None;
    }
    Some((name, field))
}

fn split_index(input: &str) -> Option<(&str, &str)> {
    if !input.trim_end().ends_with(']') {
        return None;
    }
    let mut quote = None;
    let mut escaped = false;
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut index_start = None;
    for (index, ch) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            '[' if paren_depth == 0 && brace_depth == 0 && bracket_depth == 0 => {
                index_start = Some(index);
                bracket_depth += 1;
            }
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            _ => {}
        }
    }
    let index_start = index_start?;
    let name = &input[..index_start];
    let index = input[index_start + 1..].trim_end().strip_suffix(']')?;
    if name.trim().is_empty() || index.trim().is_empty() {
        return None;
    }
    Some((name, index))
}

fn split_pipeline<'a>(input: &'a str, line: usize) -> Result<Option<Vec<&'a str>>, CompileError> {
    let mut parts = Vec::new();
    let mut quote = None;
    let mut escaped = false;
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut start = 0usize;
    let mut found = false;
    let mut index = 0usize;
    while index < input.len() {
        let ch = input[index..].chars().next().unwrap();
        let ch_len = ch.len_utf8();
        if escaped {
            escaped = false;
            index += ch_len;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            index += ch_len;
            continue;
        }
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            index += ch_len;
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            index += ch_len;
            continue;
        }
        match ch {
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            '|' if input[index..].starts_with("|>")
                && bracket_depth == 0
                && paren_depth == 0
                && brace_depth == 0
                && !input[..index].ends_with('<') =>
            {
                let part = input[start..index].trim();
                if part.is_empty() {
                    return Err(CompileError::new(
                        line,
                        "expected pipeline stage".to_string(),
                    ));
                }
                parts.push(part);
                start = index + 2;
                index += 2;
                found = true;
                continue;
            }
            _ => {}
        }
        index += ch_len;
    }
    if !found {
        return Ok(None);
    }
    let part = input[start..].trim();
    if part.is_empty() {
        return Err(CompileError::new(
            line,
            "expected pipeline stage".to_string(),
        ));
    }
    parts.push(part);
    Ok(Some(parts))
}

fn split_redirect<'a>(
    input: &'a str,
    line: usize,
) -> Result<Option<(&'a str, &'a str)>, CompileError> {
    let mut quote = None;
    let mut escaped = false;
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut index = 0usize;
    while index < input.len() {
        let ch = input[index..].chars().next().unwrap();
        let ch_len = ch.len_utf8();
        if escaped {
            escaped = false;
            index += ch_len;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            index += ch_len;
            continue;
        }
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            index += ch_len;
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            index += ch_len;
            continue;
        }
        match ch {
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            '>' if input[index..].starts_with(">>")
                && bracket_depth == 0
                && paren_depth == 0
                && brace_depth == 0 =>
            {
                let left = input[..index].trim();
                let right = input[index + 2..].trim();
                if left.is_empty() || right.is_empty() {
                    return Err(CompileError::new(
                        line,
                        "expected redirect source and target".to_string(),
                    ));
                }
                return Ok(Some((left, right)));
            }
            _ => {}
        }
        index += ch_len;
    }
    Ok(None)
}

fn parse_array(input: &str, line: usize) -> Result<Expr, CompileError> {
    let Some(inner) = input
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
    else {
        return Err(CompileError::new(
            line,
            "unterminated array literal".to_string(),
        ));
    };
    let inner = inner.trim();
    if inner.is_empty() {
        return Ok(Expr::Array(Vec::new()));
    }
    let mut values = Vec::new();
    for item in split_comma_separated(inner, line)? {
        values.push(parse_expr(item.trim(), line)?);
    }
    Ok(Expr::Array(values))
}

fn parse_map_or_record(input: &str, line: usize) -> Result<Expr, CompileError> {
    let Some(inner) = input
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
    else {
        return Err(CompileError::new(
            line,
            "unterminated map literal".to_string(),
        ));
    };
    let inner = inner.trim();
    if inner.is_empty() {
        return Ok(Expr::Map(Vec::new()));
    }
    let items = split_comma_separated(inner, line)?;
    if items.iter().all(|item| {
        split_map_entry(item, line)
            .map(|(key, _)| is_bare_record_field(key.trim()))
            .unwrap_or(false)
    }) {
        let mut fields = Vec::new();
        for item in items {
            let (key, value) = split_map_entry(item, line)?;
            fields.push((
                parse_name(key.trim(), line)?,
                parse_expr(value.trim(), line)?,
            ));
        }
        return Ok(Expr::Record(fields));
    }

    let mut entries = Vec::new();
    for item in items {
        let (key, value) = split_map_entry(item, line)?;
        entries.push((
            parse_expr(key.trim(), line)?,
            parse_expr(value.trim(), line)?,
        ));
    }
    Ok(Expr::Map(entries))
}

fn is_bare_record_field(input: &str) -> bool {
    let mut chars = input.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn split_map_entry<'a>(input: &'a str, line: usize) -> Result<(&'a str, &'a str), CompileError> {
    let mut quote = None;
    let mut escaped = false;
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut brace_depth = 0usize;
    for (index, ch) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        match ch {
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            ':' if bracket_depth == 0 && paren_depth == 0 && brace_depth == 0 => {
                let key = input[..index].trim();
                let value = input[index + 1..].trim();
                if key.is_empty() || value.is_empty() {
                    return Err(CompileError::new(
                        line,
                        "expected map key and value".to_string(),
                    ));
                }
                return Ok((key, value));
            }
            _ => {}
        }
    }
    Err(CompileError::new(
        line,
        "expected `:` in map entry".to_string(),
    ))
}

fn parse_parenthesized_or_tuple(input: &str, line: usize) -> Result<Expr, CompileError> {
    let Some(inner) = input
        .strip_prefix('(')
        .and_then(|value| value.strip_suffix(')'))
    else {
        return Err(CompileError::new(
            line,
            "unterminated tuple literal".to_string(),
        ));
    };
    let items = split_comma_separated(inner.trim(), line)?;
    if items.len() == 1 {
        return parse_expr(items[0].trim(), line);
    }
    let mut values = Vec::new();
    for item in items {
        values.push(parse_expr(item.trim(), line)?);
    }
    Ok(Expr::Tuple(values))
}

fn is_float_literal(input: &str) -> bool {
    input.contains('.')
        && input.parse::<f64>().is_ok()
        && input.chars().any(|ch| ch.is_ascii_digit())
}

fn parse_env_name(input: &str, line: usize) -> Result<String, CompileError> {
    let name = parse_name(input, line)?;
    if name
        .chars()
        .all(|ch| ch == '_' || ch.is_ascii_uppercase() || ch.is_ascii_digit())
    {
        Ok(name)
    } else {
        Err(CompileError::new(
            line,
            format!("invalid environment name `{name}`"),
        ))
    }
}

fn parse_shell_command(input: &str, line: usize) -> Result<String, CompileError> {
    let trimmed = input.trim();
    if trimmed.starts_with('{') {
        return parse_braced_shell_command(trimmed, line);
    }
    parse_quoted(trimmed, line)
}

#[derive(Clone, Copy)]
enum ShellCommandPrefix {
    Quoted,
    Braced,
}

fn parse_shell_string_suffix_expr(
    input: &str,
    checked: bool,
    line: usize,
) -> Result<Option<Expr>, CompileError> {
    let (command, rest, prefix) = parse_shell_command_prefix(input.trim(), line)?;
    if rest.trim().is_empty() {
        return Ok(None);
    }
    let rest = rest.trim();
    if let Some(args) = rest.strip_prefix(".split(") {
        let Some(args) = args.strip_suffix(')') else {
            return Err(shell_command_suffix_error(prefix, line));
        };
        let args = parse_call_args(args.trim(), line)?;
        if args.len() != 1 {
            return Err(CompileError::new(
                line,
                "split expects one separator argument".to_string(),
            ));
        }
        return Ok(Some(Expr::StringSplitValue {
            value: Box::new(Expr::Command { command, checked }),
            separator: Box::new(args.into_iter().next().unwrap()),
        }));
    }
    if let Some(args) = rest.strip_prefix(".replace(") {
        let Some(args) = args.strip_suffix(')') else {
            return Err(shell_command_suffix_error(prefix, line));
        };
        let args = parse_call_args(args.trim(), line)?;
        if args.len() != 2 {
            return Err(CompileError::new(
                line,
                "replace expects search and replacement arguments".to_string(),
            ));
        }
        let mut args = args.into_iter();
        return Ok(Some(Expr::StringReplaceValue {
            value: Box::new(Expr::Command { command, checked }),
            from: Box::new(args.next().unwrap()),
            to: Box::new(args.next().unwrap()),
        }));
    }
    if let Some(args) = rest.strip_prefix(".slice(") {
        let Some(args) = args.strip_suffix(')') else {
            return Err(shell_command_suffix_error(prefix, line));
        };
        let args = parse_call_args(args.trim(), line)?;
        if args.len() != 2 {
            return Err(CompileError::new(
                line,
                "slice expects start and end arguments".to_string(),
            ));
        }
        let mut args = args.into_iter();
        return Ok(Some(Expr::StringSliceValue {
            value: Box::new(Expr::Command { command, checked }),
            start: Box::new(args.next().unwrap()),
            end: Box::new(args.next().unwrap()),
        }));
    }
    if let Some(args) = rest.strip_prefix(".repeat(") {
        let Some(args) = args.strip_suffix(')') else {
            return Err(shell_command_suffix_error(prefix, line));
        };
        let args = parse_call_args(args.trim(), line)?;
        if args.len() != 1 {
            return Err(CompileError::new(
                line,
                "repeat expects one count argument".to_string(),
            ));
        }
        return Ok(Some(Expr::StringRepeatValue {
            value: Box::new(Expr::Command { command, checked }),
            count: Box::new(args.into_iter().next().unwrap()),
        }));
    }
    if let Some(expr) =
        parse_shell_path_suffix(rest, "isAbsolute", &command, checked, line, prefix)?
    {
        return Ok(Some(expr));
    }
    if let Some(expr) = parse_shell_path_suffix(rest, "basename", &command, checked, line, prefix)?
    {
        return Ok(Some(expr));
    }
    if let Some(expr) = parse_shell_path_suffix(rest, "dirname", &command, checked, line, prefix)? {
        return Ok(Some(expr));
    }
    if let Some(expr) = parse_shell_path_suffix(rest, "stem", &command, checked, line, prefix)? {
        return Ok(Some(expr));
    }
    if let Some(expr) = parse_shell_path_suffix(rest, "extname", &command, checked, line, prefix)? {
        return Ok(Some(expr));
    }
    if let Some(args) = rest.strip_prefix(".contains(") {
        let Some(args) = args.strip_suffix(')') else {
            return Err(shell_command_suffix_error(prefix, line));
        };
        let args = parse_call_args(args.trim(), line)?;
        if args.len() != 1 {
            return Err(CompileError::new(
                line,
                "contains expects one needle argument".to_string(),
            ));
        }
        return Ok(Some(Expr::StringContainsValue {
            value: Box::new(Expr::Command { command, checked }),
            needle: Box::new(args.into_iter().next().unwrap()),
        }));
    }
    if let Some(args) = rest.strip_prefix(".indexOf(") {
        let Some(args) = args.strip_suffix(')') else {
            return Err(shell_command_suffix_error(prefix, line));
        };
        let args = parse_call_args(args.trim(), line)?;
        if args.len() != 1 {
            return Err(CompileError::new(
                line,
                "indexOf expects one needle argument".to_string(),
            ));
        }
        return Ok(Some(Expr::StringIndexOfValue {
            value: Box::new(Expr::Command { command, checked }),
            needle: Box::new(args.into_iter().next().unwrap()),
        }));
    }
    if let Some(args) = rest.strip_prefix(".startsWith(") {
        let Some(args) = args.strip_suffix(')') else {
            return Err(shell_command_suffix_error(prefix, line));
        };
        let args = parse_call_args(args.trim(), line)?;
        if args.len() != 1 {
            return Err(CompileError::new(
                line,
                "startsWith expects one prefix argument".to_string(),
            ));
        }
        return Ok(Some(Expr::StringStartsWithValue {
            value: Box::new(Expr::Command { command, checked }),
            prefix: Box::new(args.into_iter().next().unwrap()),
        }));
    }
    if let Some(args) = rest.strip_prefix(".endsWith(") {
        let Some(args) = args.strip_suffix(')') else {
            return Err(shell_command_suffix_error(prefix, line));
        };
        let args = parse_call_args(args.trim(), line)?;
        if args.len() != 1 {
            return Err(CompileError::new(
                line,
                "endsWith expects one suffix argument".to_string(),
            ));
        }
        return Ok(Some(Expr::StringEndsWithValue {
            value: Box::new(Expr::Command { command, checked }),
            suffix: Box::new(args.into_iter().next().unwrap()),
        }));
    }
    if let Some(args) = rest.strip_prefix(".len(") {
        let Some(args) = args.strip_suffix(')') else {
            return Err(shell_command_suffix_error(prefix, line));
        };
        let args = parse_call_args(args.trim(), line)?;
        if !args.is_empty() {
            return Err(CompileError::new(
                line,
                "len expects no arguments".to_string(),
            ));
        }
        return Ok(Some(Expr::StringLenValue(Box::new(Expr::Command {
            command,
            checked,
        }))));
    }
    if let Some(args) = rest.strip_prefix(".isEmpty(") {
        let Some(args) = args.strip_suffix(')') else {
            return Err(shell_command_suffix_error(prefix, line));
        };
        let args = parse_call_args(args.trim(), line)?;
        if !args.is_empty() {
            return Err(CompileError::new(
                line,
                "isEmpty expects no arguments".to_string(),
            ));
        }
        return Ok(Some(Expr::StringIsEmptyValue(Box::new(Expr::Command {
            command,
            checked,
        }))));
    }
    if let Some(args) = rest.strip_prefix(".trim(") {
        let Some(args) = args.strip_suffix(')') else {
            return Err(shell_command_suffix_error(prefix, line));
        };
        let args = parse_call_args(args.trim(), line)?;
        if !args.is_empty() {
            return Err(CompileError::new(
                line,
                "trim expects no arguments".to_string(),
            ));
        }
        return Ok(Some(Expr::StringTrimValue(Box::new(Expr::Command {
            command,
            checked,
        }))));
    }
    if let Some(args) = rest.strip_prefix(".trimStart(") {
        let Some(args) = args.strip_suffix(')') else {
            return Err(shell_command_suffix_error(prefix, line));
        };
        let args = parse_call_args(args.trim(), line)?;
        if !args.is_empty() {
            return Err(CompileError::new(
                line,
                "trimStart expects no arguments".to_string(),
            ));
        }
        return Ok(Some(Expr::StringTrimStartValue(Box::new(Expr::Command {
            command,
            checked,
        }))));
    }
    if let Some(args) = rest.strip_prefix(".trimEnd(") {
        let Some(args) = args.strip_suffix(')') else {
            return Err(shell_command_suffix_error(prefix, line));
        };
        let args = parse_call_args(args.trim(), line)?;
        if !args.is_empty() {
            return Err(CompileError::new(
                line,
                "trimEnd expects no arguments".to_string(),
            ));
        }
        return Ok(Some(Expr::StringTrimEndValue(Box::new(Expr::Command {
            command,
            checked,
        }))));
    }
    if let Some(args) = rest.strip_prefix(".toUpper(") {
        let Some(args) = args.strip_suffix(')') else {
            return Err(shell_command_suffix_error(prefix, line));
        };
        let args = parse_call_args(args.trim(), line)?;
        if !args.is_empty() {
            return Err(CompileError::new(
                line,
                "toUpper expects no arguments".to_string(),
            ));
        }
        return Ok(Some(Expr::StringToUpperValue(Box::new(Expr::Command {
            command,
            checked,
        }))));
    }
    if let Some(args) = rest.strip_prefix(".toLower(") {
        let Some(args) = args.strip_suffix(')') else {
            return Err(shell_command_suffix_error(prefix, line));
        };
        let args = parse_call_args(args.trim(), line)?;
        if !args.is_empty() {
            return Err(CompileError::new(
                line,
                "toLower expects no arguments".to_string(),
            ));
        }
        return Ok(Some(Expr::StringToLowerValue(Box::new(Expr::Command {
            command,
            checked,
        }))));
    }
    Err(shell_command_suffix_error(prefix, line))
}

fn parse_shell_path_suffix(
    rest: &str,
    method: &str,
    command: &str,
    checked: bool,
    line: usize,
    prefix: ShellCommandPrefix,
) -> Result<Option<Expr>, CompileError> {
    let marker = format!(".{method}(");
    let Some(args) = rest.strip_prefix(&marker) else {
        return Ok(None);
    };
    let Some(args) = args.strip_suffix(')') else {
        return Err(shell_command_suffix_error(prefix, line));
    };
    let args = parse_call_args(args.trim(), line)?;
    if !args.is_empty() {
        return Err(CompileError::new(
            line,
            format!("{method} expects no arguments"),
        ));
    }
    let value = Box::new(Expr::Command {
        command: command.to_string(),
        checked,
    });
    Ok(Some(match method {
        "isAbsolute" => Expr::PathIsAbsoluteValue(value),
        "basename" => Expr::PathBasenameValue(value),
        "dirname" => Expr::PathDirnameValue(value),
        "stem" => Expr::PathStemValue(value),
        "extname" => Expr::PathExtnameValue(value),
        _ => return Ok(None),
    }))
}

fn shell_command_suffix_error(prefix: ShellCommandPrefix, line: usize) -> CompileError {
    match prefix {
        ShellCommandPrefix::Quoted => {
            CompileError::new(line, "unexpected text after quoted string".to_string())
        }
        ShellCommandPrefix::Braced => {
            CompileError::new(line, "unexpected text after shell command".to_string())
        }
    }
}

fn parse_shell_command_prefix(
    input: &str,
    line: usize,
) -> Result<(String, &str, ShellCommandPrefix), CompileError> {
    if input.starts_with('{') {
        return parse_braced_shell_command_prefix(input, line);
    }
    let (command, rest) = parse_quoted_prefix(input, line)?;
    Ok((command, rest, ShellCommandPrefix::Quoted))
}

fn parse_braced_shell_command(input: &str, line: usize) -> Result<String, CompileError> {
    let (command, rest, _) = parse_braced_shell_command_prefix(input, line)?;
    if rest.trim().is_empty() {
        Ok(command)
    } else {
        Err(CompileError::new(
            line,
            "unexpected text after shell command".to_string(),
        ))
    }
}

fn parse_braced_shell_command_prefix(
    input: &str,
    line: usize,
) -> Result<(String, &str, ShellCommandPrefix), CompileError> {
    debug_assert!(input.starts_with('{'));
    let mut quote = None;
    let mut escaped = false;
    let mut depth = 0usize;
    let mut end_index = None;
    for (index, ch) in input.char_indices() {
        if end_index.is_some() {
            continue;
        }
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end_index = Some(index);
                }
            }
            _ => {}
        }
    }
    if let Some(index) = end_index {
        return Ok((
            input[1..index].trim().to_string(),
            input[index + 1..].trim_start(),
            ShellCommandPrefix::Braced,
        ));
    }
    if quote.is_some() {
        Err(CompileError::new(
            line,
            "unterminated quoted string in shell command".to_string(),
        ))
    } else {
        Err(CompileError::new(
            line,
            "unterminated shell command".to_string(),
        ))
    }
}

fn parse_quoted(input: &str, line: usize) -> Result<String, CompileError> {
    let (value, rest) = parse_quoted_prefix(input, line)?;
    if !rest.trim().is_empty() {
        return Err(CompileError::new(
            line,
            "unexpected text after quoted string".to_string(),
        ));
    }
    Ok(value)
}

fn parse_quoted_prefix<'a>(input: &'a str, line: usize) -> Result<(String, &'a str), CompileError> {
    let mut chars = input.char_indices();
    let (_, quote) = chars
        .next()
        .ok_or_else(|| CompileError::new(line, "expected quoted string".to_string()))?;
    if quote != '"' && quote != '\'' {
        return Err(CompileError::new(
            line,
            "expected quoted string".to_string(),
        ));
    }

    let mut value = String::new();
    let mut escaped = false;
    for (offset, ch) in chars {
        if escaped {
            match ch {
                'n' => value.push('\n'),
                'r' => value.push('\r'),
                't' => value.push('\t'),
                other => value.push(other),
            }
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == quote {
            return Ok((value, input[offset + ch.len_utf8()..].trim_start()));
        }
        value.push(ch);
    }
    Err(CompileError::new(
        line,
        "unterminated quoted string".to_string(),
    ))
}

fn parse_raw_quoted(input: &str, line: usize) -> Result<String, CompileError> {
    let mut chars = input.char_indices();
    let (_, quote) = chars
        .next()
        .ok_or_else(|| CompileError::new(line, "expected quoted string".to_string()))?;
    if quote != '"' && quote != '\'' {
        return Err(CompileError::new(
            line,
            "expected quoted string".to_string(),
        ));
    }

    let mut value = String::new();
    for (offset, ch) in chars {
        if ch == quote {
            let rest = &input[offset + ch.len_utf8()..];
            if !rest.trim().is_empty() {
                return Err(CompileError::new(
                    line,
                    "unexpected text after quoted string".to_string(),
                ));
            }
            return Ok(value);
        }
        value.push(ch);
    }
    Err(CompileError::new(
        line,
        "unterminated quoted string".to_string(),
    ))
}

fn split_top_level<'a>(input: &'a str, op: &str) -> Option<(&'a str, &'a str)> {
    let mut quote = None;
    let mut escaped = false;
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut last_match = None;
    for (index, ch) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        if input[index..].starts_with(op)
            && bracket_depth == 0
            && paren_depth == 0
            && brace_depth == 0
            && valid_operator_match(input, index, op)
        {
            last_match = Some(index);
        }
        match ch {
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            _ => {}
        }
    }
    last_match.map(|index| (&input[..index], &input[index + op.len()..]))
}

fn strip_top_level_postfix<'a>(input: &'a str, operator: &str) -> Option<&'a str> {
    let index = input.len().checked_sub(operator.len())?;
    if index == 0 || !input.ends_with(operator) || !is_top_level_at(input, index) {
        return None;
    }
    let value = input[..index].trim_end();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn is_top_level_at(input: &str, target: usize) -> bool {
    let mut quote = None;
    let mut escaped = false;
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut index = 0usize;
    while index < target {
        let ch = input[index..].chars().next().unwrap();
        let ch_len = ch.len_utf8();
        if escaped {
            escaped = false;
            index += ch_len;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            index += ch_len;
            continue;
        }
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            index += ch_len;
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            index += ch_len;
            continue;
        }
        match ch {
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            _ => {}
        }
        index += ch_len;
    }
    quote.is_none() && bracket_depth == 0 && paren_depth == 0 && brace_depth == 0
}

fn split_top_level_any<'a>(
    input: &'a str,
    ops: &[(&str, BinaryOp)],
) -> Option<(&'a str, BinaryOp, &'a str)> {
    let mut quote = None;
    let mut escaped = false;
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut last_match = None;
    for (index, ch) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        for (op_text, op) in ops {
            if input[index..].starts_with(op_text)
                && bracket_depth == 0
                && paren_depth == 0
                && brace_depth == 0
                && valid_operator_match(input, index, op_text)
            {
                last_match = Some((index, *op_text, *op));
            }
        }
        match ch {
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            _ => {}
        }
    }
    last_match.map(|(index, op_text, op)| (&input[..index], op, &input[index + op_text.len()..]))
}

fn split_top_level_keyword<'a>(input: &'a str, keyword: &str) -> Option<(&'a str, &'a str)> {
    let mut quote = None;
    let mut escaped = false;
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut last_match = None;
    for (index, ch) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        if input[index..].starts_with(keyword)
            && bracket_depth == 0
            && paren_depth == 0
            && brace_depth == 0
            && keyword_boundary(input, index, keyword.len())
        {
            last_match = Some(index);
        }
        match ch {
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            _ => {}
        }
    }
    last_match.map(|index| (&input[..index], &input[index + keyword.len()..]))
}

fn split_top_level_type_operator<'a>(input: &'a str, op: &str) -> Option<(&'a str, &'a str)> {
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut last_match = None;
    let mut index = 0usize;
    while index < input.len() {
        let ch = input[index..].chars().next().unwrap();
        let ch_len = ch.len_utf8();
        if input[index..].starts_with(op)
            && bracket_depth == 0
            && paren_depth == 0
            && brace_depth == 0
        {
            last_match = Some(index);
            index += op.len();
            continue;
        }
        match ch {
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            _ => {}
        }
        index += ch_len;
    }
    let index = last_match?;
    let left = input[..index].trim();
    let right = input[index + op.len()..].trim();
    if left.is_empty() || right.is_empty() {
        return None;
    }
    Some((left, right))
}

fn keyword_boundary(input: &str, index: usize, len: usize) -> bool {
    let before = input[..index].chars().next_back();
    let after = input[index + len..].chars().next();
    before.is_some_and(char::is_whitespace) && after.is_some_and(char::is_whitespace)
}

fn split_comma_separated<'a>(input: &'a str, line: usize) -> Result<Vec<&'a str>, CompileError> {
    let mut items = Vec::new();
    let mut quote = None;
    let mut escaped = false;
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut start = 0usize;
    for (index, ch) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        match ch {
            '[' => bracket_depth += 1,
            ']' => {
                bracket_depth = bracket_depth.checked_sub(1).ok_or_else(|| {
                    CompileError::new(line, "unexpected `]` in array literal".to_string())
                })?;
            }
            '(' => paren_depth += 1,
            ')' => {
                paren_depth = paren_depth.checked_sub(1).ok_or_else(|| {
                    CompileError::new(line, "unexpected `)` in tuple literal".to_string())
                })?;
            }
            '{' => brace_depth += 1,
            '}' => {
                brace_depth = brace_depth.checked_sub(1).ok_or_else(|| {
                    CompileError::new(line, "unexpected `}` in map literal".to_string())
                })?;
            }
            ',' if bracket_depth == 0 && paren_depth == 0 && brace_depth == 0 => {
                let item = input[start..index].trim();
                if item.is_empty() {
                    return Err(CompileError::new(
                        line,
                        "expected array element".to_string(),
                    ));
                }
                items.push(item);
                start = index + 1;
            }
            _ => {}
        }
    }
    if quote.is_some() {
        return Err(CompileError::new(
            line,
            "unterminated quoted string".to_string(),
        ));
    }
    if bracket_depth != 0 {
        return Err(CompileError::new(
            line,
            "unterminated array literal".to_string(),
        ));
    }
    if paren_depth != 0 {
        return Err(CompileError::new(
            line,
            "unterminated tuple literal".to_string(),
        ));
    }
    if brace_depth != 0 {
        return Err(CompileError::new(
            line,
            "unterminated map literal".to_string(),
        ));
    }
    let item = input[start..].trim();
    if item.is_empty() {
        return Err(CompileError::new(
            line,
            "expected array element".to_string(),
        ));
    }
    items.push(item);
    Ok(items)
}

fn valid_operator_position(input: &str, index: usize, len: usize) -> bool {
    let before = input[..index].trim_end();
    let after = input[index + len..].trim_start();
    !before.is_empty() && !after.is_empty()
}

fn valid_operator_match(input: &str, index: usize, op: &str) -> bool {
    if !valid_operator_position(input, index, op.len()) {
        return false;
    }
    let before = &input[..index];
    let rest = &input[index + op.len()..];
    match op {
        "<" => !before.ends_with('<') && !rest.starts_with('<'),
        ">" => !before.ends_with('>') && !rest.starts_with('>'),
        "|" => !before.ends_with('|') && !rest.starts_with('|') && !rest.starts_with('>'),
        "&" => !before.ends_with('&') && !rest.starts_with('&'),
        "+" => !before.ends_with('+') && !rest.starts_with('+'),
        _ => true,
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_statements() {
        let program = parse(
            r#"
#! /usr/bin/env nacre
## ignored
use lib.utils
const greeting: String = "hello"
const names: [String] = ["alice", "bob"]
newtype UserId = Int
type User = { name: String, age: Int }
type Unary = String => String
fn greet(name: String, prefix: String = "Hello"): String {
return "${prefix}, ${name}"
}
let count: Int = 1
const message = greet("Nacre")
const label = if count > 0 { "positive" } else { "zero" }
const matched = match label { "positive" => "yes", _ => "no" }
const piped = $sh"printf 'a\nb\n'" |> $sh"grep b"
$sh"printf plain" |> $sh"cat"
count = count + 2
try $sh"echo ok"
$sh'echo plain'
require("git", version = ">= 1")
requireOneOf(["curl", "wget"])
if count > 0 {
$sh'echo positive'
} else {
$sh'echo zero'
}
while count > 0 {
if count == 1 {
break
}
count = count - 1
continue
}
for person in names {
$sh"echo ${person}"
}
$sh"printf write" >> write("/tmp/nacre-write")
$sh"printf append" >> append("/tmp/nacre-write")
$sh"printf err >&2" >> write("/tmp/nacre-write", stderr = "/tmp/nacre-err")
raw {
echo raw
}
"#,
        )
        .unwrap();

        assert_eq!(program.statements().len(), 25);
        assert!(matches!(program.statements()[0], Statement::Use { .. }));
        assert!(matches!(
            program.statements()[1],
            Statement::Const {
                annotation: Some(Type::String),
                ..
            }
        ));
        assert!(matches!(
            program.statements()[2],
            Statement::Const {
                annotation: Some(Type::Array(_)),
                ..
            }
        ));
        assert!(matches!(program.statements()[3], Statement::Newtype { .. }));
        assert!(matches!(
            program.statements()[4],
            Statement::TypeAlias { .. }
        ));
        assert!(matches!(
            program.statements()[5],
            Statement::TypeAlias {
                ty: Type::Function(_, _),
                ..
            }
        ));
        assert!(matches!(
            program.statements()[6],
            Statement::Function { .. }
        ));
        assert!(matches!(
            program.statements()[7],
            Statement::Let {
                annotation: Some(Type::Int),
                ..
            }
        ));
        assert!(matches!(
            program.statements()[8],
            Statement::Const {
                expr: Expr::Call { .. },
                ..
            }
        ));
        assert!(matches!(
            program.statements()[9],
            Statement::Const {
                expr: Expr::IfElse { .. },
                ..
            }
        ));
        assert!(matches!(
            program.statements()[10],
            Statement::Const {
                expr: Expr::Match { .. },
                ..
            }
        ));
        assert!(matches!(
            program.statements()[11],
            Statement::Const {
                expr: Expr::Pipeline { .. },
                ..
            }
        ));
        assert_eq!(
            program.statements()[12],
            Statement::Command("printf plain | cat".into())
        );
        assert!(matches!(program.statements()[13], Statement::Assign { .. }));
        assert_eq!(
            program.statements()[14],
            Statement::TryCommand("echo ok".into())
        );
        assert_eq!(
            program.statements()[15],
            Statement::Command("echo plain".into())
        );
        assert_eq!(
            program.statements()[16],
            Statement::Require {
                command: "git".into(),
                version: Some(">= 1".into()),
            }
        );
        assert_eq!(
            program.statements()[17],
            Statement::RequireOneOf {
                commands: vec!["curl".into(), "wget".into()]
            }
        );
        assert!(matches!(program.statements()[18], Statement::If { .. }));
        assert!(matches!(program.statements()[19], Statement::While { .. }));
        assert!(matches!(program.statements()[20], Statement::For { .. }));
        assert!(matches!(
            program.statements()[21],
            Statement::Redirect { append: false, .. }
        ));
        assert!(matches!(
            program.statements()[22],
            Statement::Redirect { append: true, .. }
        ));
        assert!(matches!(
            program.statements()[23],
            Statement::Redirect {
                append: false,
                stderr: Some(_),
                ..
            }
        ));
        assert_eq!(
            program.statements()[24],
            Statement::Raw("echo raw\n".into())
        );
    }

    #[test]
    fn parses_function_call_reassignment() {
        assert_eq!(
            parse("value = decorate(\"prefix\", source()!)\n")
                .unwrap()
                .statements()[0],
            Statement::Assign {
                name: "value".into(),
                expr: Expr::Call {
                    name: "decorate".into(),
                    args: vec![
                        Expr::String("prefix".into()),
                        Expr::TryResult(Box::new(Expr::Call {
                            name: "source".into(),
                            args: Vec::new(),
                        })),
                    ],
                },
            }
        );
    }

    #[test]
    fn parses_destructuring_bindings() {
        let program = parse(
            r#"
const (host, port) = ("localhost", 8080)
let { name, age } = { name: "Ada", age: 36 }
const [first, ...rest] = ["a", "b", "c"]
"#,
        )
        .unwrap();

        assert_eq!(
            program.statements()[0],
            Statement::Destructure {
                mutable: false,
                pattern: BindingPattern::Tuple(vec!["host".into(), "port".into()]),
                expr: Expr::Tuple(vec![Expr::String("localhost".into()), Expr::Int(8080)]),
            }
        );
        assert_eq!(
            program.statements()[1],
            Statement::Destructure {
                mutable: true,
                pattern: BindingPattern::Record(vec![
                    ("name".into(), "name".into()),
                    ("age".into(), "age".into()),
                ]),
                expr: Expr::Record(vec![
                    ("name".into(), Expr::String("Ada".into())),
                    ("age".into(), Expr::Int(36)),
                ]),
            }
        );
        assert_eq!(
            program.statements()[2],
            Statement::Destructure {
                mutable: false,
                pattern: BindingPattern::Array {
                    names: vec!["first".into()],
                    rest: Some("rest".into()),
                },
                expr: Expr::Array(vec![
                    Expr::String("a".into()),
                    Expr::String("b".into()),
                    Expr::String("c".into()),
                ]),
            }
        );
    }

    #[test]
    fn parses_inline_comments_outside_quotes_and_shell_fragments() {
        let program = parse(
            r#"
const name = "a ## b" ## trailing
const raw = r"keep ## raw" ## trailing
$sh{ printf '## shell' } ## trailing
if true { ## block
$sh"echo ok" ## command
} ## end
"#,
        )
        .unwrap();

        assert_eq!(program.statements().len(), 4);
        assert_eq!(
            program.statements()[0],
            Statement::Const {
                name: "name".into(),
                annotation: None,
                expr: Expr::String("a ## b".into()),
            }
        );
        assert_eq!(
            program.statements()[1],
            Statement::Const {
                name: "raw".into(),
                annotation: None,
                expr: Expr::RawString("keep ## raw".into()),
            }
        );
        assert_eq!(
            program.statements()[2],
            Statement::Command("printf '## shell'".into())
        );
        assert!(matches!(program.statements()[3], Statement::If { .. }));
    }

    #[test]
    fn parses_rest_parameters() {
        let program = parse(
            r#"
fn summarize(label: String, values: ...String): String {
return label
}
"#,
        )
        .unwrap();

        let Statement::Function { params, .. } = &program.statements()[0] else {
            panic!("expected function");
        };
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].ty, Type::String);
        assert!(!params[0].variadic);
        assert_eq!(params[1].ty, Type::Array(Box::new(Type::String)));
        assert!(params[1].variadic);
    }

    #[test]
    fn parses_marker_traits_and_impls() {
        let program = parse(
            r#"
trait Show[T] {
## marker
}
impl Show[Int] {
}
"#,
        )
        .unwrap();

        assert_eq!(
            program.statements()[0],
            Statement::Trait {
                name: "Show".into(),
                type_param: "T".into(),
                methods: Vec::new()
            }
        );
        assert_eq!(
            program.statements()[1],
            Statement::Impl {
                trait_name: "Show".into(),
                for_type: Type::Int,
                methods: Vec::new()
            }
        );
    }

    #[test]
    fn parses_trait_method_signatures_and_impl_methods() {
        let program = parse(
            r#"
trait Show[T] {
fn show(value: T): String
}
impl Show[Int] {
fn show(value: Int): String {
return "int"
}
}
"#,
        )
        .unwrap();

        assert!(matches!(
            &program.statements()[0],
            Statement::Trait { methods, .. }
                if methods.len() == 1
                    && methods[0].name == "show"
                    && methods[0].params[0].ty == Type::Named("T".into())
        ));
        assert!(matches!(
            &program.statements()[1],
            Statement::Impl { methods, .. }
                if methods.len() == 1
                    && methods[0].name == "show"
                    && methods[0].params[0].ty == Type::Int
        ));
    }

    #[test]
    fn parses_nested_raw_block_delimiters() {
        let program = parse(
            r#"
raw {
echo outer
raw {
echo inner
}
echo done
}
"#,
        )
        .unwrap();

        assert_eq!(
            program.statements()[0],
            Statement::Raw("echo outer\nraw {\necho inner\n}\necho done\n".into())
        );
    }

    #[test]
    fn parses_all_binary_operators() {
        let cases = [
            ("1 - 2", BinaryOp::Sub),
            ("1 * 2", BinaryOp::Mul),
            ("1 / 2", BinaryOp::Div),
            ("1 % 2", BinaryOp::Mod),
            ("1 != 2", BinaryOp::Ne),
            ("1 <= 2", BinaryOp::Le),
            ("1 > 2", BinaryOp::Gt),
            ("1 >= 2", BinaryOp::Ge),
            (r#""a" ++ "b""#, BinaryOp::Concat),
            ("1 & 3", BinaryOp::BitAnd),
            ("1 | 2", BinaryOp::BitOr),
            ("1 ^ 3", BinaryOp::BitXor),
            ("1 << 2", BinaryOp::Shl),
            ("4 >> 1", BinaryOp::Shr),
            ("true && false", BinaryOp::And),
            ("true || false", BinaryOp::Or),
        ];
        for (source, expected) in cases {
            let expr = parse_expr(source, 1).unwrap();
            assert_eq!(binary_op(expr), Some(expected));
        }
        assert_eq!(
            parse_expr("!hasCommand(\"missing\")", 1).unwrap(),
            Expr::Not(Box::new(Expr::HasCommand("missing".into())))
        );
        assert_eq!(
            parse_expr("~1", 1).unwrap(),
            Expr::BitNot(Box::new(Expr::Int(1)))
        );
        assert_eq!(parse_expr("(1)", 1).unwrap(), Expr::Int(1));
        assert!(matches!(
            parse_expr("(1 + 2) * 3", 1).unwrap(),
            Expr::Binary { op: BinaryOp::Mul, left, .. }
                if matches!(*left, Expr::Binary { op: BinaryOp::Add, .. })
        ));
        assert!(matches!(
            parse_expr("1 * (2 + 3)", 1).unwrap(),
            Expr::Binary { op: BinaryOp::Mul, right, .. }
                if matches!(*right, Expr::Binary { op: BinaryOp::Add, .. })
        ));
        assert_eq!(
            parse_expr("value as UserId", 1).unwrap(),
            Expr::Cast {
                expr: Box::new(Expr::Ident("value".into())),
                ty: Type::Named("UserId".into()),
            }
        );
        assert!(matches!(
            parse_expr("1 << 2 == 4", 1).unwrap(),
            Expr::Binary { op: BinaryOp::Eq, left, .. }
                if matches!(*left, Expr::Binary { op: BinaryOp::Shl, .. })
        ));
        assert!(matches!(
            parse_expr("choose(true || false)", 1).unwrap(),
            Expr::Call { args, .. }
                if matches!(args.first(), Some(Expr::Binary { op: BinaryOp::Or, .. }))
        ));
        assert!(matches!(
            parse_expr(r#"choose("a" ++ "b")"#, 1).unwrap(),
            Expr::Call { args, .. }
                if matches!(args.first(), Some(Expr::Binary { op: BinaryOp::Concat, .. }))
        ));
    }

    #[test]
    fn parses_primitive_literals() {
        assert_eq!(parse_expr("()", 1).unwrap(), Expr::Unit);
        assert_eq!(parse_expr("0xFF", 1).unwrap(), Expr::Int(255));
        assert_eq!(parse_expr("0b1010", 1).unwrap(), Expr::Int(10));
        assert_eq!(parse_expr("3.14", 1).unwrap(), Expr::Float("3.14".into()));
        assert_eq!(
            parse_expr(r#"r"raw \n text""#, 1).unwrap(),
            Expr::RawString(r"raw \n text".into())
        );
        assert_eq!(
            parse_expr(r#""escaped \n text""#, 1).unwrap(),
            Expr::String("escaped \n text".into())
        );
        assert_eq!(
            parse(
                r#"
const text = """
hello
"world"
"""
"#,
            )
            .unwrap()
            .statements()[0],
            Statement::Const {
                name: "text".into(),
                annotation: None,
                expr: Expr::String("\nhello\n\"world\"\n".into()),
            }
        );
        assert_eq!(parse_expr("env.HOME", 1).unwrap(), Expr::Env("HOME".into()));
        assert_eq!(
            parse_expr(r#"env.HOME ?? "/tmp""#, 1).unwrap(),
            Expr::EnvDefault {
                name: "HOME".into(),
                default: "/tmp".into()
            }
        );
        assert_eq!(
            parse_expr("Some(1)", 1).unwrap(),
            Expr::Some(Box::new(Expr::Int(1)))
        );
        assert_eq!(parse_expr("None", 1).unwrap(), Expr::None);
        assert_eq!(
            parse_expr("Ok(1)", 1).unwrap(),
            Expr::Ok(Box::new(Expr::Int(1)))
        );
        assert_eq!(
            parse_expr(r#"Err("nope")"#, 1).unwrap(),
            Expr::Err(Box::new(Expr::String("nope".into())))
        );
        let program = parse(
            r#"
const value = do {
left <- Some(1)
const offset: Int = left + 1
right <- Some(2)
pure(offset + right)
}
"#,
        )
        .unwrap();
        assert!(matches!(
            &program.statements()[0],
            Statement::Const {
                expr: Expr::Do { steps, result },
                ..
            } if steps.len() == 3
                && matches!(result.as_ref(), Expr::Call { name, args } if name == "pure" && args.len() == 1)
        ));
        assert_eq!(
            parse_expr("Ok(1)?", 1).unwrap(),
            Expr::ResultOption(Box::new(Expr::Ok(Box::new(Expr::Int(1)))))
        );
        assert_eq!(
            parse_expr("Ok(1)!", 1).unwrap(),
            Expr::TryResult(Box::new(Expr::Ok(Box::new(Expr::Int(1)))))
        );
        assert_eq!(
            parse("step()!\n").unwrap().statements()[0],
            Statement::TryResult(Expr::Call {
                name: "step".into(),
                args: Vec::new()
            })
        );
        let sum = parse(
            r#"
type Shape =
  | Circle(Float)
  | Rect(Float, Float)
  | Empty
"#,
        )
        .unwrap();
        assert!(matches!(
            &sum.statements()[0],
            Statement::SumType { name, variants }
                if name == "Shape"
                    && variants.len() == 3
                    && variants[0].name == "Circle"
                    && variants[1].fields == vec![Type::Float, Type::Float]
                    && variants[2].fields.is_empty()
        ));
        assert_eq!(
            parse_expr(r#"maybe ?? "fallback""#, 1).unwrap(),
            Expr::Default {
                value: Box::new(Expr::Ident("maybe".into())),
                fallback: Box::new(Expr::String("fallback".into()))
            }
        );
        assert_eq!(
            parse_expr(r#"$sh"hostname""#, 1).unwrap(),
            Expr::Command {
                command: "hostname".into(),
                checked: false
            }
        );
        assert_eq!(
            parse(
                r#"
const output = $sh"cat <<EOF
hello
## shell content
EOF"
"#,
            )
            .unwrap()
            .statements()[0],
            Statement::Const {
                name: "output".into(),
                annotation: None,
                expr: Expr::Command {
                    command: "cat <<EOF\nhello\n## shell content\nEOF".into(),
                    checked: false,
                },
            }
        );
        assert_eq!(
            parse_expr(r#"async $sh"hostname""#, 1).unwrap(),
            Expr::AsyncCommand("hostname".into())
        );
        assert_eq!(
            parse_expr(r#"spawn $sh"hostname""#, 1).unwrap(),
            Expr::AsyncCommand("hostname".into())
        );
        assert_eq!(
            parse_expr("await future", 1).unwrap(),
            Expr::Await("future".into())
        );
        assert_eq!(
            parse_expr("future.wait()", 1).unwrap(),
            Expr::Await("future".into())
        );
        assert_eq!(
            parse_expr(r#"try $sh"hostname""#, 1).unwrap(),
            Expr::Command {
                command: "hostname".into(),
                checked: true
            }
        );
        assert_eq!(
            parse_expr(r#"hasCommand("git")"#, 1).unwrap(),
            Expr::HasCommand("git".into())
        );
        assert_eq!(
            parse_expr(r#"pathExists("/tmp")"#, 1).unwrap(),
            Expr::PathExists(Box::new(Expr::String("/tmp".into())))
        );
        assert_eq!(parse_expr("process.args()", 1).unwrap(), Expr::ProcessArgs);
        assert_eq!(parse_expr("cli.parse()", 1).unwrap(), Expr::CliParse);
        assert_eq!(
            parse_expr(r#"process.env("HOME")"#, 1).unwrap(),
            Expr::ProcessEnv {
                name: Box::new(Expr::String("HOME".into()))
            }
        );
        assert_eq!(
            parse_expr(r#"fs.isFile("/tmp/a")"#, 1).unwrap(),
            Expr::FsIsFile {
                path: Box::new(Expr::String("/tmp/a".into()))
            }
        );
        assert_eq!(
            parse_expr(r#"fs.isDir("/tmp")"#, 1).unwrap(),
            Expr::FsIsDir {
                path: Box::new(Expr::String("/tmp".into()))
            }
        );
        assert_eq!(
            parse_expr(r#"fs.size("/tmp/a")"#, 1).unwrap(),
            Expr::FsSize {
                path: Box::new(Expr::String("/tmp/a".into()))
            }
        );
        assert_eq!(
            parse_expr(r#"fs.readLines("/tmp/a")"#, 1).unwrap(),
            Expr::FsReadLines {
                path: Box::new(Expr::String("/tmp/a".into()))
            }
        );
        assert_eq!(
            parse_expr(r#"fs.list("/tmp")"#, 1).unwrap(),
            Expr::FsList {
                path: Box::new(Expr::String("/tmp".into()))
            }
        );
        assert_eq!(
            parse_expr(r#"fs.writeLines("/tmp/a", lines)"#, 1).unwrap(),
            Expr::FsWriteLines {
                path: Box::new(Expr::String("/tmp/a".into())),
                lines: Box::new(Expr::Ident("lines".into()))
            }
        );
        assert_eq!(
            parse_expr(r#"fs.appendLines("/tmp/a", lines)"#, 1).unwrap(),
            Expr::FsAppendLines {
                path: Box::new(Expr::String("/tmp/a".into())),
                lines: Box::new(Expr::Ident("lines".into()))
            }
        );
        assert_eq!(
            parse_expr(r#"json.parse("{\"name\":\"Ada\"}")"#, 1).unwrap(),
            Expr::JsonParse {
                value: Box::new(Expr::String(r#"{"name":"Ada"}"#.into()))
            }
        );
        assert_eq!(
            parse_expr("json.stringify(data)", 1).unwrap(),
            Expr::JsonStringify {
                name: "data".into()
            }
        );
        assert_eq!(
            parse_expr(r#"json.stringify({ "name": "Ada" })"#, 1).unwrap(),
            Expr::JsonStringifyValue {
                value: Box::new(Expr::Map(vec![(
                    Expr::String("name".into()),
                    Expr::String("Ada".into())
                )]))
            }
        );
        assert_eq!(
            parse_expr(r#"["a,b", "c"]"#, 1).unwrap(),
            Expr::Array(vec![Expr::String("a,b".into()), Expr::String("c".into())])
        );
        assert_eq!(
            parse_expr("[1, 2, 3]", 1).unwrap(),
            Expr::Array(vec![Expr::Int(1), Expr::Int(2), Expr::Int(3)])
        );
        assert_eq!(
            parse_expr(r#"{ "PORT": "8080", "HOST": "localhost" }"#, 1).unwrap(),
            Expr::Map(vec![
                (Expr::String("PORT".into()), Expr::String("8080".into())),
                (
                    Expr::String("HOST".into()),
                    Expr::String("localhost".into())
                )
            ])
        );
        assert_eq!(
            parse_expr(r#"{ name: "Ada", age: 36 }"#, 1).unwrap(),
            Expr::Record(vec![
                ("name".into(), Expr::String("Ada".into())),
                ("age".into(), Expr::Int(36))
            ])
        );
        assert_eq!(
            parse_expr("names[0]", 1).unwrap(),
            Expr::Index {
                name: "names".into(),
                index: Box::new(Expr::Int(0))
            }
        );
        assert_eq!(
            parse_expr(r#"(["alice", "bob"])[1]"#, 1).unwrap(),
            Expr::IndexValue {
                value: Box::new(Expr::Array(vec![
                    Expr::String("alice".into()),
                    Expr::String("bob".into())
                ])),
                index: Box::new(Expr::Int(1))
            }
        );
        assert_eq!(
            parse_expr(r#"({"PORT": "8080"})["PORT"]"#, 1).unwrap(),
            Expr::IndexValue {
                value: Box::new(Expr::Map(vec![(
                    Expr::String("PORT".into()),
                    Expr::String("8080".into())
                )])),
                index: Box::new(Expr::String("PORT".into()))
            }
        );
        assert_eq!(
            parse_expr("names.len()", 1).unwrap(),
            Expr::Len("names".into())
        );
        assert_eq!(
            parse_expr(r#"("nacre").len()"#, 1).unwrap(),
            Expr::StringLenValue(Box::new(Expr::String("nacre".into())))
        );
        assert_eq!(
            parse_expr(r#"(["alice", "bob"]).len()"#, 1).unwrap(),
            Expr::ArrayLenValue(Box::new(Expr::Array(vec![
                Expr::String("alice".into()),
                Expr::String("bob".into())
            ])))
        );
        assert_eq!(
            parse_expr(r#"({"PORT": "8080"}).len()"#, 1).unwrap(),
            Expr::MapLenValue(Box::new(Expr::Map(vec![(
                Expr::String("PORT".into()),
                Expr::String("8080".into())
            )])))
        );
        assert_eq!(
            parse_expr("names.isEmpty()", 1).unwrap(),
            Expr::IsEmpty("names".into())
        );
        assert_eq!(
            parse_expr(r#"("nacre").isEmpty()"#, 1).unwrap(),
            Expr::StringIsEmptyValue(Box::new(Expr::String("nacre".into())))
        );
        assert_eq!(
            parse_expr(r#"([]).isEmpty()"#, 1).unwrap(),
            Expr::ArrayIsEmptyValue(Box::new(Expr::Array(Vec::new())))
        );
        assert_eq!(
            parse_expr(r#"({}).isEmpty()"#, 1).unwrap(),
            Expr::MapIsEmptyValue(Box::new(Expr::Map(Vec::new())))
        );
        assert_eq!(
            parse_expr(r#"("nacre").slice(1, 4)"#, 1).unwrap(),
            Expr::StringSliceValue {
                value: Box::new(Expr::String("nacre".into())),
                start: Box::new(Expr::Int(1)),
                end: Box::new(Expr::Int(4))
            }
        );
        assert_eq!(
            parse_expr("names.first()", 1).unwrap(),
            Expr::ArrayFirst("names".into())
        );
        assert_eq!(
            parse_expr(r#"(["alice", "bob"]).first()"#, 1).unwrap(),
            Expr::ArrayFirstValue(Box::new(Expr::Array(vec![
                Expr::String("alice".into()),
                Expr::String("bob".into())
            ])))
        );
        assert_eq!(
            parse_expr("names.last()", 1).unwrap(),
            Expr::ArrayLast("names".into())
        );
        assert_eq!(
            parse_expr(r#"(["alice", "bob"]).last()"#, 1).unwrap(),
            Expr::ArrayLastValue(Box::new(Expr::Array(vec![
                Expr::String("alice".into()),
                Expr::String("bob".into())
            ])))
        );
        assert_eq!(
            parse_expr("names.reverse()", 1).unwrap(),
            Expr::ArrayReverse("names".into())
        );
        assert_eq!(
            parse_expr(r#"(["alice", "bob"]).reverse()"#, 1).unwrap(),
            Expr::ArrayReverseValue(Box::new(Expr::Array(vec![
                Expr::String("alice".into()),
                Expr::String("bob".into())
            ])))
        );
        assert_eq!(
            parse_expr("names.sort()", 1).unwrap(),
            Expr::ArraySort("names".into())
        );
        assert_eq!(
            parse_expr(r#"(["bob", "alice"]).sort()"#, 1).unwrap(),
            Expr::ArraySortValue(Box::new(Expr::Array(vec![
                Expr::String("bob".into()),
                Expr::String("alice".into())
            ])))
        );
        assert_eq!(
            parse_expr("names.unique()", 1).unwrap(),
            Expr::ArrayUnique("names".into())
        );
        assert_eq!(
            parse_expr(r#"(["alice", "alice"]).unique()"#, 1).unwrap(),
            Expr::ArrayUniqueValue(Box::new(Expr::Array(vec![
                Expr::String("alice".into()),
                Expr::String("alice".into())
            ])))
        );
        assert_eq!(
            parse_expr(r#"names.map(name => name.toUpper())"#, 1).unwrap(),
            Expr::ArrayMap {
                name: "names".into(),
                mapper: Box::new(Expr::Lambda {
                    params: vec!["name".into()],
                    body: Box::new(Expr::StringToUpper("name".into()))
                })
            }
        );
        assert_eq!(
            parse_expr(r#"([1, 2]).map(value => value * 2)"#, 1).unwrap(),
            Expr::ArrayMapValue {
                value: Box::new(Expr::Array(vec![Expr::Int(1), Expr::Int(2)])),
                mapper: Box::new(Expr::Lambda {
                    params: vec!["value".into()],
                    body: Box::new(Expr::Binary {
                        left: Box::new(Expr::Ident("value".into())),
                        op: BinaryOp::Mul,
                        right: Box::new(Expr::Int(2))
                    })
                })
            }
        );
        assert_eq!(
            split_top_level_method_call(r#"Some(2).map(value => value * 3)"#, "map"),
            Some(("Some(2)", "value => value * 3"))
        );
        assert_eq!(
            parse_builtin_expr_call(r#"Some(2).map(value => value * 3)"#, "Some", 1).unwrap(),
            None
        );
        assert_eq!(
            parse_expr("Some(2)", 1).unwrap(),
            Expr::Some(Box::new(Expr::Int(2)))
        );
        assert_eq!(
            parse_expr(r#"Some(2).map(value => value * 3)"#, 1).unwrap(),
            Expr::ArrayMapValue {
                value: Box::new(Expr::Some(Box::new(Expr::Int(2)))),
                mapper: Box::new(Expr::Lambda {
                    params: vec!["value".into()],
                    body: Box::new(Expr::Binary {
                        left: Box::new(Expr::Ident("value".into())),
                        op: BinaryOp::Mul,
                        right: Box::new(Expr::Int(3))
                    })
                })
            }
        );
        assert_eq!(
            parse_expr(r#"Some(2).flatMap(value => Some(value * 3))"#, 1).unwrap(),
            Expr::OptionFlatMapValue {
                value: Box::new(Expr::Some(Box::new(Expr::Int(2)))),
                mapper: Box::new(Expr::Lambda {
                    params: vec!["value".into()],
                    body: Box::new(Expr::Some(Box::new(Expr::Binary {
                        left: Box::new(Expr::Ident("value".into())),
                        op: BinaryOp::Mul,
                        right: Box::new(Expr::Int(3))
                    })))
                })
            }
        );
        assert_eq!(
            parse_expr(r#"Some(2).orElse(Some(3))"#, 1).unwrap(),
            Expr::OptionOrElseValue {
                value: Box::new(Expr::Some(Box::new(Expr::Int(2)))),
                fallback: Box::new(Expr::Some(Box::new(Expr::Int(3))))
            }
        );
        assert_eq!(
            split_top_level(r#"None <|> Some(3)"#, "<|>"),
            Some(("None ", " Some(3)"))
        );
        assert_eq!(
            parse_expr(r#"None <|> Some(3)"#, 1).unwrap(),
            Expr::OptionOrElseValue {
                value: Box::new(Expr::None),
                fallback: Box::new(Expr::Some(Box::new(Expr::Int(3))))
            }
        );
        assert_eq!(
            parse_expr(r#"Some(2) <$> (value => value * 3)"#, 1).unwrap(),
            Expr::ArrayMapValue {
                value: Box::new(Expr::Some(Box::new(Expr::Int(2)))),
                mapper: Box::new(Expr::Lambda {
                    params: vec!["value".into()],
                    body: Box::new(Expr::Binary {
                        left: Box::new(Expr::Ident("value".into())),
                        op: BinaryOp::Mul,
                        right: Box::new(Expr::Int(3))
                    })
                })
            }
        );
        assert_eq!(
            parse_expr(r#"Some(2) >>= (value => Some(value * 3))"#, 1).unwrap(),
            Expr::OptionFlatMapValue {
                value: Box::new(Expr::Some(Box::new(Expr::Int(2)))),
                mapper: Box::new(Expr::Lambda {
                    params: vec!["value".into()],
                    body: Box::new(Expr::Some(Box::new(Expr::Binary {
                        left: Box::new(Expr::Ident("value".into())),
                        op: BinaryOp::Mul,
                        right: Box::new(Expr::Int(3))
                    })))
                })
            }
        );
        assert_eq!(
            parse_expr(r#"Some(double).ap(Some(2))"#, 1).unwrap(),
            Expr::OptionApValue {
                function: Box::new(Expr::Some(Box::new(Expr::Ident("double".into())))),
                value: Box::new(Expr::Some(Box::new(Expr::Int(2))))
            }
        );
        assert_eq!(
            parse_expr(r#"function <*> Some(2)"#, 1).unwrap(),
            Expr::OptionAp {
                name: "function".into(),
                value: Box::new(Expr::Some(Box::new(Expr::Int(2))))
            }
        );
        assert_eq!(
            parse_expr(r#"(["alice", "bob", "carol"]).slice(1, 3)"#, 1).unwrap(),
            Expr::ArraySliceValue {
                value: Box::new(Expr::Array(vec![
                    Expr::String("alice".into()),
                    Expr::String("bob".into()),
                    Expr::String("carol".into())
                ])),
                start: Box::new(Expr::Int(1)),
                end: Box::new(Expr::Int(3))
            }
        );
        assert_eq!(
            parse_expr(r#"(["alice", "bob"]).take(1)"#, 1).unwrap(),
            Expr::ArrayTakeValue {
                value: Box::new(Expr::Array(vec![
                    Expr::String("alice".into()),
                    Expr::String("bob".into())
                ])),
                count: Box::new(Expr::Int(1))
            }
        );
        assert_eq!(
            parse_expr(r#"(["alice", "bob"]).drop(1)"#, 1).unwrap(),
            Expr::ArrayDropValue {
                value: Box::new(Expr::Array(vec![
                    Expr::String("alice".into()),
                    Expr::String("bob".into())
                ])),
                count: Box::new(Expr::Int(1))
            }
        );
        assert_eq!(
            parse_expr(r#"names.join(",")"#, 1).unwrap(),
            Expr::Join {
                name: "names".into(),
                separator: Box::new(Expr::String(",".into()))
            }
        );
        assert_eq!(
            parse_expr(r#"(["a", "b"]).join(",")"#, 1).unwrap(),
            Expr::JoinValue {
                value: Box::new(Expr::Array(vec![
                    Expr::String("a".into()),
                    Expr::String("b".into())
                ])),
                separator: Box::new(Expr::String(",".into()))
            }
        );
        assert_eq!(
            parse_expr("names.slice(1, 3)", 1).unwrap(),
            Expr::Slice {
                name: "names".into(),
                start: Box::new(Expr::Int(1)),
                end: Box::new(Expr::Int(3))
            }
        );
        assert_eq!(
            parse_expr("names.take(2)", 1).unwrap(),
            Expr::ArrayTake {
                name: "names".into(),
                count: Box::new(Expr::Int(2))
            }
        );
        assert_eq!(
            parse_expr("names.drop(1)", 1).unwrap(),
            Expr::ArrayDrop {
                name: "names".into(),
                count: Box::new(Expr::Int(1))
            }
        );
        assert_eq!(
            parse_expr(r#"names.push("carol")"#, 1).unwrap(),
            Expr::ArrayPush {
                name: "names".into(),
                value: Box::new(Expr::String("carol".into()))
            }
        );
        assert_eq!(
            parse_expr("names.pop()", 1).unwrap(),
            Expr::ArrayPop {
                name: "names".into()
            }
        );
        assert_eq!(
            parse_expr("envs.keys()", 1).unwrap(),
            Expr::MapKeys("envs".into())
        );
        assert_eq!(
            parse_expr("envs.values()", 1).unwrap(),
            Expr::MapValues("envs".into())
        );
        assert_eq!(
            parse_expr(r#"envs.has("PORT")"#, 1).unwrap(),
            Expr::MapHas {
                name: "envs".into(),
                key: Box::new(Expr::String("PORT".into()))
            }
        );
        assert_eq!(
            parse_expr(r#"envs.set("PORT", "9090")"#, 1).unwrap(),
            Expr::MapSet {
                name: "envs".into(),
                key: Box::new(Expr::String("PORT".into())),
                value: Box::new(Expr::String("9090".into()))
            }
        );
        assert_eq!(
            parse_expr(r#"envs.remove("PORT")"#, 1).unwrap(),
            Expr::MapRemove {
                name: "envs".into(),
                key: Box::new(Expr::String("PORT".into()))
            }
        );
        assert_eq!(
            parse_expr(r#"({"PORT": "8080"}).keys()"#, 1).unwrap(),
            Expr::MapKeysValue(Box::new(Expr::Map(vec![(
                Expr::String("PORT".into()),
                Expr::String("8080".into())
            )])))
        );
        assert_eq!(
            parse_expr(r#"({"PORT": "8080"}).values()"#, 1).unwrap(),
            Expr::MapValuesValue(Box::new(Expr::Map(vec![(
                Expr::String("PORT".into()),
                Expr::String("8080".into())
            )])))
        );
        assert_eq!(
            parse_expr(r#"({"PORT": "8080"}).has("PORT")"#, 1).unwrap(),
            Expr::MapHasValue {
                value: Box::new(Expr::Map(vec![(
                    Expr::String("PORT".into()),
                    Expr::String("8080".into())
                )])),
                key: Box::new(Expr::String("PORT".into()))
            }
        );
        assert_eq!(
            parse_expr(r#"text.contains("ac")"#, 1).unwrap(),
            Expr::StringContains {
                name: "text".into(),
                needle: Box::new(Expr::String("ac".into()))
            }
        );
        assert_eq!(
            parse_expr(r#"("nacre").contains("ac")"#, 1).unwrap(),
            Expr::StringContainsValue {
                value: Box::new(Expr::String("nacre".into())),
                needle: Box::new(Expr::String("ac".into()))
            }
        );
        assert_eq!(
            parse_expr(r#"(["alice", "bob"]).contains("bob")"#, 1).unwrap(),
            Expr::ArrayContainsValue {
                value: Box::new(Expr::Array(vec![
                    Expr::String("alice".into()),
                    Expr::String("bob".into())
                ])),
                item: Box::new(Expr::String("bob".into()))
            }
        );
        assert_eq!(
            parse_expr(r#"names.indexOf("bob")"#, 1).unwrap(),
            Expr::ArrayIndexOf {
                name: "names".into(),
                value: Box::new(Expr::String("bob".into()))
            }
        );
        assert_eq!(
            parse_expr(r#"(["alice", "bob"]).indexOf("bob")"#, 1).unwrap(),
            Expr::ArrayIndexOfValue {
                value: Box::new(Expr::Array(vec![
                    Expr::String("alice".into()),
                    Expr::String("bob".into())
                ])),
                item: Box::new(Expr::String("bob".into()))
            }
        );
        assert_eq!(
            parse_expr(r#"("nacre").indexOf("ac")"#, 1).unwrap(),
            Expr::StringIndexOfValue {
                value: Box::new(Expr::String("nacre".into())),
                needle: Box::new(Expr::String("ac".into()))
            }
        );
        assert_eq!(
            parse_expr(r#"text.startsWith("na")"#, 1).unwrap(),
            Expr::StringStartsWith {
                name: "text".into(),
                prefix: Box::new(Expr::String("na".into()))
            }
        );
        assert_eq!(
            parse_expr(r#"("nacre").startsWith("na")"#, 1).unwrap(),
            Expr::StringStartsWithValue {
                value: Box::new(Expr::String("nacre".into())),
                prefix: Box::new(Expr::String("na".into()))
            }
        );
        assert_eq!(
            parse_expr(r#"text.endsWith("re")"#, 1).unwrap(),
            Expr::StringEndsWith {
                name: "text".into(),
                suffix: Box::new(Expr::String("re".into()))
            }
        );
        assert_eq!(
            parse_expr(r#"("nacre").endsWith("re")"#, 1).unwrap(),
            Expr::StringEndsWithValue {
                value: Box::new(Expr::String("nacre".into())),
                suffix: Box::new(Expr::String("re".into()))
            }
        );
        assert_eq!(
            parse_expr("text.trim()", 1).unwrap(),
            Expr::StringTrim("text".into())
        );
        assert_eq!(
            parse_expr(r#"("  text  ").trim()"#, 1).unwrap(),
            Expr::StringTrimValue(Box::new(Expr::String("  text  ".into())))
        );
        assert_eq!(
            parse_expr("text.trimStart()", 1).unwrap(),
            Expr::StringTrimStart("text".into())
        );
        assert_eq!(
            parse_expr(r#"("  text  ").trimStart()"#, 1).unwrap(),
            Expr::StringTrimStartValue(Box::new(Expr::String("  text  ".into())))
        );
        assert_eq!(
            parse_expr("text.trimEnd()", 1).unwrap(),
            Expr::StringTrimEnd("text".into())
        );
        assert_eq!(
            parse_expr(r#"("  text  ").trimEnd()"#, 1).unwrap(),
            Expr::StringTrimEndValue(Box::new(Expr::String("  text  ".into())))
        );
        assert_eq!(
            parse_expr("text.toUpper()", 1).unwrap(),
            Expr::StringToUpper("text".into())
        );
        assert_eq!(
            parse_expr(r#"("text").toUpper()"#, 1).unwrap(),
            Expr::StringToUpperValue(Box::new(Expr::String("text".into())))
        );
        assert_eq!(
            parse_expr("text.toLower()", 1).unwrap(),
            Expr::StringToLower("text".into())
        );
        assert_eq!(
            parse_expr(r#"("TEXT").toLower()"#, 1).unwrap(),
            Expr::StringToLowerValue(Box::new(Expr::String("TEXT".into())))
        );
        assert_eq!(
            parse_expr("path.isAbsolute()", 1).unwrap(),
            Expr::PathIsAbsolute("path".into())
        );
        assert_eq!(
            parse_expr(r#"("/tmp/nacre.txt").isAbsolute()"#, 1).unwrap(),
            Expr::PathIsAbsoluteValue(Box::new(Expr::String("/tmp/nacre.txt".into())))
        );
        assert_eq!(
            parse_expr("path.basename()", 1).unwrap(),
            Expr::PathBasename("path".into())
        );
        assert_eq!(
            parse_expr(r#"("/tmp/nacre.txt").basename()"#, 1).unwrap(),
            Expr::PathBasenameValue(Box::new(Expr::String("/tmp/nacre.txt".into())))
        );
        assert_eq!(
            parse_expr("path.dirname()", 1).unwrap(),
            Expr::PathDirname("path".into())
        );
        assert_eq!(
            parse_expr(r#"("/tmp/nacre.txt").dirname()"#, 1).unwrap(),
            Expr::PathDirnameValue(Box::new(Expr::String("/tmp/nacre.txt".into())))
        );
        assert_eq!(
            parse_expr("path.stem()", 1).unwrap(),
            Expr::PathStem("path".into())
        );
        assert_eq!(
            parse_expr(r#"("/tmp/nacre.txt").stem()"#, 1).unwrap(),
            Expr::PathStemValue(Box::new(Expr::String("/tmp/nacre.txt".into())))
        );
        assert_eq!(
            parse_expr("path.extname()", 1).unwrap(),
            Expr::PathExtname("path".into())
        );
        assert_eq!(
            parse_expr(r#"("/tmp/nacre.txt").extname()"#, 1).unwrap(),
            Expr::PathExtnameValue(Box::new(Expr::String("/tmp/nacre.txt".into())))
        );
        assert_eq!(
            parse_expr("text.repeat(3)", 1).unwrap(),
            Expr::StringRepeat {
                name: "text".into(),
                count: Box::new(Expr::Int(3))
            }
        );
        assert_eq!(
            parse_expr(r#"("na").repeat(3)"#, 1).unwrap(),
            Expr::StringRepeatValue {
                value: Box::new(Expr::String("na".into())),
                count: Box::new(Expr::Int(3))
            }
        );
        assert_eq!(
            parse_expr(r#"text.split(",")"#, 1).unwrap(),
            Expr::StringSplit {
                name: "text".into(),
                separator: Box::new(Expr::String(",".into()))
            }
        );
        assert_eq!(
            parse_expr(r#"text.replace("na", "Na")"#, 1).unwrap(),
            Expr::StringReplace {
                name: "text".into(),
                from: Box::new(Expr::String("na".into())),
                to: Box::new(Expr::String("Na".into()))
            }
        );
        assert_eq!(
            parse_expr(r#"("nacre").replace("na", "Na")"#, 1).unwrap(),
            Expr::StringReplaceValue {
                value: Box::new(Expr::String("nacre".into())),
                from: Box::new(Expr::String("na".into())),
                to: Box::new(Expr::String("Na".into()))
            }
        );
        assert_eq!(
            parse_expr("uid.value", 1).unwrap(),
            Expr::Value("uid".into())
        );
        assert_eq!(
            parse_expr("UserId(42)", 1).unwrap(),
            Expr::NewtypeCtor {
                name: "UserId".into(),
                value: Box::new(Expr::Int(42))
            }
        );
        assert_eq!(
            parse_expr(r#"greet("Nacre")"#, 1).unwrap(),
            Expr::Call {
                name: "greet".into(),
                args: vec![Expr::String("Nacre".into())]
            }
        );
        assert_eq!(
            parse_expr(r#"utils.greet("Nacre")"#, 1).unwrap(),
            Expr::Call {
                name: "utils.greet".into(),
                args: vec![Expr::String("Nacre".into())]
            }
        );
        assert_eq!(
            parse_expr(r#"$sh"printf hi" |> $sh"cat""#, 1).unwrap(),
            Expr::Pipeline {
                input: None,
                commands: vec!["printf hi".into(), "cat".into()]
            }
        );
        assert_eq!(
            parse_expr(r#""input" |> $sh"cat""#, 1).unwrap(),
            Expr::Pipeline {
                input: Some(Box::new(Expr::String("input".into()))),
                commands: vec!["cat".into()]
            }
        );
        assert_eq!(
            parse_expr(r#"$sh{ printf '%s\n' "hi" }"#, 1).unwrap(),
            Expr::Command {
                command: r#"printf '%s\n' "hi""#.into(),
                checked: false
            }
        );
        assert_eq!(
            parse_expr(r#"$sh{ printf hi } |> $sh{ cat }"#, 1).unwrap(),
            Expr::Pipeline {
                input: None,
                commands: vec!["printf hi".into(), "cat".into()]
            }
        );
        assert_eq!(
            parse_expr(r#"if 1 < 2 { "yes" } else { "no" }"#, 1).unwrap(),
            Expr::IfElse {
                condition: Box::new(Expr::Binary {
                    left: Box::new(Expr::Int(1)),
                    op: BinaryOp::Lt,
                    right: Box::new(Expr::Int(2))
                }),
                then_expr: Box::new(Expr::String("yes".into())),
                else_expr: Box::new(Expr::String("no".into()))
            }
        );
        assert_eq!(
            parse_expr(
                r#"if false { "zero" } else if true { "one" } else { "many" }"#,
                1
            )
            .unwrap(),
            Expr::IfElse {
                condition: Box::new(Expr::Bool(false)),
                then_expr: Box::new(Expr::String("zero".into())),
                else_expr: Box::new(Expr::IfElse {
                    condition: Box::new(Expr::Bool(true)),
                    then_expr: Box::new(Expr::String("one".into())),
                    else_expr: Box::new(Expr::String("many".into()))
                })
            }
        );
        assert_eq!(
            parse_expr(r#"match "ok" { "ok" => 1, _ => 0 }"#, 1).unwrap(),
            Expr::Match {
                value: Box::new(Expr::String("ok".into())),
                arms: vec![
                    MatchArm {
                        pattern: Some(Expr::String("ok".into())),
                        guard: None,
                        expr: Expr::Int(1)
                    },
                    MatchArm {
                        pattern: None,
                        guard: None,
                        expr: Expr::Int(0)
                    }
                ]
            }
        );
        assert_eq!(
            parse_expr(
                r#"match (200, "GET") { (200, "GET") => "ok", _ => "other" }"#,
                1
            )
            .unwrap(),
            Expr::Match {
                value: Box::new(Expr::Tuple(vec![
                    Expr::Int(200),
                    Expr::String("GET".into())
                ])),
                arms: vec![
                    MatchArm {
                        pattern: Some(Expr::Tuple(vec![
                            Expr::Int(200),
                            Expr::String("GET".into())
                        ])),
                        guard: None,
                        expr: Expr::String("ok".into())
                    },
                    MatchArm {
                        pattern: None,
                        guard: None,
                        expr: Expr::String("other".into())
                    }
                ]
            }
        );
        assert_eq!(
            parse_expr(r#"match user { { name } => name, _ => "unknown" }"#, 1).unwrap(),
            Expr::Match {
                value: Box::new(Expr::Ident("user".into())),
                arms: vec![
                    MatchArm {
                        pattern: Some(Expr::RecordPattern(vec![("name".into(), None)])),
                        guard: None,
                        expr: Expr::Ident("name".into())
                    },
                    MatchArm {
                        pattern: None,
                        guard: None,
                        expr: Expr::String("unknown".into())
                    }
                ]
            }
        );
        assert_eq!(
            parse_expr(r#"match result { Ok({ status }) => status, _ => 0 }"#, 1).unwrap(),
            Expr::Match {
                value: Box::new(Expr::Ident("result".into())),
                arms: vec![
                    MatchArm {
                        pattern: Some(Expr::Ok(Box::new(Expr::RecordPattern(vec![(
                            "status".into(),
                            None
                        )])))),
                        guard: None,
                        expr: Expr::Ident("status".into())
                    },
                    MatchArm {
                        pattern: None,
                        guard: None,
                        expr: Expr::Int(0)
                    }
                ]
            }
        );
        assert_eq!(
            parse_expr(
                r#"match pair { Some((code, text)) => text, _ => "unknown" }"#,
                1
            )
            .unwrap(),
            Expr::Match {
                value: Box::new(Expr::Ident("pair".into())),
                arms: vec![
                    MatchArm {
                        pattern: Some(Expr::Some(Box::new(Expr::Tuple(vec![
                            Expr::Ident("code".into()),
                            Expr::Ident("text".into())
                        ])))),
                        guard: None,
                        expr: Expr::Ident("text".into())
                    },
                    MatchArm {
                        pattern: None,
                        guard: None,
                        expr: Expr::String("unknown".into())
                    }
                ]
            }
        );
        assert_eq!(
            parse_expr(
                r#"match Some(1) { Some(value) if value > 0 => value, _ => 0 }"#,
                1
            )
            .unwrap(),
            Expr::Match {
                value: Box::new(Expr::Some(Box::new(Expr::Int(1)))),
                arms: vec![
                    MatchArm {
                        pattern: Some(Expr::Some(Box::new(Expr::Ident("value".into())))),
                        guard: Some(Expr::Binary {
                            left: Box::new(Expr::Ident("value".into())),
                            op: BinaryOp::Gt,
                            right: Box::new(Expr::Int(0))
                        }),
                        expr: Expr::Ident("value".into())
                    },
                    MatchArm {
                        pattern: None,
                        guard: None,
                        expr: Expr::Int(0)
                    }
                ]
            }
        );
        assert_eq!(
            parse_expr(r#"("host", 8080)"#, 1).unwrap(),
            Expr::Tuple(vec![Expr::String("host".into()), Expr::Int(8080)])
        );
        assert_eq!(
            parse_expr("pair._2", 1).unwrap(),
            Expr::TupleField {
                name: "pair".into(),
                field: 2
            }
        );
        assert_eq!(
            parse_expr(r#"("host", 8080)._1"#, 1).unwrap(),
            Expr::TupleFieldValue {
                value: Box::new(Expr::Tuple(vec![
                    Expr::String("host".into()),
                    Expr::Int(8080)
                ])),
                field: 1
            }
        );
        assert_eq!(
            parse_expr("user.name", 1).unwrap(),
            Expr::Field {
                name: "user".into(),
                field: "name".into()
            }
        );
        assert_eq!(
            parse_expr(r#"({ name: "Ada", age: 36 }).name"#, 1).unwrap(),
            Expr::FieldValue {
                value: Box::new(Expr::Record(vec![
                    ("name".into(), Expr::String("Ada".into())),
                    ("age".into(), Expr::Int(36))
                ])),
                field: "name".into()
            }
        );
        assert_eq!(
            parse_expr("x => x + 1", 1).unwrap(),
            Expr::Lambda {
                params: vec!["x".into()],
                body: Box::new(Expr::Binary {
                    left: Box::new(Expr::Ident("x".into())),
                    op: BinaryOp::Add,
                    right: Box::new(Expr::Int(1))
                })
            }
        );
        assert_eq!(
            parse_expr("(left, right) => left ++ right", 1).unwrap(),
            Expr::Lambda {
                params: vec!["left".into(), "right".into()],
                body: Box::new(Expr::Binary {
                    left: Box::new(Expr::Ident("left".into())),
                    op: BinaryOp::Concat,
                    right: Box::new(Expr::Ident("right".into()))
                })
            }
        );
    }

    #[test]
    fn ignores_operators_inside_strings() {
        let expr = parse_expr(r#""a + b" == "a + b""#, 1).unwrap();
        let (left, op, right) = binary_parts(expr).unwrap();
        assert_eq!(op, BinaryOp::Eq);
        assert_eq!(*left, Expr::String("a + b".into()));
        assert_eq!(*right, Expr::String("a + b".into()));
    }

    #[test]
    fn reports_parse_errors() {
        let cases = [
            ("const = 1", 1, "expected variable name"),
            ("const bad-name = 1", 1, "invalid variable name"),
            (
                "const x = env.home ?? \"/tmp\"",
                1,
                "invalid environment name",
            ),
            ("const x = env.HOME ?? nope", 1, "expected quoted string"),
            ("const x = \"unterminated", 1, "unterminated quoted string"),
            (
                "const x = try $sh\"printf x\".trim(1)",
                1,
                "trim expects no arguments",
            ),
            ("try $sh", 1, "expected quoted string"),
            ("try $sh{ echo nope", 1, "unterminated shell command"),
            (
                "try $sh{ echo } trailing",
                1,
                "unexpected text after shell command",
            ),
            ("require(nope)", 1, "expected quoted string"),
            (
                "requireOneOf([])",
                1,
                "requireOneOf expects at least one command",
            ),
            (
                "requireOneOf([1])",
                1,
                "requireOneOf expects an array of quoted strings",
            ),
            (
                "const [a, ...rest, b] = [1, 2, 3]",
                1,
                "array rest destructuring must be last",
            ),
            ("if 1 {\n$sh'no'\n", 1, "unterminated block"),
            ("while true {\n$sh'no'\n", 1, "unterminated block"),
            (
                "trait Show[T] {\nconst x = 1\n}",
                2,
                "trait bodies support method signatures only",
            ),
            (
                "impl Show[Int] {\nfn show(value: Int): String\n}",
                1,
                "unterminated function header",
            ),
            (
                "fn nope(name) : String {\nreturn name\n}",
                1,
                "function parameter requires type annotation",
            ),
            (
                "fn nope(name: String) {\nreturn name\n}",
                1,
                "expected function return type",
            ),
            ("const x = $sh\"a\" |> nope", 1, "pipeline stages"),
            ("const x = $sh\"a\" |>", 1, "expected pipeline stage"),
            ("$sh\"a\" >> nope", 1, "redirect target"),
            (">> write(\"x\")", 1, "expected redirect source and target"),
            ("const x = if true { 1 }", 1, "requires else branch"),
            ("const x = if true", 1, "expected `{`"),
            ("const x = match 1 { 1 }", 1, "expected `=>`"),
            ("const x = match 1", 1, "expected `{`"),
            ("raw {\necho nope\n", 1, "unterminated raw block"),
            ("try $sh nope", 1, "expected quoted string"),
            ("const x = 0xNOPE", 1, "invalid integer literal"),
            ("const x = 0b102", 1, "invalid integer literal"),
            (
                "const x = \"\"\"\nnope\n",
                1,
                "unterminated multi-line string",
            ),
            ("const x = [1,", 1, "unterminated array literal"),
            ("const x = [1,]", 1, "expected array element"),
            (
                "const x: (String) = \"x\"",
                1,
                "tuple type requires at least two elements",
            ),
            (
                "const x: Map[String] = {}",
                1,
                "Map type requires key and value types",
            ),
            ("const x = {", 1, "unterminated map literal"),
            ("const x = { \"a\" }", 1, "expected `:` in map entry"),
            ("const x = { \"a\": }", 1, "expected map key and value"),
            ("newtype bad = Int", 1, "invalid type name"),
            ("const x = (1,", 1, "unterminated tuple literal"),
            ("not an assignment", 1, "expected assignment"),
        ];

        for (source, line, message) in cases {
            let error = parse(source).unwrap_err();
            assert_eq!(error.line(), line);
            assert!(error.message().contains(message), "{error}");
            assert!(error.to_string().contains(message));
        }
    }

    fn binary_op(expr: Expr) -> Option<BinaryOp> {
        match expr {
            Expr::Binary { op, .. } => Some(op),
            _ => None,
        }
    }

    fn binary_parts(expr: Expr) -> Option<(Box<Expr>, BinaryOp, Box<Expr>)> {
        match expr {
            Expr::Binary { left, op, right } => Some((left, op, right)),
            _ => None,
        }
    }

    #[test]
    fn test_helpers_report_non_binary_expressions() {
        assert_eq!(binary_op(Expr::Int(1)), None);
        assert_eq!(binary_parts(Expr::Int(1)), None);
    }

    #[test]
    fn helper_parsers_report_edge_case_errors() {
        assert_eq!(parse_block_header("if {", "if"), None);
        assert_eq!(parse_for_header("for item in {"), None);
        assert!(parse_function_header("const x = 1", 1).unwrap().is_none());
        assert_error(
            parse_function_header("fn greet(): String", 2),
            "unterminated function header",
        );
        assert_error(
            parse_function_header("fn greet: String {", 3),
            "expected function parameters",
        );
        assert_error(
            parse_function_header("fn greet(name: String {: String {", 4),
            "expected function parameters",
        );
        assert_error(
            parse_function_header("fn greet() {", 5),
            "expected function return type",
        );

        assert!(parse_trait_header("fn nope() {", 6).unwrap().is_none());
        assert_error(
            parse_trait_header("trait Show[T]", 7),
            "unterminated trait header",
        );
        assert_error(
            parse_trait_header("trait Show[T, U] {", 8),
            "trait requires exactly one type parameter",
        );

        assert!(parse_impl_header("trait Show[T] {", 9).unwrap().is_none());
        assert_error(
            parse_impl_header("impl Show[Int]", 10),
            "unterminated impl header",
        );
        assert_error(
            parse_impl_header("impl Show {", 11),
            "expected trait implementation",
        );
        assert_error(
            parse_impl_header("impl Show[Int, String] {", 12),
            "trait implementation requires exactly one type",
        );

        assert!(parse_function_signature("const x = 1", 13)
            .unwrap()
            .is_none());
        assert_error(
            parse_function_signature("fn show(value: T): String {", 14),
            "trait method signatures must not include bodies",
        );
        assert_error(
            parse_function_signature("fn show: String", 15),
            "expected function parameters",
        );
        assert_error(
            parse_function_signature("fn show(value: T", 16),
            "expected function parameters",
        );
        assert_error(
            parse_function_signature("fn show(value: T)", 17),
            "expected function return type",
        );
        assert_error(
            parse_function_header("fn empty[](): String {", 18),
            "expected array element",
        );
        assert_error(parse_type_head("Show", 19), "expected type parameters");
        assert_error(
            parse_type_head("Show[T", 20),
            "unterminated type parameters",
        );
        assert_error(parse_type_alias_name("Box[]", 21), "expected array element");
        assert_error(
            parse_type_alias_name("Box[T", 22),
            "unterminated type parameters",
        );

        assert_error(parse_type("", 23), "expected type name");
        assert_error(parse_type("=> String", 23), "expected function parameter");
        assert_eq!(parse_type("{}", 23).unwrap(), Type::Record(Vec::new()));
        assert_error(parse_type("{ missing }", 24), "expected record field type");
        assert_error(
            parse_type("Map[String]", 25),
            "Map type requires key and value types",
        );
        assert_error(
            parse_type("(String)", 26),
            "tuple type requires at least two elements",
        );
        assert_eq!(
            parse_type("() => String", 27).unwrap(),
            Type::Function(Vec::new(), Box::new(Type::String))
        );
        assert_eq!(
            parse_type("String?", 27).unwrap(),
            Type::Applied("Option".into(), vec![Type::String])
        );
        assert_eq!(
            parse_type(r#"String \/ String"#, 27).unwrap(),
            Type::Applied("Result".into(), vec![Type::String, Type::String])
        );
        assert_eq!(
            parse_type("String | Int", 27).unwrap(),
            Type::Union(vec![Type::String, Type::Int])
        );
        assert_eq!(
            parse_type("String | Int | Bool", 27).unwrap(),
            Type::Union(vec![Type::String, Type::Int, Type::Bool])
        );
        assert_eq!(
            parse_type("String & Path", 27).unwrap(),
            Type::Intersection(vec![Type::String, Type::Path])
        );

        assert_error(
            take_braced_expr(r#"{ "unterminated" "#, 28),
            "unterminated if expression",
        );
        assert_error(
            parse_braced_shell_command("{ echo", 29),
            "unterminated shell command",
        );
        assert_error(
            parse_braced_shell_command("{ echo 'unterminated }", 30),
            "unterminated quoted string in shell command",
        );
        assert_error(
            parse_braced_shell_command("{ echo } trailing", 31),
            "unexpected text after shell command",
        );
        assert_error(parse_raw_quoted("", 32), "expected quoted string");
        assert_error(parse_raw_quoted("nope", 33), "expected quoted string");
        assert_error(
            parse_raw_quoted("'unterminated", 34),
            "unterminated quoted string",
        );

        assert_error(
            split_comma_separated("first,,second", 35),
            "expected array element",
        );
        assert_error(split_comma_separated("]", 36), "unexpected `]`");
        assert_error(split_comma_separated(")", 37), "unexpected `)`");
        assert_error(split_comma_separated("}", 38), "unexpected `}`");
        assert_error(
            split_comma_separated("'unterminated", 39),
            "unterminated quoted string",
        );
        assert_error(
            split_comma_separated("[value", 40),
            "unterminated array literal",
        );
        assert_error(
            split_comma_separated("(value", 41),
            "unterminated tuple literal",
        );
        assert_error(
            split_comma_separated("{value", 42),
            "unterminated map literal",
        );
        assert_error(
            split_comma_separated("value,", 43),
            "expected array element",
        );
        assert_eq!(
            split_comma_separated(r#""a\,b", c"#, 43).unwrap(),
            vec![r#""a\,b""#, "c"]
        );

        assert!(!valid_operator_position("+ 1", 0, 1));
        assert!(!valid_operator_position("1 +", 2, 1));
        assert_eq!(find_assignment_equals(r#"x["="] = "{\""}"#), Some(7));
        assert_eq!(
            parse_builtin_string_call("write", "write", 44).unwrap(),
            None
        );
        assert_eq!(
            parse_builtin_expr_call("pathExists", "pathExists", 45).unwrap(),
            None
        );
        assert_error(parse_module_path("", 45), "expected module path");
        assert_eq!(
            parse_require("require(\"sh\", version = \">= 1\")", 46).unwrap(),
            Some(("sh".into(), Some(">= 1".into())))
        );
        assert_error(
            parse_require("require(\"sh\", \">= 1\")", 46),
            "version must use",
        );
        assert_error(
            parse_require("require(\"sh\", label = \">= 1\")", 46),
            "optional argument must be `version`",
        );
        assert_eq!(parse_require_one_of("requireOneOf", 46).unwrap(), None);
        assert_eq!(parse_for_header("for  in xs {"), None);
        assert_eq!(parse_function_type_params("()", 46).unwrap(), Vec::new());
        assert_eq!(
            parse_function_type_params("(String, Int)", 46).unwrap(),
            vec![Type::String, Type::Int]
        );
        assert_eq!(parse_type_application("[Int]", 46).unwrap(), None);
        assert_eq!(parse_type_application("Box[Int", 46).unwrap(), None);
        assert_error(
            parse_builtin_expr_call("pathExists()", "pathExists", 47),
            "pathExists expects one argument",
        );
        assert_error(
            parse_builtin_expr_call("pathExists(\"a\", \"b\")", "pathExists", 48),
            "pathExists expects one argument",
        );
        assert_error(
            parse_require_one_of("requireOneOf([])", 49),
            "requireOneOf expects at least one command",
        );
        assert_error(
            parse_require_one_of("requireOneOf([1])", 50),
            "requireOneOf expects an array of quoted strings",
        );
        assert_error(
            parse_require_one_of("requireOneOf(\"sh\")", 50),
            "requireOneOf expects an array of quoted strings",
        );
        assert_error(
            parse_redirect("echo nope >> write(\"x\")", 51),
            "redirect source must be a `$sh` command or pipeline",
        );
        assert_error(
            parse_redirect("$sh\"ok\" >> write(\"x\", stdout = \"y\")", 51),
            "optional argument must be `stderr`",
        );
        assert_error(
            parse_if_expr("if true { 1 } else 2", 52),
            "expected `{` in else branch",
        );
        assert_error(
            parse_if_expr("if true { 1 } else { 2 } trailing", 53),
            "unexpected text after if expression",
        );
        assert_error(
            parse_match_expr("match 1 { _ => 1 } trailing", 54),
            "unexpected text after match expression",
        );
        assert_error(
            split_match_arm("=> 1", 55),
            "expected match pattern and expression",
        );
        assert_eq!(split_call("(1)"), None);
        assert_eq!(split_tuple_field("value._0"), None);
        assert_eq!(split_field(".name"), None);
        assert_eq!(split_field("value._name"), None);
        assert_eq!(split_field("value.call()"), None);
        assert_eq!(split_index("[0]"), None);
        assert_eq!(split_index("value[]"), None);
        assert_error(
            split_pipeline("|> $sh\"cat\"", 56),
            "expected pipeline stage",
        );
        assert_eq!(is_bare_record_field(""), false);
        assert_eq!(
            split_map_entry(r#""a:b": [1, 2]"#, 57).unwrap(),
            (r#""a:b""#, "[1, 2]")
        );
        assert_eq!(
            split_map_entry(r#""a\:b": (1, 2)"#, 57).unwrap(),
            (r#""a\:b""#, "(1, 2)")
        );
        assert_eq!(
            find_top_level_char(r#""\{" + [1, 2] + { a: "(" } + target"#, '+'),
            Some(5)
        );
        assert_eq!(find_top_level_arrow(r#""\=>" => value"#), Some(6));
        assert_eq!(
            take_braced_expr(r#"{ "\}" }rest"#, 57).unwrap(),
            (r#" "\}" "#, "rest")
        );
        assert_eq!(
            parse_braced_shell_command(r#"{ echo "ok" }"#, 57).unwrap(),
            r#"echo "ok""#
        );
        assert_eq!(
            parse_expr("r'raw'", 57).unwrap(),
            Expr::RawString("raw".into())
        );

        let mut raw_lines = "unterminated\n".lines().enumerate().peekable();
        assert_error(collect_block(&mut raw_lines, 58), "unterminated block");
    }

    #[test]
    fn parses_split_else_blocks() {
        let program = parse(
            r#"
if true {
$sh"then"
}
else {
$sh"else"
}
"#,
        )
        .unwrap();

        assert!(matches!(
            program.statements()[0],
            Statement::If {
                else_branch: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn parses_block_statements() {
        let program = parse(
            r#"
{
const label = "inside"
$sh"printf ${label}"
}
"#,
        )
        .unwrap();

        assert!(matches!(
            &program.statements()[0],
            Statement::Block { body } if body.statements().len() == 2
        ));
    }

    #[test]
    fn parses_else_if_blocks() {
        let program = parse(
            r#"
if count == 0 {
$sh"zero"
} else if count == 1 {
$sh"one"
} else if count == 2 {
$sh"two"
} else {
$sh"many"
}
"#,
        )
        .unwrap();

        let Statement::If {
            else_branch: Some(first_else),
            ..
        } = &program.statements()[0]
        else {
            panic!("expected if statement with else branch");
        };
        let Statement::If {
            else_branch: Some(second_else),
            ..
        } = &first_else.statements()[0]
        else {
            panic!("expected nested else-if statement");
        };
        assert!(matches!(
            second_else.statements()[0],
            Statement::If {
                else_branch: Some(_),
                ..
            }
        ));
    }

    fn assert_error<T>(result: Result<T, CompileError>, message: &str) {
        assert!(result.is_err());
        let error = result.err().unwrap();
        assert!(
            error.message().contains(message),
            "expected `{}` to contain `{}`",
            error.message(),
            message
        );
    }
}
