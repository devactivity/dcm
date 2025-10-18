use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "dcm")]
#[command(about = "lightweight docker container manager", long_about = None)]
#[command(version, author)]
pub struct Cli {
    #[arg(short, long, value_name = "SOCKET_PATH")]
    pub socket: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    List {
        #[arg(short, long)]
        all: bool,

        #[arg(short, long)]
        quiet: bool,

        #[arg(short, long, value_name = "FORMAT", default_value = "table")]
        format: String,
    },

    Inspect {
        #[arg(required = true)]
        ids: Vec<String>,
    },

    Start {
        #[arg(required = true)]
        ids: Vec<String>,
    },

    Stop {
        #[arg(required = true)]
        ids: Vec<String>,

        #[arg(short, long)]
        timeout: Option<u64>,
    },

    Restart {
        #[arg(required = true)]
        ids: Vec<String>,

        #[arg(short, long)]
        timeout: Option<u64>,
    },

    Rm {
        #[arg(required = true)]
        ids: Vec<String>,

        #[arg(short, long)]
        force: bool,

        #[arg(short, long)]
        volumes: bool,
    },

    Stats {
        #[arg(required = true)]
        ids: Vec<String>,
    },

    Logs {
        id: String,

        #[arg(short, long)]
        follow: bool,
        #[arg(short, long)]
        tail: Option<String>,
        #[arg(short, long)]
        since: Option<String>,
        #[arg(short, long)]
        until: Option<String>,
        #[arg(short, long)]
        timestamps: bool,
    },
}
