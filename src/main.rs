use std::num::NonZeroU32;
use std::sync::Arc;

use anyhow::Result;
use clap::{Parser, Subcommand};
use redis::aio::{Connection, PubSub};
use tokio::sync::Mutex;

mod reddit;
mod redis_helper;
mod update_list;
mod server;
mod updater;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[clap(long = "redis-url", short = 'r', default_value = "redis://127.0.0.1/")]
    redis_url: String,

    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    pub async fn new_redis_connection(&self) -> Result<Arc<Mutex<Connection>>> {
        let client = redis::Client::open(&*self.redis_url).unwrap();
        Ok(Arc::new(Mutex::new(client.get_async_connection().await?)))
    }

    pub async fn new_redis_pubsub(&self) -> Result<PubSub> {
        let client = redis::Client::open(&*self.redis_url).unwrap();
        Ok(client.get_async_connection().await?.into_pubsub())
    }

}

#[derive(Subcommand)]
pub enum Commands {
    /// Updates the subreddit list in the database
    UpdateSubredditList {
        #[clap(long = "rate-limit", short = 'r', default_value = "100")]
        rate_limit: NonZeroU32,
    },
    /// Serve the pages
    Server {
        #[clap(long = "listen", short = 'l', default_value = "0.0.0.0:4000")]
        listen: String,
    },
    Updater {
        #[clap(long = "rate-limit", short = 'r', default_value = "100")]
        rate_limit: NonZeroU32,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // install global collector configured based on RUST_LOG env var.
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match &cli.command {
        Commands::UpdateSubredditList { rate_limit } => {
            update_list::update_list(&cli, *rate_limit).await?;
        }
        Commands::Server { listen } => {
            server::server(&cli, &listen).await?;
        }
        Commands::Updater { rate_limit } => {
            updater::updater(&cli, *rate_limit).await?;
        }
    }


    Ok(())
}
