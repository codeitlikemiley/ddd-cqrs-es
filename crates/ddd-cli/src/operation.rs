use anyhow::{Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

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

pub fn apply_operations(
    root: &Path,
    operations: &[FileOperation],
    dry_run: bool,
    force: bool,
) -> Result<Vec<FileOperationReport>> {
    let mut reports = Vec::with_capacity(operations.len());
    for operation in operations {
        let absolute_path = root.join(&operation.path);
        let exists = absolute_path.exists();
        if exists && !operation.overwrite && !force {
            anyhow::bail!(
                "{} already exists; rerun with --force to overwrite",
                absolute_path.display()
            );
        }

        if !dry_run {
            if let Some(parent) = absolute_path.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
            std::fs::write(&absolute_path, &operation.content)
                .with_context(|| format!("failed to write {}", absolute_path.display()))?;
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
