use buzz_goose::{
    ArtifactStore, FileArtifactStore, GooseRuntime, GooseRuntimeConfig, RepositoryHealthRequest,
};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "buzz-goose", about = "Bounded real Goose execution for Buzz")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Debug, Subcommand)]
enum Command {
    /// Run a real read-only Goose repository-health inspection.
    Inspect {
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
        #[arg(long)]
        artifact_dir: PathBuf,
        #[arg(long)]
        tenant: String,
        #[arg(long)]
        actor: String,
    },
    /// Report real Goose runtime availability and version.
    Health,
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = GooseRuntime::new(GooseRuntimeConfig::from_env()?);
    match Cli::parse().command {
        Command::Health => println!("{}", serde_json::to_string_pretty(&runtime.health().await)?),
        Command::Inspect {
            workspace,
            artifact_dir,
            tenant,
            actor,
        } => {
            let record = runtime
                .inspect_repository_health(RepositoryHealthRequest {
                    workspace,
                    goal: "Inspect this repository and tell me whether it is healthy.".to_string(),
                    tenant_id: tenant,
                    actor_id: actor,
                })
                .await;
            let artifact = FileArtifactStore::new(artifact_dir).store(&record)?;
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({ "record": record, "artifact": artifact })
                )?
            );
        }
    }
    Ok(())
}
