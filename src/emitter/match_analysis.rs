use crate::{Expr, MatchArm};

pub(super) fn tuple_match_width(arms: &[MatchArm]) -> Option<usize> {
    arms.iter().find_map(|arm| match &arm.pattern {
        Some(Expr::Tuple(patterns)) => Some(patterns.len()),
        _ => None,
    })
}

pub(super) fn variant_match_width(arms: &[MatchArm]) -> Option<usize> {
    arms.iter()
        .filter_map(|arm| match arm.pattern.as_ref() {
            Some(Expr::Variant { args, .. }) => Some(args.len()),
            _ => None,
        })
        .max()
}

pub(super) fn constructor_tuple_match_width(arms: &[MatchArm]) -> Option<usize> {
    arms.iter().find_map(|arm| match &arm.pattern {
        Some(Expr::Some(payload)) | Some(Expr::Ok(payload)) | Some(Expr::Err(payload)) => {
            match payload.as_ref() {
                Expr::Tuple(patterns) => Some(patterns.len()),
                _ => None,
            }
        }
        _ => None,
    })
}

pub(super) fn has_constructor_match_pattern(arms: &[MatchArm]) -> bool {
    arms.iter().any(|arm| {
        matches!(
            arm.pattern,
            Some(Expr::Some(_)) | Some(Expr::Ok(_)) | Some(Expr::Err(_)) | Some(Expr::None)
        )
    })
}

pub(super) fn record_match_fields(arms: &[MatchArm]) -> Option<Vec<String>> {
    let mut fields = Vec::new();
    for arm in arms {
        if let Some(Expr::RecordPattern(patterns)) = &arm.pattern {
            for (field, _) in patterns {
                if !fields.contains(field) {
                    fields.push(field.clone());
                }
            }
        }
    }
    (!fields.is_empty()).then_some(fields)
}

pub(super) fn constructor_record_match_fields(arms: &[MatchArm]) -> Option<Vec<String>> {
    let mut fields = Vec::new();
    for arm in arms {
        let patterns = match &arm.pattern {
            Some(Expr::Some(payload)) | Some(Expr::Ok(payload)) | Some(Expr::Err(payload)) => {
                match payload.as_ref() {
                    Expr::RecordPattern(patterns) => patterns,
                    _ => continue,
                }
            }
            _ => continue,
        };
        for (field, _) in patterns {
            if !fields.contains(field) {
                fields.push(field.clone());
            }
        }
    }
    (!fields.is_empty()).then_some(fields)
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    fn arm(pattern: Expr) -> MatchArm {
        MatchArm {
            pattern: Some(pattern),
            guard: None,
            expr: Expr::Unit,
        }
    }

    #[test]
    fn derives_match_storage_shapes() {
        let tuple = vec![arm(Expr::Tuple(vec![
            Expr::Int(1),
            Expr::Ident("_".into()),
        ]))];
        assert_eq!(tuple_match_width(&tuple), Some(2));

        let record = vec![arm(Expr::RecordPattern(vec![
            ("name".into(), None),
            ("age".into(), Some(Expr::Ident("_".into()))),
        ]))];
        assert_eq!(
            record_match_fields(&record),
            Some(vec!["name".to_string(), "age".to_string()])
        );

        let constructor = vec![arm(Expr::Some(Box::new(Expr::Tuple(vec![Expr::Ident(
            "value".into(),
        )]))))];
        assert!(has_constructor_match_pattern(&constructor));
        assert_eq!(constructor_tuple_match_width(&constructor), Some(1));
    }
}
