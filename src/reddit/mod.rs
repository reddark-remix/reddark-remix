use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::Arc;
use anyhow::Result;
use chrono::{DateTime, Utc};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum::{EnumIter, IntoEnumIterator};
use crate::reddit::backend::RedditRequestBackend;

pub mod backend;

#[derive(Clone, Debug, Copy, Ord, PartialOrd, PartialEq, Eq, Serialize, Deserialize, EnumIter)]
pub enum SubredditState {
    UNKNOWN,
    PRIVATE,
    PUBLIC,
    ARCHIVED,
    OLIVER,
    RESTRICTED,
}

impl SubredditState {
    pub fn to_string(&self) -> String {
        match self {
            SubredditState::UNKNOWN => "unknown".to_string(),
            SubredditState::PRIVATE => "private".to_string(),
            SubredditState::PUBLIC => "public".to_string(),
            SubredditState::RESTRICTED => "restricted".to_string(),
            SubredditState::ARCHIVED => "archived".to_string(),
            SubredditState::OLIVER => "oliver".to_string(),
        }
    }

    pub fn is_dark(&self) -> bool {
        match self {
            SubredditState::UNKNOWN => false,
            SubredditState::PRIVATE => true,
            SubredditState::PUBLIC => false,
            SubredditState::ARCHIVED => true,
            SubredditState::OLIVER => true,
            SubredditState::RESTRICTED => true,
        }
    }

    pub fn is_light(&self) -> bool {
        !self.is_dark()
    }

    pub fn dark_states() -> Vec<SubredditState> {
        Self::iter().filter(|e| e.is_dark()).collect()
    }

    pub fn light_states() -> Vec<SubredditState> {
        Self::iter().filter(|e| e.is_light()).collect()
    }

    pub fn state_map() -> BTreeMap<SubredditState, String> {
        Self::iter().map(|e| (e, e.to_string())).collect()
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
    backend: Box<dyn RedditRequestBackend>,
}

impl Reddit {
    pub fn new(backend: Box<dyn RedditRequestBackend>) -> Arc<Self> {
        return Arc::new(Reddit {
            backend,
        });
    }

    pub async fn get_oliver_list(&self) -> Result<Vec<String>> {
        let data = reqwest::get("https://raw.githubusercontent.com/username-is-required/reddark-subinfo/main/john-oliver-subs.json").await?;
        let data: Value = data.json().await?;
        let data = data.get("johnOliverSubs").ok_or_else(|| anyhow::anyhow!("Unable to find element"))?;
        let data = data.as_array().ok_or_else(|| anyhow::anyhow!("Element is not array"))?;
        let data = data.iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        Ok(data)
    }

    pub async fn get_subreddit_state(&self, name: &str) -> Result<SubredditState> {
        let u = format!("{}/about.json", name);
        let data = self.backend.make_reddit_request(&u, None).await?;
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
        let query = [("sr_name".to_string(), names.iter().map(|n| n.to_string().trim_start_matches("r/").trim().to_string()).join(","))];
        let data = self.backend.make_reddit_request("api/info.json", Some(&query)).await?;
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
        let data = self.backend.make_reddit_request("/r/ModCoord/wiki/index.json", None).await?;
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