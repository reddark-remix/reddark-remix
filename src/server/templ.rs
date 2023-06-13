use std::collections::BTreeMap;
use std::sync::Arc;
use axum::extract::State;
use axum::response::IntoResponse;
use axum_template::engine::Engine;
use axum_template::RenderHtml;
use serde::Serialize;
use tera::Tera;
use crate::reddit::{Subreddit, SubredditState};
use crate::server::{AppEngine, AppState};

#[derive(Serialize, Debug)]
struct ParamSubreddit {
    name: String,
    state: String,
}

#[derive(Serialize, Debug)]
struct Params {
    total_subs: usize,
    dark_subs: usize,
    perc_subs: String,
    sections: Vec<String>,
    subreddits: BTreeMap<String, Vec<ParamSubreddit>>,
}

pub async fn make_app_engine() -> anyhow::Result<AppEngine> {
    let mut tera = Tera::default();
    tera.add_template_file("templates/index.html", Some("index"))?;
    Ok(Engine::from(tera))
}

pub async fn get_index(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let subs = state.redis_helper.get_current_state().await.unwrap();
    let dark_subs = subs.iter().filter(|s| s.state == SubredditState::PRIVATE || s.state == SubredditState::RESTRICTED).count();
    let total_subs = subs.len();
    let sections = state.redis_helper.get_sections().await.unwrap();
    let params = Params {
        perc_subs: format!("{:.2}", (dark_subs as f32 / total_subs as f32) * 100.0),
        total_subs,
        dark_subs,
        sections: sections.clone(),
        subreddits: sections.into_iter()
            .map(|section| {
                let mut fsubs =  subs.iter()
                    .filter(|s| s.section == section)
                    .map(|s| ParamSubreddit {
                        name: s.name.clone(),
                        state: s.state.to_string(),
                    })
                    .collect::<Vec<ParamSubreddit>>();
                fsubs.sort_by(|a, b| a.name.to_uppercase().partial_cmp(&b.name.to_uppercase()).unwrap());
                (section.clone(), fsubs)
            })
            .collect(),
    };

    RenderHtml("index", state.engine.clone(), params)
}