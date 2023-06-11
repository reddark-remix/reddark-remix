use std::sync::Arc;
use clap::{Parser, Subcommand};
use anyhow::Result;
use redis::aio::Connection;
use redis::AsyncCommands;
use tokio::sync::Mutex;
use tracing::info;

mod reddit;
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
    pub async fn make_connection(&self) -> anyhow::Result<Arc<Mutex<Connection>>> {
        let client = redis::Client::open(&*self.redis_url).unwrap();
        Ok(Arc::new(Mutex::new(client.get_async_connection().await?)))
    }
}

#[derive(Subcommand)]
pub enum Commands {
    /// Updates the subreddit list in the database
    UpdateSubredditList {  },
    /// Serve the pages
    Server {
        #[clap(long = "listen", short = 'l', default_value = "0.0.0.0:4000")]
        listen: String,
    },
    Updater {},
}

#[tokio::main]
async fn main() -> Result<()> {
    // install global collector configured based on RUST_LOG env var.
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    let mut con = cli.make_connection().await?;
    let mut reddit = reddit::Reddit::new();

    match &cli.command {
        Commands::UpdateSubredditList { .. } => {
            update_list::update_list(con.clone(), &mut reddit).await?;
        }
        Commands::Server { listen } => {
            server::server(con.clone(), &cli, &listen).await?;
        }
        Commands::Updater { .. } => {
            updater::updater(con.clone(), reddit).await?;
        }
    }


    Ok(())
}
