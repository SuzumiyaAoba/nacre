use crate::Type;

pub(super) fn value_suffixes(ty: &Type) -> Vec<String> {
    fn append(prefix: &str, ty: &Type, suffixes: &mut Vec<String>) {
        match ty {
            Type::Record(fields) => {
                for (field, ty) in fields {
                    append(&format!("{prefix}_{field}"), ty, suffixes);
                }
            }
            Type::Tuple(fields) => {
                for (index, ty) in fields.iter().enumerate() {
                    append(&format!("{prefix}_{}", index + 1), ty, suffixes);
                }
            }
            Type::Applied(name, args) if name == "Option" || name == "Result" => {
                suffixes.push(prefix.to_string());
                for ty in args {
                    if !is_scalar_backed_type(ty) {
                        append(prefix, ty, suffixes);
                    }
                }
            }
            Type::Brand { base, .. } => append(prefix, base, suffixes),
            Type::Union(types) | Type::Intersection(types) => {
                for ty in types {
                    append(prefix, ty, suffixes);
                }
            }
            _ => suffixes.push(prefix.to_string()),
        }
    }

    let mut suffixes = Vec::new();
    append("", ty, &mut suffixes);
    suffixes.sort();
    suffixes.dedup();
    suffixes
}

pub(super) fn is_scalar_backed_type(ty: &Type) -> bool {
    match ty {
        Type::Array(_) | Type::Map(_, _) | Type::Record(_) | Type::Tuple(_) => false,
        Type::Applied(_, args) | Type::Union(args) | Type::Intersection(args) => {
            args.iter().all(is_scalar_backed_type)
        }
        Type::Brand { base, .. } => is_scalar_backed_type(base),
        Type::Generic(_) | Type::Named(_) => false,
        Type::Int
        | Type::Float
        | Type::Bool
        | Type::String
        | Type::Path
        | Type::ExitCode
        | Type::Unit
        | Type::Future(_)
        | Type::Function(_, _) => true,
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    #[test]
    fn computes_storage_suffixes_for_structured_values() {
        let ty = Type::Record(vec![
            ("name".into(), Type::String),
            ("position".into(), Type::Tuple(vec![Type::Int, Type::Int])),
        ]);

        assert_eq!(
            value_suffixes(&ty),
            vec![
                "_name".to_string(),
                "_position_1".to_string(),
                "_position_2".to_string()
            ]
        );
        assert!(!is_scalar_backed_type(&ty));
        assert!(is_scalar_backed_type(&Type::String));
    }
}
