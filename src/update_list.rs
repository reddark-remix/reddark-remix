use std::num::NonZeroU32;
use crate::reddit::Reddit;
use anyhow::Result;
use tracing::info;
use redis::AsyncCommands;
use crate::Cli;

pub async fn update_list(cli: &Cli, rate_limit: NonZeroU32) -> Result<()> {
    let reddit = Reddit::new(rate_limit);
    let con = cli.new_redis_connection().await?;

    {
        let mut con = con.lock().await;
        info!("Fetching subreddits...");
        let subs = reddit.fetch_subreddits().await?;
        for sub in subs {
            let e: bool = con.hexists("subreddit", sub.safe_name()).await?;
            if !e {
                info!("Adding subreddit {}...", sub.name);
                let val = serde_json::to_string(&sub)?;
                con.hset("subreddit", sub.safe_name(), val).await?;
            } else {
                info!("Subreddit {} already exists!", sub.name);
            }
        }
        info!("Done!");
    }
    Ok(())
}