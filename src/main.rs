use std::num::NonZeroU32;
use std::sync::Arc;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use redis::aio::{Connection, PubSub};
use tokio::sync::Mutex;
use tracing::info;
use crate::reddit::backend::direct::DirectBackend;
use crate::reddit::backend::tor::TorBackend;
use crate::reddit::Reddit;

mod reddit;
mod redis_helper;
mod update_list;
mod server;
mod updater;

#[derive(Copy, Clone, Debug, Eq, Ord, PartialOrd, PartialEq, ValueEnum)]
pub enum RedditBackendSelector {
    DIRECT,
    TOR,
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[clap(long = "redis-url", short = 'r', default_value = "redis://127.0.0.1/")]
    redis_url: String,

    #[clap(long = "reddit-backend", default_value = "direct")]
    reddit_backend: RedditBackendSelector,

    #[clap(long = "rate-limit", default_value = "0.5")]
    rate_limit: f32,

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

    pub async fn new_reddit_backend(&self) -> Result<Arc<Reddit>> {
        match self.reddit_backend {
            RedditBackendSelector::DIRECT => Ok(Reddit::new(DirectBackend::new(self.rate_limit)?)),
            RedditBackendSelector::TOR => Ok(Reddit::new(TorBackend::new(self.rate_limit)?)),
        }
    }
}

#[derive(Subcommand)]
pub enum Commands {
    /// Updates the subreddit list in the database
    UpdateSubredditList {
        #[clap(long = "period", short = 'p')]
        period: Option<NonZeroU32>,
    },
    /// Serve the pages
    Server {
        #[clap(long = "listen", short = 'l', default_value = "0.0.0.0:4000")]
        listen: String,
    },
    Updater {
        #[clap(long = "period", short = 'p')]
        period: Option<NonZeroU32>,
    },
    Check {
        #[clap(long = "subreddit", short = 's')]
        subreddit: String,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // install global collector configured based on RUST_LOG env var.
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match &cli.command {
        Commands::UpdateSubredditList { period } => {
            update_list::update_list(&cli, *period).await?;
        }
        Commands::Server { listen } => {
            server::server(&cli, &listen).await?;
        }
        Commands::Updater { period } => {
            updater::updater(&cli, *period).await?;
        }
        Commands::Check { subreddit } => {
            let reddit = cli.new_reddit_backend().await?;
            let result = reddit.get_subreddit_state(subreddit).await?;
            info!("Subreddit {subreddit} is state: {result:?}");
        }
    }


    Ok(())
}
