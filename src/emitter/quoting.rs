pub(super) fn emit_awk_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('"');
}

pub(super) fn emit_string(out: &mut String, value: &str) {
    if value.contains("${") {
        emit_interpolated_string(out, value);
    } else {
        emit_bash_string(out, value);
    }
}

pub(super) fn emit_interpolated_string(out: &mut String, value: &str) {
    out.push('"');
    let mut rest = value;
    while let Some(start) = rest.find("${") {
        emit_double_quoted_literal_segment(out, &rest[..start]);
        let after_start = &rest[start + 2..];
        let end = after_start
            .find('}')
            .expect("string interpolation was validated before emission");
        emit_interpolation_ref(out, &after_start[..end]);
        rest = &after_start[end + 1..];
    }
    emit_double_quoted_literal_segment(out, rest);
    out.push('"');
}

fn emit_interpolation_ref(out: &mut String, value: &str) {
    out.push_str("${");
    if let Some((base, index)) = value
        .strip_suffix(']')
        .and_then(|value| value.split_once('['))
    {
        out.push_str(base);
        out.push('[');
        out.push_str(index);
        out.push(']');
    } else if let Some((base, field)) = value.split_once('.') {
        out.push_str(base);
        if !field.starts_with('_') {
            out.push('_');
        }
        out.push_str(field);
    } else {
        out.push_str(value);
    }
    out.push('}');
}

fn emit_double_quoted_literal_segment(out: &mut String, value: &str) {
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '`' => out.push_str("\\`"),
            '$' => out.push_str("\\$"),
            _ => out.push(ch),
        }
    }
}

pub(super) fn emit_bash_string(out: &mut String, value: &str) {
    emit_shell_word(out, value);
}

pub(super) fn emit_shell_word(out: &mut String, value: &str) {
    out.push('\'');
    for ch in value.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    #[test]
    fn quotes_shell_awk_and_interpolated_strings() {
        let mut out = String::new();
        emit_shell_word(&mut out, "a'b");
        assert_eq!(out, "'a'\\''b'");

        out.clear();
        emit_awk_string(&mut out, "a\"\\\n\r\t");
        assert_eq!(out, r#""a\"\\\n\r\t""#);

        out.clear();
        emit_interpolated_string(&mut out, "hello ${name}\"\\`");
        assert_eq!(out, "\"hello ${name}\\\"\\\\\\`\"");

        out.clear();
        emit_interpolated_string(&mut out, "${user.name}:${pair._1}:${items[0]}");
        assert_eq!(out, "\"${user_name}:${pair_1}:${items[0]}\"");
    }
}
