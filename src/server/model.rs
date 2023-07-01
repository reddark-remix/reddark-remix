use std::collections::BTreeMap;
use serde::{Serialize, Deserialize};
use crate::reddit::{Subreddit, SubredditState};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", content = "content")]
pub enum PushMessage {
    CurrentStateUpdate {
        sections: Vec<String>,
        subreddits: Vec<Subreddit>,
        dark_states: Vec<SubredditState>,
        light_states: Vec<SubredditState>,
        state_map: BTreeMap<SubredditState, String>,
    },
    Delta {
        name: String,
        section: String,
        previous_state: SubredditState,
        state: SubredditState,
    },
    Reload {

    },
}
