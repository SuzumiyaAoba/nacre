use crate::Expr;

use super::emit_expr;

pub(super) fn emit_pipeline_capture(out: &mut String, input: Option<&Expr>, commands: &[String]) {
    out.push_str("\"$(");
    emit_pipeline(out, input, commands);
    out.push_str(")\"");
}

pub(super) fn emit_pipeline(out: &mut String, input: Option<&Expr>, commands: &[String]) {
    if let Some(input) = input {
        out.push_str("printf '%s' ");
        emit_expr(out, input);
        if !commands.is_empty() {
            out.push_str(" | ");
        }
    }
    for (index, command) in commands.iter().enumerate() {
        if index > 0 {
            out.push_str(" | ");
        }
        emit_shell_command(out, command);
    }
}

pub(super) fn emit_shell_command(out: &mut String, command: &str) {
    if !contains_shell_heredoc(command) {
        out.push_str(command);
        return;
    }

    out.push_str("{\n");
    out.push_str(command);
    if !command.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("} ");
}

fn contains_shell_heredoc(command: &str) -> bool {
    let lines = command.lines().collect::<Vec<_>>();
    for (index, line) in lines.iter().enumerate() {
        let Some((delimiter, strip_tabs)) = shell_heredoc_delimiter(line) else {
            continue;
        };
        if lines[index + 1..].iter().any(|line| {
            let line = if strip_tabs {
                line.trim_start_matches('\t')
            } else {
                line
            };
            *line == delimiter
        }) {
            return true;
        }
    }
    false
}

fn shell_heredoc_delimiter(line: &str) -> Option<(String, bool)> {
    let mut quote = None;
    let mut escaped = false;
    let chars = line.char_indices().peekable();
    for (index, ch) in chars {
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
        if ch != '<'
            || !line[index..].starts_with("<<")
            || line[index..].starts_with("<<<")
            || (index > 0 && line.as_bytes()[index - 1] == b'<')
        {
            continue;
        }

        let mut rest = &line[index + 2..];
        let strip_tabs = rest.starts_with('-');
        if strip_tabs {
            rest = &rest[1..];
        }
        rest = rest.trim_start();
        let first = rest.chars().next()?;
        let delimiter = if first == '"' || first == '\'' {
            let end = rest[1..].find(first)?;
            &rest[1..end + 1]
        } else {
            rest.split(|ch: char| ch.is_whitespace() || ";|&<>".contains(ch))
                .next()?
        };
        if !delimiter.is_empty() {
            return Some((delimiter.to_string(), strip_tabs));
        }
    }
    None
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    #[test]
    fn detects_supported_heredoc_forms() {
        assert_eq!(
            shell_heredoc_delimiter("cat <<EOF"),
            Some(("EOF".to_string(), false))
        );
        assert_eq!(
            shell_heredoc_delimiter("cat <<-'EOF'"),
            Some(("EOF".to_string(), true))
        );
        assert_eq!(shell_heredoc_delimiter("cat <<< value"), None);
        assert!(contains_shell_heredoc("cat <<EOF\nvalue\nEOF\n"));
        assert!(contains_shell_heredoc("cat <<-EOF\n\tvalue\n\tEOF\n"));
        assert!(!contains_shell_heredoc("cat <<< value\nvalue\n"));
        assert!(!contains_shell_heredoc("printf '<<EOF'\n"));
    }

    #[test]
    fn wraps_heredoc_commands_for_pipeline_safety() {
        let mut out = String::new();
        emit_shell_command(&mut out, "printf ok");
        assert_eq!(out, "printf ok");

        out.clear();
        emit_shell_command(&mut out, "cat <<EOF\nvalue\nEOF");
        assert_eq!(out, "{\ncat <<EOF\nvalue\nEOF\n} ");
    }
}
