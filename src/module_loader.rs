mod namespace;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{parse, CompileError, Program, Statement};

pub(crate) fn load_program(path: &Path) -> Result<Program, CompileError> {
    let resolver = DependencyResolver::discover(path)?;
    let mut seen = HashSet::new();
    parse_file_expanded(path, &resolver, &mut seen)
}

pub(crate) fn write_lockfile_for(path: &Path) -> Result<(), CompileError> {
    let Some(manifest) = find_manifest(path.parent().unwrap_or_else(|| Path::new("."))) else {
        return Err(CompileError::new(
            0,
            "cannot write lockfile without nacre.toml".to_string(),
        ));
    };
    let resolver = DependencyResolver::from_manifest_unlocked(&manifest)?;
    let lock = Lockfile::from_dependencies(&resolver.dependencies)?;
    let lock_path = manifest
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("nacre.lock");
    fs::write(&lock_path, lock.to_toml()).map_err(|error| {
        CompileError::new(
            0,
            format!("failed to write {}: {error}", lock_path.display()),
        )
    })
}

#[derive(Default)]
struct DependencyResolver {
    dependencies: HashMap<String, PathBuf>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageManifest {
    #[serde(default)]
    package: Option<PackageMetadata>,
    #[serde(default)]
    dependencies: BTreeMap<String, PackageDependency>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageMetadata {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    version: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageDependency {
    path: PathBuf,
}

impl DependencyResolver {
    fn discover(input: &Path) -> Result<Self, CompileError> {
        let Some(manifest) = find_manifest(input.parent().unwrap_or_else(|| Path::new("."))) else {
            return Ok(Self::default());
        };
        Self::from_manifest(&manifest)
    }

    fn from_manifest(manifest: &Path) -> Result<Self, CompileError> {
        let resolver = Self::from_manifest_unlocked(manifest)?;
        resolver.validate_lockfile(manifest)?;
        Ok(resolver)
    }

    fn from_manifest_unlocked(manifest: &Path) -> Result<Self, CompileError> {
        let source = fs::read_to_string(manifest).map_err(|error| {
            CompileError::new(0, format!("failed to read {}: {error}", manifest.display()))
        })?;
        let manifest_value = toml::from_str::<PackageManifest>(&source).map_err(|error| {
            CompileError::new(
                0,
                format!("failed to parse {}: {error}", manifest.display()),
            )
        })?;
        if let Some(package) = &manifest_value.package {
            if let Some(name) = &package.name {
                if !is_package_name(name) || name == "std" {
                    return Err(CompileError::new(
                        0,
                        format!("invalid package name `{name}` in {}", manifest.display()),
                    ));
                }
            }
            if package.version.as_deref().is_some_and(str::is_empty) {
                return Err(CompileError::new(
                    0,
                    format!(
                        "package version must not be empty in {}",
                        manifest.display()
                    ),
                ));
            }
        }
        let manifest_dir = manifest.parent().unwrap_or_else(|| Path::new("."));
        let mut dependencies = HashMap::new();
        for (name, dependency) in manifest_value.dependencies {
            if !is_package_name(&name) || name == "std" {
                return Err(CompileError::new(
                    0,
                    format!("invalid dependency name `{name}` in {}", manifest.display()),
                ));
            }
            let root = manifest_dir.join(dependency.path);
            if !root.is_dir() {
                return Err(CompileError::new(
                    0,
                    format!(
                        "dependency `{name}` path is not a directory: {}",
                        root.display()
                    ),
                ));
            }
            dependencies.insert(
                name,
                fs::canonicalize(&root).unwrap_or_else(|_| root.to_path_buf()),
            );
        }
        Ok(Self { dependencies })
    }

    fn validate_lockfile(&self, manifest: &Path) -> Result<(), CompileError> {
        let lock_path = manifest
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("nacre.lock");
        if !lock_path.is_file() {
            return Ok(());
        }
        let source = fs::read_to_string(&lock_path).map_err(|error| {
            CompileError::new(
                0,
                format!("failed to read {}: {error}", lock_path.display()),
            )
        })?;
        let lock = toml::from_str::<Lockfile>(&source).map_err(|error| {
            CompileError::new(
                0,
                format!("failed to parse {}: {error}", lock_path.display()),
            )
        })?;
        for (name, root) in &self.dependencies {
            let Some(package) = lock.package.iter().find(|package| package.name == *name) else {
                return Err(CompileError::new(
                    0,
                    format!(
                        "dependency `{name}` is missing from {}",
                        lock_path.display()
                    ),
                ));
            };
            let actual = LockedPackage::from_dependency(name, root)?;
            if package != &actual {
                return Err(CompileError::new(
                    0,
                    format!(
                        "dependency `{name}` does not match {}; run `nacre --write-lock <input.ncr>`",
                        lock_path.display()
                    ),
                ));
            }
        }
        Ok(())
    }

    fn dependency_root(&self, name: &str) -> Option<&Path> {
        self.dependencies.get(name).map(PathBuf::as_path)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Lockfile {
    #[serde(default)]
    package: Vec<LockedPackage>,
}

impl Lockfile {
    fn from_dependencies(dependencies: &HashMap<String, PathBuf>) -> Result<Self, CompileError> {
        let mut package = dependencies
            .iter()
            .map(|(name, root)| LockedPackage::from_dependency(name, root))
            .collect::<Result<Vec<_>, CompileError>>()?;
        package.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(Self { package })
    }

    fn to_toml(&self) -> String {
        let mut output = String::from("# This file is generated by nacre --write-lock.\n");
        for package in &self.package {
            output.push_str("\n[[package]]\n");
            output.push_str(&format!("name = \"{}\"\n", toml_escape(&package.name)));
            output.push_str("source = \"path\"\n");
            output.push_str(&format!("path = \"{}\"\n", toml_escape(&package.path)));
            output.push_str(&format!(
                "fingerprint = \"{}\"\n",
                toml_escape(&package.fingerprint)
            ));
        }
        output
    }
}

fn toml_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct LockedPackage {
    name: String,
    source: String,
    path: String,
    fingerprint: String,
}

impl LockedPackage {
    fn from_dependency(name: &str, root: &Path) -> Result<Self, CompileError> {
        Ok(Self {
            name: name.to_string(),
            source: "path".to_string(),
            path: fs::canonicalize(root)
                .unwrap_or_else(|_| root.to_path_buf())
                .to_string_lossy()
                .to_string(),
            fingerprint: dependency_fingerprint(root)?,
        })
    }
}

fn dependency_fingerprint(root: &Path) -> Result<String, CompileError> {
    let mut files = Vec::new();
    collect_nacre_files(root, root, &mut files)?;
    files.sort();
    let mut hash = 0xcbf29ce484222325u64;
    for (relative, source) in files {
        for byte in relative.as_bytes().iter().chain(source.as_bytes()) {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    Ok(format!("{hash:016x}"))
}

fn collect_nacre_files(
    root: &Path,
    dir: &Path,
    files: &mut Vec<(String, String)>,
) -> Result<(), CompileError> {
    for entry in fs::read_dir(dir).map_err(|error| {
        CompileError::new(
            0,
            format!(
                "failed to read dependency directory {}: {error}",
                dir.display()
            ),
        )
    })? {
        let entry = entry.map_err(|error| {
            CompileError::new(
                0,
                format!(
                    "failed to read dependency directory {}: {error}",
                    dir.display()
                ),
            )
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_nacre_files(root, &path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "ncr") {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let source = fs::read_to_string(&path).map_err(|error| {
                CompileError::new(0, format!("failed to read {}: {error}", path.display()))
            })?;
            files.push((relative, source));
        }
    }
    Ok(())
}

fn find_manifest(start: &Path) -> Option<PathBuf> {
    for dir in start.ancestors() {
        let manifest = dir.join("nacre.toml");
        if manifest.is_file() {
            return Some(manifest);
        }
    }
    None
}

fn is_package_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn parse_file_expanded(
    path: &Path,
    resolver: &DependencyResolver,
    seen: &mut HashSet<PathBuf>,
) -> Result<Program, CompileError> {
    let source = fs::read_to_string(path).map_err(|error| {
        CompileError::new(0, format!("failed to read {}: {error}", path.display()))
    })?;
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if !seen.insert(canonical) {
        return Ok(Program::new(Vec::new(), Vec::new()));
    }
    let program = parse(&source)
        .map_err(|error| error.with_source_context(path.display().to_string(), &source))?;
    expand_modules(
        program,
        path.parent().unwrap_or_else(|| Path::new(".")),
        resolver,
        seen,
    )
}

fn expand_modules(
    program: Program,
    base_dir: &Path,
    resolver: &DependencyResolver,
    seen: &mut HashSet<PathBuf>,
) -> Result<Program, CompileError> {
    let mut statements = Vec::new();
    let mut lines = Vec::new();
    for (statement, line) in program.statements().iter().zip(program.statement_lines()) {
        if let Statement::Use { path } = statement {
            let module_path = resolve_module_path_with_deps(base_dir, path, *line, resolver)?;
            let module = parse_file_expanded(&module_path, resolver, seen)?;
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

#[cfg(test)]
fn resolve_module_path(
    base_dir: &Path,
    parts: &[String],
    line: usize,
) -> Result<PathBuf, CompileError> {
    resolve_module_path_with_deps(base_dir, parts, line, &DependencyResolver::default())
}

fn resolve_module_path_with_deps(
    base_dir: &Path,
    parts: &[String],
    line: usize,
    resolver: &DependencyResolver,
) -> Result<PathBuf, CompileError> {
    if let Some(package) = parts.first() {
        if let Some(root) = resolver.dependency_root(package) {
            return resolve_dependency_module_path(package, root, &parts[1..], parts, line);
        }
    }
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

fn resolve_dependency_module_path(
    package: &str,
    root: &Path,
    parts: &[String],
    full_parts: &[String],
    line: usize,
) -> Result<PathBuf, CompileError> {
    if parts.is_empty() {
        let index = root.join("index.ncr");
        if index.is_file() {
            return Ok(index);
        }
    } else if let Some(path) = resolve_module_under_root(root, parts) {
        return Ok(path);
    }
    Err(CompileError::new(
        line,
        format!(
            "module `{}` was not found in dependency `{package}`",
            full_parts.join(".")
        ),
    ))
}

fn resolve_module_under_root(root: &Path, parts: &[String]) -> Option<PathBuf> {
    let relative = parts.iter().collect::<PathBuf>();
    let file = root.join(&relative).with_extension("ncr");
    if file.is_file() {
        return Some(file);
    }
    let definition = root.join(&relative).with_extension("d.ncr");
    if definition.is_file() {
        return Some(definition);
    }
    let index = root.join(&relative).join("index.ncr");
    if index.is_file() {
        return Some(index);
    }
    None
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use crate::compile_file;
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

    #[test]
    fn compile_file_resolves_local_package_dependencies() {
        let root = temp_path("package-deps");
        let app = root.join("app");
        let tools = root.join("tools");
        fs::create_dir_all(&app).unwrap();
        fs::create_dir_all(&tools).unwrap();
        fs::write(
            app.join("nacre.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies.tools]\npath = \"../tools\"\n",
        )
        .unwrap();
        fs::write(
            tools.join("format.ncr"),
            "fn label(value: String): String {\nreturn \"tool:${value}\"\n}\n",
        )
        .unwrap();
        let main = app.join("main.ncr");
        fs::write(
            &main,
            "use tools.format\nconst result = format.label(\"ok\")\n",
        )
        .unwrap();

        let program = load_program(&main).unwrap();
        assert!(program.statements().iter().any(|statement| matches!(
            statement,
            Statement::Function { name, .. } if name == "format.label"
        )));
        compile_file(&main).unwrap();

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_local_package_module_reports_dependency_name() {
        let root = temp_path("missing-package-module");
        let app = root.join("app");
        let tools = root.join("tools");
        fs::create_dir_all(&app).unwrap();
        fs::create_dir_all(&tools).unwrap();
        fs::write(
            app.join("nacre.toml"),
            "[dependencies.tools]\npath = \"../tools\"\n",
        )
        .unwrap();
        let main = app.join("main.ncr");
        fs::write(&main, "use tools.missing\n").unwrap();

        let error = load_program(&main).unwrap_err();
        assert_eq!(error.line(), 1);
        assert!(error
            .message()
            .contains("module `tools.missing` was not found in dependency `tools`"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_local_package_dependency_reports_manifest_error() {
        let root = temp_path("invalid-package-dep");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("nacre.toml"),
            "[dependencies.tools]\npath = \"missing\"\n",
        )
        .unwrap();
        let main = root.join("main.ncr");
        fs::write(&main, "").unwrap();

        let error = load_program(&main).unwrap_err();
        assert_eq!(error.line(), 0);
        assert!(error
            .message()
            .contains("dependency `tools` path is not a directory"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn compile_file_parse_error_keeps_source_context() {
        let root = temp_path("parse-context");
        fs::create_dir_all(&root).unwrap();
        let main = root.join("main.ncr");
        fs::write(&main, "const bad-name = 1\n").unwrap();

        let error = compile_file(&main).unwrap_err();
        let source_name = main.display().to_string();
        assert_eq!(error.line(), 1);
        assert_eq!(error.source_name(), Some(source_name.as_str()));
        assert_eq!(error.source_line(), Some("const bad-name = 1"));
        assert!(error.to_string().contains("^"));

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
