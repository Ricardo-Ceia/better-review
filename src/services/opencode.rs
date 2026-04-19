use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};

use anyhow::{Context, Result};
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct OpencodeService {
    repo_path: PathBuf,
    binary: String,
}

impl OpencodeService {
    pub fn new(repo_path: impl Into<PathBuf>, binary: impl Into<String>) -> Self {
        Self {
            repo_path: repo_path.into(),
            binary: binary.into(),
        }
    }

    pub async fn launch_interactive(&self) -> Result<ExitStatus> {
        Command::new(&self.binary)
            .current_dir(&self.repo_path)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .await
            .context("launch interactive opencode")
    }
}
