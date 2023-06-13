use std::collections::HashMap;
use std::sync::Arc;
use redis::aio::Connection;
use tokio::sync::Mutex;
use anyhow::Result;
use futures_util::TryStream;
use futures_util::StreamExt;
use redis::{AsyncCommands, Msg};
use tracing::info;
use crate::Cli;
use crate::reddit::{Subreddit, SubredditDelta, SubredditState};

#[derive(Clone)]
pub struct RedisHelper {
    con: Arc<Mutex<Connection>>,
}

impl RedisHelper {
    pub async fn new(cli: &Cli) -> Result<Self> {
        let con = cli.new_redis_connection().await?;
        Ok(Self {
            con,
        })
    }
    pub async fn get_current_state(&self) -> Result<Vec<Subreddit>> {
        let srs: HashMap<String, String> = self.con.lock().await.hgetall("subreddit").await?;
        let values = srs.values()
            .map(|v| {
                serde_json::from_str::<Subreddit>(v)
            })
            .collect::<Result<Vec<Subreddit>, serde_json::Error>>()?;
        Ok(values)
    }

    pub async fn update_subreddit(&self, subreddit: &Subreddit) -> Result<()> {
        let val = serde_json::to_string(&subreddit)?;
        self.con.lock().await.hset("subreddit", subreddit.safe_name(), val).await?;
        Ok(())
    }

    pub async fn set_sections(&self, sections: Vec<String>) -> Result<()> {
        let val = serde_json::to_string(&sections)?;
        self.con.lock().await.set("sections", val).await?;
        Ok(())
    }

    pub async fn get_sections(&self) -> Result<Vec<String>> {
        let sections: Option<String> = self.con.lock().await.get("sections").await?;
        if let Some(sections) = sections {
            Ok(serde_json::from_str(&sections)?)
        } else {
            Ok(vec![
                "40+ million".to_string(),
                "30+ million".to_string(),
                "20+ million".to_string(),
                "10+ million".to_string(),
                "5+ million".to_string(),
                "1+ million".to_string(),
                "500k+".to_string(),
                "250k+".to_string(),
                "100k+".to_string(),
                "50k+".to_string(),
                "5k+".to_string(),
                "5k and below".to_string(),
                "1k+".to_string(),
                "1k and below".to_string(),
            ])
        }
    }

    pub async fn send_delta(&self, delta: &SubredditDelta) -> Result<()> {
        if delta.prev_state != SubredditState::UNKNOWN || (delta.prev_state == SubredditState::UNKNOWN && delta.subreddit.state == SubredditState::PRIVATE) {
            info!("Sending subreddit delta for {}...", delta.subreddit.name);
            let data = serde_json::to_string(&delta)?;
            self.con.lock().await.lpush("historical_deltas", data.clone()).await?;
            self.con.lock().await.publish("subreddit_updates", data).await?;
        } else {
            info!("Skipping subreddit delta for {}.", delta.subreddit.name);
        }
        Ok(())
    }

    pub async fn trim_history(&self) -> Result<()> {
        self.con.lock().await.ltrim("historical_deltas", 0, 10000).await?;
        Ok(())
    }

    pub async fn apply_delta(&self, delta: &SubredditDelta) -> Result<()> {
        self.update_subreddit(&delta.subreddit).await?;
        if delta.prev_state != delta.subreddit.state {
            self.send_delta(&delta).await?;
        }
        Ok(())
    }
}

pub async fn new_delta_stream(cli: &Cli) -> Result<impl TryStream<Ok = SubredditDelta, Error = anyhow::Error>> {
    let mut pubsub = cli.new_redis_pubsub().await?;
    pubsub.subscribe("subreddit_updates").await?;
    let s = pubsub.into_on_message();
    let s = s.map(|item: Msg| {
        let item: Msg = item;
        let delta: String = item.get_payload()?;
        let delta: SubredditDelta = serde_json::from_str(&delta)?;
        anyhow::Ok(delta)
    });
    Ok(s)
}