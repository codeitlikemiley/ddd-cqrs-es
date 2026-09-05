use anyhow::{Context, Result};
use serde::Serialize;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug)]
pub struct FileOperation {
    pub path: PathBuf,
    pub content: String,
    pub overwrite: bool,
    pub description: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct FileOperationReport {
    pub action: &'static str,
    pub path: String,
    pub bytes: usize,
    pub description: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct CommandReport {
    pub status: &'static str,
    pub message: String,
    pub operations: Vec<FileOperationReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl CommandReport {
    pub fn new(status: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
            operations: Vec::new(),
            command: None,
            data: None,
        }
    }

    pub fn with_operations(mut self, operations: Vec<FileOperationReport>) -> Self {
        self.operations = operations;
        self
    }

    pub fn with_command(mut self, command: Vec<String>) -> Self {
        self.command = Some(command);
        self
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }
}

/// Joins a generated relative path onto the project root, refusing anything
/// that could resolve outside it.
///
/// Generated paths are derived from names in `ddd.toml`, which is untrusted
/// input in a cloned repository, so every read and write goes through this
/// containment check instead of a bare `root.join(..)`.
pub fn contained_join(root: &Path, relative: &Path) -> Result<PathBuf> {
    if relative.as_os_str().is_empty() {
        anyhow::bail!("generated path is empty");
    }
    for component in relative.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir => anyhow::bail!(
                "generated path {} escapes the project root via `..`",
                relative.display()
            ),
            Component::RootDir | Component::Prefix(_) => anyhow::bail!(
                "generated path {} must be relative to the project root",
                relative.display()
            ),
        }
    }
    Ok(root.join(relative))
}

pub fn apply_operations(
    root: &Path,
    operations: &[FileOperation],
    dry_run: bool,
    force: bool,
) -> Result<Vec<FileOperationReport>> {
    validate_operations(root, operations, force)?;

    let mut reports = Vec::with_capacity(operations.len());
    let mut applied = Vec::new();
    for operation in operations {
        let absolute_path = contained_join(root, &operation.path)?;
        let exists = absolute_path.exists();
        let previous = if exists {
            Some(
                std::fs::read(&absolute_path)
                    .with_context(|| format!("failed to read {}", absolute_path.display()))?,
            )
        } else {
            None
        };

        if !dry_run {
            let write_result = if is_manifest_path(&operation.path) {
                write_manifest_atomically(&absolute_path, &operation.content)
            } else {
                if let Some(parent) = absolute_path.parent() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("failed to create {}", parent.display()))?;
                }
                std::fs::write(&absolute_path, &operation.content)
                    .with_context(|| format!("failed to write {}", absolute_path.display()))
            };
            if let Err(error) = write_result {
                rollback_applied(&applied);
                return Err(error);
            }
            applied.push(AppliedWrite {
                path: absolute_path.clone(),
                previous,
            });
        }

        reports.push(FileOperationReport {
            action: if exists { "update" } else { "create" },
            path: operation.path.display().to_string(),
            bytes: operation.content.len(),
            description: operation.description.clone(),
        });
    }
    Ok(reports)
}

struct AppliedWrite {
    path: PathBuf,
    previous: Option<Vec<u8>>,
}

fn rollback_applied(applied: &[AppliedWrite]) {
    for write in applied.iter().rev() {
        match &write.previous {
            Some(content) => {
                let _ = std::fs::write(&write.path, content);
            }
            None => {
                let _ = std::fs::remove_file(&write.path);
            }
        }
    }
}

fn is_manifest_path(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == "ddd.toml")
}

fn validate_operations(root: &Path, operations: &[FileOperation], force: bool) -> Result<()> {
    for operation in operations {
        let absolute_path = contained_join(root, &operation.path)?;
        if absolute_path.exists() && !operation.overwrite && !force {
            anyhow::bail!(
                "{} already exists; rerun with --force to overwrite",
                absolute_path.display()
            );
        }
    }
    Ok(())
}

fn write_manifest_atomically(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let temp_path = path.with_extension("toml.tmp");
    std::fs::write(&temp_path, content)
        .with_context(|| format!("failed to write {}", temp_path.display()))?;
    std::fs::rename(&temp_path, path).with_context(|| {
        format!(
            "failed to replace {} with {}",
            path.display(),
            temp_path.display()
        )
    })?;
    Ok(())
}

pub fn write_operation(
    path: impl Into<PathBuf>,
    content: impl Into<String>,
    overwrite: bool,
    description: impl Into<String>,
) -> FileOperation {
    FileOperation {
        path: path.into(),
        content: content.into(),
        overwrite,
        description: description.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_operations, contained_join, write_operation};
    use std::path::Path;

    #[test]
    fn contained_join_accepts_relative_project_paths() {
        let joined =
            contained_join(Path::new("/project"), Path::new("src/domain/order.rs")).unwrap();

        assert_eq!(joined, Path::new("/project/src/domain/order.rs"));
    }

    #[test]
    fn contained_join_rejects_traversal_and_absolute_paths() {
        for path in [
            "../outside.rs",
            "src/domain/../../../outside.rs",
            "/etc/passwd",
        ] {
            assert!(
                contained_join(Path::new("/project"), Path::new(path)).is_err(),
                "{path} should be refused"
            );
        }
    }

    #[test]
    fn apply_operations_refuses_to_write_outside_the_root() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        let operations = vec![write_operation(
            "src/domain/../../../outside.rs",
            "// escaped\n",
            false,
            "traversal attempt",
        )];

        let error = apply_operations(&root, &operations, false, true).unwrap_err();

        assert!(error.to_string().contains("escapes the project root"));
        assert!(!temp.path().join("outside.rs").exists());
    }

    #[test]
    fn apply_operations_validates_all_paths_before_writing() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        std::fs::create_dir_all(root.join("src/domain")).unwrap();
        std::fs::write(root.join("src/domain/existing.rs"), "old\n").unwrap();

        let operations = vec![
            write_operation("src/domain/new.rs", "new\n", false, "new module"),
            write_operation(
                "src/domain/existing.rs",
                "newer\n",
                false,
                "blocked overwrite",
            ),
        ];

        let error = apply_operations(&root, &operations, false, false).unwrap_err();
        assert!(error.to_string().contains("already exists"));
        assert!(!root.join("src/domain/new.rs").exists());
    }

    #[test]
    fn apply_operations_writes_manifest_atomically() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        std::fs::create_dir_all(&root).unwrap();
        let manifest = root.join("ddd.toml");
        std::fs::write(&manifest, "old = true\n").unwrap();

        let operations = vec![write_operation(
            "ddd.toml",
            "new = true\n",
            true,
            "update project manifest",
        )];
        apply_operations(&root, &operations, false, false).unwrap();

        assert_eq!(std::fs::read_to_string(&manifest).unwrap(), "new = true\n");
        assert!(!root.join("ddd.toml.tmp").exists());
    }

    #[test]
    fn apply_operations_rolls_back_on_mid_batch_failure() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        std::fs::create_dir_all(root.join("src/domain")).unwrap();
        let locked = root.join("locked");
        std::fs::create_dir_all(&locked).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
        }

        let operations = vec![
            write_operation("src/domain/new.rs", "new\n", false, "new module"),
            write_operation("locked/other.rs", "bad\n", false, "read-only directory"),
        ];

        let result = apply_operations(&root, &operations, false, false);
        #[cfg(unix)]
        assert!(result.is_err(), "expected write into read-only dir to fail");
        #[cfg(not(unix))]
        let _ = result;
        assert!(!root.join("src/domain/new.rs").exists());
    }
}
