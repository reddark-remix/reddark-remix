use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;
use governor::{clock, Jitter, Quota, RateLimiter};
use governor::middleware::NoOpMiddleware;
use governor::state::{InMemoryState, NotKeyed};
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Copy, Ord, PartialOrd, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubredditState {
    UNKNOWN,
    PRIVATE,
    PUBLIC,
}

impl SubredditState {
    pub fn to_string(&self) -> String {
        match self {
            SubredditState::UNKNOWN => "public".to_string(),
            SubredditState::PRIVATE => "private".to_string(),
            SubredditState::PUBLIC => "public".to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Ord, PartialOrd, PartialEq, Eq)]
pub struct Subreddit {
    pub name: String,
    pub section: String,
    pub state: SubredditState,
}

impl Subreddit {
    pub fn safe_name(&self) -> String {
        self.name.replace(|c:char| !c.is_alphanumeric(), "_").to_string()
    }

    // pub fn is_private(&self) -> bool {
    //     match self.state {
    //         SubredditState::UNKNOWN => false,
    //         SubredditState::PRIVATE => true,
    //         SubredditState::PUBLIC => false,
    //     }
    // }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubredditDelta {
    pub prev_state: SubredditState,
    pub subreddit: Subreddit,
}

pub struct Reddit {
    limiter: RateLimiter<NotKeyed, InMemoryState, clock::DefaultClock, NoOpMiddleware>,
}

impl Reddit {
    pub fn new(rate_limit: NonZeroU32) -> Arc<Self> {
        let limiter = RateLimiter::direct(Quota::per_second(rate_limit));
        return Arc::new(Reddit {
            limiter,
        });
    }

    async fn make_request(&self, rel_url: &str) -> Result<reqwest::Response> {
        //self.limiter.until_ready_with_jitter(Jitter::up_to(Duration::from_millis(100))).await;
        self.limiter.until_ready().await;
        let client = reqwest::Client::builder();
        let client = client.user_agent("Mozilla/5.0 (X11; Linux x86_64; rv:109.0) Gecko/20100101 Firefox/114.0");
        let client = client.build()?;
        let req = client.get(format!("https://old.reddit.com/{}", rel_url));
        let req = req.header("Range", "bytes=0-50");
        let resp = req.send().await?;
        if resp.status().is_success() || resp.status() == 403 {
            Ok(resp)
        } else {
            Err(anyhow::anyhow!("Error querying reddit: {}", resp.text().await?))
        }
    }

    pub async fn is_subreddit_private(&self, name: &str) -> Result<bool> {
        let u = format!("{}.json", name);
        let resp = self.make_request(&u).await?;
        let data: serde_json::Value = resp.json().await?;
        if let Some(reason) = data.get("reason") {
            Ok(reason.as_str().unwrap_or("") == "private")
        } else {
            Ok(false)
        }
    }

    pub async fn fetch_subreddits(&self) -> Result<Vec<Subreddit>> {
        let resp = self.make_request("/r/ModCoord/wiki/index.json").await?;
        let data: serde_json::Value = resp.json().await?;
        let text = data.get("data").and_then(|v| v.get("content_md")).ok_or(anyhow::anyhow!("Couldn't get content_md!"))?;
        let text = text.as_str().ok_or(anyhow::anyhow!("Can't parse text"))?;

        let mut current_section = "".to_string();
        let mut subreddits = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with("##") && !line.contains("Please") && line.contains(":") {
                current_section = line.replace("##", "").replace(":", "").trim().to_string();
            } else if line.starts_with("r/") {
                subreddits.push(Subreddit {
                    name: line.to_string(),
                    section: current_section.clone(),
                    state: SubredditState::UNKNOWN,
                });
            }
        }

        Ok(subreddits)
    }
}