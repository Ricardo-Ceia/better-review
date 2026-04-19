mod app;
mod domain;
mod services;
mod ui;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    app::run().await
}

#[cfg(test)]
mod tests {
    #[test]
    fn crate_modules_are_visible() {
        let _ = super::services::git::GitService::new(".");
        let _ = super::services::parser::parse_git_diff("").expect("empty diff should parse");
        let _ = super::ui::styles::title();
        let _ = super::domain::diff::FileDiff::default();
    }
}
