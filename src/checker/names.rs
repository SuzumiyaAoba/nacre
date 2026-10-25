use std::collections::{HashMap, HashSet};

use crate::symbols::{is_identifier, sanitize_symbol};
use crate::{BindingPattern, ForBinding, Program, Statement, Type};

pub(super) fn method_call_parts(name: &str) -> Option<(&str, &str)> {
    let (receiver, method) = name.rsplit_once('.')?;
    if is_identifier(receiver) && is_identifier(method) {
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

pub(super) fn collect_declared_names(program: &Program, names: &mut HashSet<String>) {
    for statement in program.statements() {
        match statement {
            Statement::Function {
                name, params, body, ..
            } => {
                names.insert(name.clone());
                names.extend(params.iter().map(|param| param.name.clone()));
                collect_declared_names(body, names);
            }
            Statement::ExternalFunction { name, params, .. } => {
                names.insert(name.clone());
                names.extend(params.iter().map(|param| param.name.clone()));
            }
            Statement::Impl { methods, .. } => {
                for method in methods {
                    names.insert(method.name.clone());
                    names.extend(method.params.iter().map(|param| param.name.clone()));
                    collect_declared_names(&method.body, names);
                }
            }
            Statement::Const { name, .. } | Statement::Let { name, .. } => {
                names.insert(name.clone());
            }
            Statement::Destructure { pattern, .. } => collect_binding_pattern_names(pattern, names),
            Statement::Block { body } | Statement::While { body, .. } => {
                collect_declared_names(body, names);
            }
            Statement::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_declared_names(then_branch, names);
                if let Some(else_branch) = else_branch {
                    collect_declared_names(else_branch, names);
                }
            }
            Statement::For { binding, body, .. } => {
                collect_for_binding_names(binding, names);
                collect_declared_names(body, names);
            }
            _ => {}
        }
    }
}

pub(super) fn collect_for_binding_names(binding: &ForBinding, names: &mut HashSet<String>) {
    match binding {
        ForBinding::Name(name) => {
            names.insert(name.clone());
        }
        ForBinding::Pattern(pattern) => collect_binding_pattern_names(pattern, names),
    }
}

pub(super) fn collect_binding_pattern_names(pattern: &BindingPattern, names: &mut HashSet<String>) {
    match pattern {
        BindingPattern::Name(name) => {
            if name != "_" {
                names.insert(name.clone());
            }
        }
        BindingPattern::Tuple(values) => {
            for value in values {
                collect_binding_pattern_names(value, names);
            }
        }
        BindingPattern::Record(fields) => {
            for (_, binding) in fields {
                collect_binding_pattern_names(binding, names);
            }
        }
        BindingPattern::Array { patterns, rest } => {
            for value in patterns {
                collect_binding_pattern_names(value, names);
            }
            names.extend(rest.iter().cloned());
        }
    }
}

pub(super) fn argument_count(count: usize) -> String {
    match count {
        1 => "1 argument".to_string(),
        other => format!("{other} arguments"),
    }
}

pub(super) fn module_binding_name(module: &str, name: &str) -> String {
    format!("{module}_{name}")
}

pub(super) fn associated_const_name(type_name: &str, name: &str) -> String {
    format!("{}_{}", sanitize_symbol(type_name), name)
}

pub(super) fn associated_method_name(type_name: &str, name: &str) -> String {
    format!("{type_name}.{name}")
}

pub(super) fn applied_nominal_name(
    name: &str,
    type_params: &[String],
    inferred: &HashMap<String, Type>,
) -> String {
    let args = type_params
        .iter()
        .filter_map(|param| inferred.get(param))
        .map(Type::name)
        .collect::<Vec<_>>()
        .join(", ");
    format!("{name}[{args}]")
}
