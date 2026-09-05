use anyhow::Context;
use clap::{CommandFactory, Parser};
use fatsecret_cli::{cli::Cli, cli::Commands, commands};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    // Completions need no profile; handle before dispatch.
    if let Commands::Completions { shell } = &cli.command {
        let mut cmd = Cli::command();
        clap_complete::generate(*shell, &mut cmd, "fatsecret-cli", &mut std::io::stdout());
        return Ok(());
    }

    commands::dispatch(cli)
        .await
        .context("fatsecret-cli failed")?;
    Ok(())
}
