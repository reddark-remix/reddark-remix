use std::collections::HashMap;
use std::sync::Arc;
use redis::AsyncCommands;
use tokio::sync::Mutex;
use tracing::info;
use crate::reddit::{Reddit, Subreddit, SubredditDelta, SubredditState};

pub async fn updater(con: Arc<Mutex<redis::aio::Connection>>, reddit: Reddit) -> anyhow::Result<()> {
    let srs: HashMap<String, String> = con.lock().await.hgetall("subreddit").await?;
    let reddit = Arc::new(reddit);

    let fns = srs.values().map(|sr| {
        let sr: Subreddit = serde_json::from_str(sr).unwrap();
        let con = con.clone();
        let reddit = reddit.clone();
        let f = async move {
            info!("Updating subreddit {}...", sr.name);

            let is_private = reddit.is_subreddit_private(&sr.name).await?;

            let mut sr_new = sr.clone();
            sr_new.state = if is_private { SubredditState::PRIVATE } else { SubredditState::PUBLIC };

            if sr_new != sr {
                info!("Change happend!");
                let val = serde_json::to_string(&sr_new)?;
                con.lock().await.hset("subreddit", sr_new.safe_name(), val).await?;
                if sr.state != SubredditState::UNKNOWN {
                    let delta = SubredditDelta {
                        prev_state: sr.state,
                        subreddit: sr_new,
                    };

                    con.lock().await.publish("subreddit_updates", serde_json::to_string(&delta)?).await?;
                } else {
                    info!("Skipping notify due to previous unknown!");
                }
            }

            anyhow::Ok(())
        };
        tokio::spawn(f)
    }).collect::<Vec<_>>();

    for h in fns {
        h.await;
    }

    Ok(())
}