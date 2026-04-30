mod app;
mod domain;
mod services;
mod settings;
mod ui;
mod web;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("web") | Some("--web") => web::run().await,
        _ => app::run().await,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn crate_modules_are_visible() {
        let _ = super::services::git::GitService::new(".");
        let _ = super::services::parser::parse_git_diff("").expect("empty diff should parse");
        let _ = super::ui::styles::title();
        let _ = super::domain::diff::FileDiff::default();
        let _ = super::settings::AppSettings::default();
        let _ = super::web::run;
    }
}
