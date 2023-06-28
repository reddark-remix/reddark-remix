use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use governor::{clock, Jitter, Quota, RateLimiter};
use governor::middleware::NoOpMiddleware;
use governor::state::{InMemoryState, NotKeyed};
use anyhow::Result;
use chrono::{DateTime, Utc};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Clone, Debug, Copy, Ord, PartialOrd, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubredditState {
    UNKNOWN,
    PRIVATE,
    PUBLIC,
    ARCHIVED,
    RESTRICTED,
}

impl SubredditState {
    pub fn to_string(&self) -> String {
        match self {
            SubredditState::UNKNOWN => "public".to_string(),
            SubredditState::PRIVATE => "private".to_string(),
            SubredditState::PUBLIC => "public".to_string(),
            SubredditState::RESTRICTED => "restricted".to_string(),
            SubredditState::ARCHIVED => "archived".to_string(),
        }
    }
}

impl FromStr for SubredditState {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "public" => Ok(SubredditState::PUBLIC),
            "restricted" => Ok(SubredditState::RESTRICTED),
            "private" => Ok(SubredditState::PRIVATE),
            "archived" => Ok(SubredditState::ARCHIVED),
            _ => Err(anyhow::anyhow!("No known state: {s}")),
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
        self.name.replace(|c: char| !c.is_alphanumeric(), "_").to_string()
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
    pub timestamp: DateTime<Utc>,
}

impl From<Subreddit> for SubredditDelta {
    fn from(value: Subreddit) -> Self {
        Self {
            prev_state: value.state,
            subreddit: value,
            timestamp: Utc::now(),
        }
    }
}

pub struct Reddit {
    limiter: RateLimiter<NotKeyed, InMemoryState, clock::DefaultClock, NoOpMiddleware>,
}

impl Reddit {
    pub fn new(rate_limit: NonZeroU32) -> Arc<Self> {
        info!("Initializing reddit with rate limit of {}!", rate_limit);
        // NOTE: Hardcoding rate limit here because without that for some reason reddit starts banning?
        // I don't know why passing 100 from cli is different from nonzero!(). But it is.
        let limiter = RateLimiter::direct(Quota::per_second(rate_limit));
        return Arc::new(Reddit {
            limiter,
        });
    }

    async fn make_request<T: Serialize + Sized>(&self, rel_url: &str, query: Option<&[T]>) -> Result<reqwest::Response> {
        self.limiter.until_ready_with_jitter(Jitter::up_to(Duration::from_millis(1))).await;
        let client = reqwest::Client::builder();
        let client = client.user_agent("Mozilla/5.0 (X11; Linux x86_64; rv:109.0) Gecko/20100101 Firefox/114.0");
        let client = client.build()?;
        let req = client.get(format!("https://old.reddit.com/{}", rel_url));
        let req = if let Some(q) = query {
            req.query(q)
        } else {
            req
        };
        let req = req.header("Range", "bytes=0-50");
        //info!("Sending request! {req:?}");
        let resp = req.send().await?;
        if resp.status().is_success() || resp.status() == 403 || resp.status() == 404 {
            Ok(resp)
        } else {
            let s = format!("{resp:?}");
            Err(anyhow::anyhow!("Error querying reddit: {s} {}", resp.text().await?))
        }
    }

    pub async fn get_subreddit_state(&self, name: &str) -> Result<SubredditState> {
        let u = format!("{}/about.json", name);
        let resp = self.make_request::<()>(&u, None).await?;
        let data: serde_json::Value = resp.json().await?;
        if let Some(reason) = data.get("reason") {
            let is_private = reason.as_str().unwrap_or("") == "private" || reason.as_str().unwrap_or("") == "banned";
            if is_private {
                Ok(SubredditState::PRIVATE)
            } else {
                Ok(SubredditState::PUBLIC)
            }
        } else {
            if let Some(data) = data.get("data") {
                if let Some(tp) = data.get("subreddit_type") {
                    let tp = tp.as_str().unwrap_or("");
                    if tp.to_uppercase() == "restricted".to_uppercase() {
                        Ok(SubredditState::RESTRICTED)
                    } else {
                        Ok(SubredditState::PUBLIC)
                    }
                } else {
                    Ok(SubredditState::PUBLIC)
                }
            } else {
                Ok(SubredditState::UNKNOWN)
            }
        }
    }

    pub async fn get_subreddit_state_bulk<T: ToString>(&self, names: &[T]) -> Result<BTreeMap<String, SubredditState>> {
        if names.len() > 100 {
            return Err(anyhow::anyhow!("Too many names passed!"));
        }
        let query = [("sr_name", names.iter().map(|n| n.to_string().trim_start_matches("r/").trim().to_string()).join(","))];
        let resp = self.make_request("api/info.json", Some(&query)).await?;
        let data: serde_json::Value = resp.json().await?;
        let data = data
            .get("data")
            .ok_or_else(|| anyhow::anyhow!("No data element"))?
            .get("children")
            .ok_or_else(|| anyhow::anyhow!("No children element"))?
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Children is not array"))?;
        data.iter()
            .map(|v| {
                let sub = v
                    .get("data")
                    .ok_or_else(|| anyhow::anyhow!("No data field in sr"))?;
                    let state = sub.get("subreddit_type")
                    .ok_or_else(|| anyhow::anyhow!("No subreddit type"))?
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Subreddit type is not a string"))?;
                let name = sub.get("display_name_prefixed")
                    .ok_or_else(|| anyhow::anyhow!("No subreddit display_name"))?
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Subreddit display_name is not a string"))?;

                anyhow::Ok((name.to_lowercase().to_string(), SubredditState::from_str(state)?))
            })
            .collect()
    }

    pub async fn fetch_subreddits(&self) -> Result<(Vec<String>, Vec<Subreddit>)> {
        let resp = self.make_request::<()>("/r/ModCoord/wiki/index.json", None).await?;
        let data: serde_json::Value = resp.json().await?;
        let text = data.get("data").and_then(|v| v.get("content_md")).ok_or(anyhow::anyhow!("Couldn't get content_md!"))?;
        let text = text.as_str().ok_or(anyhow::anyhow!("Can't parse text"))?;

        let mut current_section = "".to_string();
        let mut subreddits = Vec::new();
        let mut sections = Vec::new();
        let mut new_section = false;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with("##") && !line.contains("Please") && line.contains(":") {
                current_section = line.replace("##", "").replace(":", "").trim().to_string();
                new_section = true;
            } else if line.starts_with("r/") {
                if new_section {
                    sections.push(current_section.clone());
                    new_section = false;
                }
                subreddits.push(Subreddit {
                    name: line.to_string(),
                    section: current_section.clone(),
                    state: SubredditState::UNKNOWN,
                });
            }
        }

        Ok((sections, subreddits))
    }
}