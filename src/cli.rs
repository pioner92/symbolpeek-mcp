use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use rmcp::{transport::stdio, ServiceExt};

const SYMBOLPEEK_SKILL: &str = include_str!("../skills/symbolpeek/SKILL.md");
const SYMBOLPEEK_OPENAI_METADATA: &str = include_str!("../skills/symbolpeek/agents/openai.yaml");

/// Runs the `SymbolPeek` CLI entry point.
///
/// # Errors
///
/// Returns an error when the MCP stdio service cannot start or shut down
/// cleanly.
pub async fn run() -> Result<()> {
    let mut arguments = std::env::args().skip(1);
    let command = arguments.next();
    if command.as_deref() == Some("stats") {
        let reset_lifetime = arguments.any(|argument| argument == "--reset");
        crate::server::print_cli_statistics(reset_lifetime);
        return Ok(());
    }
    if matches!(command.as_deref(), Some("--help" | "-h")) {
        print_help();
        return Ok(());
    }
    if matches!(command.as_deref(), Some("--version" | "-V")) {
        println!("symbolpeek {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if command.as_deref() == Some("install-skills") {
        let target = arguments.next().unwrap_or_else(|| "all".to_owned());
        if arguments.next().is_some() {
            bail!("usage: symbolpeek install-skills [codex|claude|all]");
        }
        install_skills(&target)?;
        return Ok(());
    }

    let service = crate::server::SymbolPeekServer::new()
        .serve(stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}

fn print_help() {
    println!(
        "SymbolPeek\n\nUsage:\n  symbolpeek                          Start the MCP server\n  symbolpeek stats                    Show lifetime statistics\n  symbolpeek stats --reset\n  symbolpeek install-skills [client]  Install agent guidance (codex, claude, or all)\n  symbolpeek --version\n\nAlias:\n  sym                                 Equivalent short command name\n\nThe MCP server communicates over stdio."
    );
}

fn install_skills(target: &str) -> Result<()> {
    let home = user_home()?;
    match target {
        "codex" => {
            let root =
                std::env::var_os("CODEX_HOME").map_or_else(|| home.join(".codex"), PathBuf::from);
            print_installed("Codex", &install_skill_at(&root)?);
        }
        "claude" => {
            let root = std::env::var_os("CLAUDE_CONFIG_DIR")
                .map_or_else(|| home.join(".claude"), PathBuf::from);
            print_installed("Claude Code", &install_skill_at(&root)?);
        }
        "all" => {
            let codex_root =
                std::env::var_os("CODEX_HOME").map_or_else(|| home.join(".codex"), PathBuf::from);
            let claude_root = std::env::var_os("CLAUDE_CONFIG_DIR")
                .map_or_else(|| home.join(".claude"), PathBuf::from);
            print_installed("Codex", &install_skill_at(&codex_root)?);
            print_installed("Claude Code", &install_skill_at(&claude_root)?);
        }
        _ => bail!("unknown client '{target}'; expected codex, claude, or all"),
    }
    println!("Restart the client so it discovers the SymbolPeek skill.");
    Ok(())
}

fn user_home() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .context("could not determine the user home directory")
}

fn install_skill_at(client_root: &Path) -> Result<PathBuf> {
    let skill_dir = client_root.join("skills").join("symbolpeek");
    let agents_dir = skill_dir.join("agents");
    std::fs::create_dir_all(&agents_dir)
        .with_context(|| format!("failed to create {}", agents_dir.display()))?;
    std::fs::write(skill_dir.join("SKILL.md"), SYMBOLPEEK_SKILL)
        .with_context(|| format!("failed to install skill in {}", skill_dir.display()))?;
    std::fs::write(agents_dir.join("openai.yaml"), SYMBOLPEEK_OPENAI_METADATA).with_context(
        || {
            format!(
                "failed to install skill metadata in {}",
                agents_dir.display()
            )
        },
    )?;
    Ok(skill_dir)
}

fn print_installed(client: &str, path: &Path) {
    println!(
        "Installed SymbolPeek skill for {client}: {}",
        path.display()
    );
}

#[cfg(test)]
mod tests {
    use super::{install_skill_at, SYMBOLPEEK_SKILL};

    #[test]
    fn installs_skill_and_metadata() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "symbolpeek-skill-test-{}-{unique}",
            std::process::id(),
        ));
        let skill_dir = install_skill_at(&root).expect("skill installation should succeed");

        assert_eq!(
            std::fs::read_to_string(skill_dir.join("SKILL.md")).unwrap(),
            SYMBOLPEEK_SKILL
        );
        assert!(skill_dir.join("agents/openai.yaml").is_file());

        std::fs::remove_dir_all(root).unwrap();
    }
}
