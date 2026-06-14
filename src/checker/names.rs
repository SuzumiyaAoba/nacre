use crate::Type;

pub(super) fn is_valid_name(input: &str) -> bool {
    let mut chars = input.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

pub(super) fn method_call_parts(name: &str) -> Option<(&str, &str)> {
    let (receiver, method) = name.rsplit_once('.')?;
    if is_valid_name(receiver) && is_valid_name(method) {
        Some((receiver, method))
    } else {
        None
    }
}

pub(super) fn impl_method_name(trait_name: &str, for_type: &Type, method: &str) -> String {
    format!(
        "__nacre_trait_{}_{}_{}",
        sanitize_symbol(trait_name),
        sanitize_symbol(&for_type.name()),
        sanitize_symbol(method)
    )
}

pub(super) fn sanitize_symbol(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch == '_' || ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    out
}

pub(super) fn backend_function_name(name: &str) -> String {
    if matches!(
        name,
        "if" | "then"
            | "else"
            | "elif"
            | "fi"
            | "case"
            | "esac"
            | "for"
            | "select"
            | "while"
            | "until"
            | "do"
            | "done"
            | "in"
            | "function"
            | "time"
            | "coproc"
    ) {
        format!("__nacre_keyword_{name}")
    } else {
        name.to_string()
    }
}
