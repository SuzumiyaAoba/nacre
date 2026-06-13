use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::CompileError;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecutionPolicy {
    read_roots: Vec<PathBuf>,
    write_roots: Vec<PathBuf>,
    command_groups: BTreeMap<String, CommandGroupPolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandGroupPolicy {
    commands: BTreeMap<String, CommandPolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPolicy {
    program: PathBuf,
    read_args: Vec<usize>,
    write_args: Vec<usize>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct PolicyFile {
    #[serde(default)]
    filesystem: FilesystemFile,
    #[serde(default)]
    command_groups: BTreeMap<String, CommandGroupFile>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct FilesystemFile {
    #[serde(default)]
    read: Vec<PathBuf>,
    #[serde(default)]
    write: Vec<PathBuf>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandGroupFile {
    commands: BTreeMap<String, CommandFile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandFile {
    program: PathBuf,
    #[serde(default)]
    read_args: Vec<usize>,
    #[serde(default)]
    write_args: Vec<usize>,
}

impl ExecutionPolicy {
    pub fn deny_all() -> Self {
        Self::default()
    }

    pub fn from_file(path: &Path) -> Result<Self, CompileError> {
        let source = fs::read_to_string(path).map_err(|error| {
            CompileError::new(
                0,
                format!("failed to read policy {}: {error}", path.display()),
            )
        })?;
        let parsed = toml::from_str::<PolicyFile>(&source).map_err(|error| {
            CompileError::new(
                0,
                format!("failed to parse policy {}: {error}", path.display()),
            )
        })?;
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        Self::from_parsed(parsed, base)
    }

    fn from_parsed(parsed: PolicyFile, base: &Path) -> Result<Self, CompileError> {
        let read_roots = canonical_roots(base, parsed.filesystem.read, "read")?;
        let write_roots = canonical_roots(base, parsed.filesystem.write, "write")?;
        let mut command_groups = BTreeMap::new();
        for (group_name, group) in parsed.command_groups {
            validate_identifier(&group_name, "command group")?;
            let mut commands = BTreeMap::new();
            for (command_name, command) in group.commands {
                validate_identifier(&command_name, "command alias")?;
                let configured_program = if command.program.is_absolute() {
                    command.program
                } else {
                    base.join(command.program)
                };
                let program = fs::canonicalize(&configured_program).map_err(|error| {
                    CompileError::new(
                        0,
                        format!(
                            "failed to resolve command `{group_name}.{command_name}` program {}: {error}",
                            configured_program.display()
                        ),
                    )
                })?;
                if !program.is_file() {
                    return Err(CompileError::new(
                        0,
                        format!(
                            "command `{group_name}.{command_name}` program is not a file: {}",
                            program.display()
                        ),
                    ));
                }
                if !is_executable(&program)? {
                    return Err(CompileError::new(
                        0,
                        format!(
                            "command `{group_name}.{command_name}` program is not executable: {}",
                            program.display()
                        ),
                    ));
                }
                let mut read_args = command.read_args;
                read_args.sort_unstable();
                read_args.dedup();
                let mut write_args = command.write_args;
                write_args.sort_unstable();
                write_args.dedup();
                if read_args.iter().any(|index| write_args.contains(index)) {
                    return Err(CompileError::new(
                        0,
                        format!(
                            "command `{group_name}.{command_name}` cannot mark one argument as both read and write"
                        ),
                    ));
                }
                commands.insert(
                    command_name,
                    CommandPolicy {
                        program,
                        read_args,
                        write_args,
                    },
                );
            }
            command_groups.insert(group_name, CommandGroupPolicy { commands });
        }
        Ok(Self {
            read_roots,
            write_roots,
            command_groups,
        })
    }

    pub(crate) fn command(&self, group: &str, command: &str) -> Option<&CommandPolicy> {
        self.command_groups
            .get(group)
            .and_then(|group| group.commands.get(command))
    }

    pub(crate) fn read_roots(&self) -> &[PathBuf] {
        &self.read_roots
    }

    pub(crate) fn write_roots(&self) -> &[PathBuf] {
        &self.write_roots
    }

    pub(crate) fn has_read_access(&self) -> bool {
        !self.read_roots.is_empty()
    }

    pub(crate) fn has_write_access(&self) -> bool {
        !self.write_roots.is_empty()
    }
}

impl CommandPolicy {
    pub(crate) fn program(&self) -> &Path {
        &self.program
    }

    pub(crate) fn read_args(&self) -> &[usize] {
        &self.read_args
    }

    pub(crate) fn write_args(&self) -> &[usize] {
        &self.write_args
    }
}

fn canonical_roots(
    base: &Path,
    roots: Vec<PathBuf>,
    access: &str,
) -> Result<Vec<PathBuf>, CompileError> {
    let mut canonical = Vec::with_capacity(roots.len());
    for root in roots {
        let root = if root.is_absolute() {
            root
        } else {
            base.join(root)
        };
        let root = fs::canonicalize(&root).map_err(|error| {
            CompileError::new(
                0,
                format!(
                    "failed to resolve {access} root {}: {error}",
                    root.display()
                ),
            )
        })?;
        if !root.is_dir() {
            return Err(CompileError::new(
                0,
                format!("{access} root is not a directory: {}", root.display()),
            ));
        }
        canonical.push(root);
    }
    canonical.sort();
    canonical.dedup();
    Ok(canonical)
}

fn validate_identifier(value: &str, kind: &str) -> Result<(), CompileError> {
    let mut chars = value.chars();
    if !chars
        .next()
        .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
        || !chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
    {
        return Err(CompileError::new(
            0,
            format!("invalid {kind} name `{value}`"),
        ));
    }
    Ok(())
}

fn is_executable(path: &Path) -> Result<bool, CompileError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = fs::metadata(path).map_err(|error| {
            CompileError::new(
                0,
                format!(
                    "failed to inspect command program {}: {error}",
                    path.display()
                ),
            )
        })?;
        Ok(metadata.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deny_all_has_no_capabilities() {
        let policy = ExecutionPolicy::deny_all();
        assert!(!policy.has_read_access());
        assert!(!policy.has_write_access());
        assert!(policy.command("read", "cat").is_none());
    }
}
