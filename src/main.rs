mod cli;
mod display;
mod docker;
mod error;

use anyhow::Result;
use clap::Parser;
use log::info;

use crate::cli::{Cli, Commands};

use crate::docker::DockerClient;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let cli = Cli::parse();

    let docker_client = DockerClient::new(cli.socket.as_deref())?;

    match cli.command {
        Commands::List { all, quiet, format } => {
            info!("listing containers with all={all}, quiet={quiet}");
            docker_client.list_containers(all, quiet, format).await?;
        }
        Commands::Inspect { ids } => {
            info!("Inspecting containers: {ids:?}");
            docker_client.inspect_containers(&ids).await?;
        }
        Commands::Start { ids } => {
            info!("Starting containers: {ids:?}");
            docker_client.start_containers(&ids).await?;
        }
        Commands::Stop { ids, timeout } => {
            info!("Stopping containers: {ids:?} with timeout: {timeout:?}");
            docker_client.stop_containers(&ids, timeout).await?;
        }
        Commands::Restart { ids, timeout } => {
            info!("Restarting containers: {ids:?} with timeout: {timeout:?}");
            docker_client.restart_containers(&ids, timeout).await?;
        }
        Commands::Rm {
            ids,
            force,
            volumes,
        } => {
            info!("Removing containers: {ids:?} with force: {force}, volumes: {volumes}");
            docker_client
                .remove_containers(&ids, force, volumes)
                .await?;
        }
        Commands::Stats { ids } => {
            info!("Showing stats for containers: {ids:?}");
            docker_client.container_stats(&ids).await?;
        }
        Commands::Logs {
            id,
            follow,
            tail,
            since,
            until,
            timestamps,
        } => {
            info!("
                Showing logs for containers: {id} with follow={follow}, tail: {tail:?}, since: {since:?}, until:{until:?}, timestamps={timestamps}
            ");
            docker_client
                .container_logs(&id, follow, tail, since, until, timestamps)
                .await?;
        }
    }

    Ok(())
}
