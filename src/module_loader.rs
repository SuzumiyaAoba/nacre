mod namespace;

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{parse, CompileError, Program, Statement};

pub(crate) fn load_program(path: &Path) -> Result<Program, CompileError> {
    let mut seen = HashSet::new();
    parse_file_expanded(path, &mut seen)
}

fn parse_file_expanded(path: &Path, seen: &mut HashSet<PathBuf>) -> Result<Program, CompileError> {
    let source = fs::read_to_string(path).map_err(|error| {
        CompileError::new(0, format!("failed to read {}: {error}", path.display()))
    })?;
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if !seen.insert(canonical) {
        return Ok(Program::new(Vec::new(), Vec::new()));
    }
    let program = parse(&source)?;
    expand_modules(
        program,
        path.parent().unwrap_or_else(|| Path::new(".")),
        seen,
    )
}

fn expand_modules(
    program: Program,
    base_dir: &Path,
    seen: &mut HashSet<PathBuf>,
) -> Result<Program, CompileError> {
    let mut statements = Vec::new();
    let mut lines = Vec::new();
    for (statement, line) in program.statements().iter().zip(program.statement_lines()) {
        if let Statement::Use { path } = statement {
            let module_path = resolve_module_path(base_dir, path, *line)?;
            let module = parse_file_expanded(&module_path, seen)?;
            let Some(namespace) = path.last() else {
                return Err(CompileError::new(
                    *line,
                    "module path cannot be empty".to_string(),
                ));
            };
            let module = namespace::namespace_module(module, namespace);
            statements.extend_from_slice(module.statements());
            lines.extend_from_slice(module.statement_lines());
        } else {
            statements.push(statement.clone());
            lines.push(*line);
        }
    }
    Ok(Program::new(statements, lines))
}

fn resolve_module_path(
    base_dir: &Path,
    parts: &[String],
    line: usize,
) -> Result<PathBuf, CompileError> {
    let relative = parts.iter().collect::<PathBuf>();
    let mut roots = vec![base_dir.to_path_buf()];
    if parts.first().is_some_and(|part| part == "std") {
        roots.push(Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf());
    }
    for root in roots {
        let file = root.join(&relative).with_extension("ncr");
        if file.is_file() {
            return Ok(file);
        }
        let definition = root.join(&relative).with_extension("d.ncr");
        if definition.is_file() {
            return Ok(definition);
        }
        let index = root.join(&relative).join("index.ncr");
        if index.is_file() {
            return Ok(index);
        }
    }
    Err(CompileError::new(
        line,
        format!("module `{}` was not found", parts.join(".")),
    ))
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn resolves_supported_module_file_layouts() {
        let root = temp_path("module-layouts");
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("direct.ncr"), "").unwrap();
        fs::write(root.join("definition.d.ncr"), "").unwrap();
        fs::write(root.join("nested/index.ncr"), "").unwrap();

        assert_eq!(
            resolve_module_path(&root, &["direct".into()], 1).unwrap(),
            root.join("direct.ncr")
        );
        assert_eq!(
            resolve_module_path(&root, &["definition".into()], 1).unwrap(),
            root.join("definition.d.ncr")
        );
        assert_eq!(
            resolve_module_path(&root, &["nested".into()], 1).unwrap(),
            root.join("nested/index.ncr")
        );

        let error = resolve_module_path(&root, &["missing".into()], 7).unwrap_err();
        assert_eq!(error.line(), 7);
        assert!(error.message().contains("module `missing` was not found"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolves_only_local_modules_and_bundled_std_modules() {
        let root = temp_path("module-roots");
        fs::create_dir_all(&root).unwrap();

        assert_eq!(
            resolve_module_path(&root, &["std".into(), "path".into()], 1).unwrap(),
            Path::new(env!("CARGO_MANIFEST_DIR")).join("std/path.ncr")
        );

        let error = resolve_module_path(
            &root,
            &["docs".into(), "examples".into(), "hello".into()],
            9,
        )
        .unwrap_err();
        assert_eq!(error.line(), 9);
        assert!(error
            .message()
            .contains("module `docs.examples.hello` was not found"));

        fs::remove_dir_all(root).unwrap();
    }

    fn temp_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("nacre-{unique}-{name}"))
    }
}
