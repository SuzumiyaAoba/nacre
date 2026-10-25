use crate::symbols::is_identifier;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::CompileError;

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
pub(super) struct DependencyResolver {
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
    pub(super) fn discover(input: &Path) -> Result<Self, CompileError> {
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
                if !is_identifier(name) || name == "std" {
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
            if !is_identifier(&name) || name == "std" {
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

    pub(super) fn dependency_root(&self, name: &str) -> Option<&Path> {
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
