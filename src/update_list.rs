use std::sync::Arc;
use crate::reddit::Reddit;
use anyhow::Result;
use redis::Commands;
use tracing::info;
use redis::AsyncCommands;
use tokio::sync::Mutex;

pub async fn update_list(con: Arc<Mutex<redis::aio::Connection>>, reddit: &mut Reddit) -> Result<()> {
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
    Ok(())
}