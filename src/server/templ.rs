use std::collections::BTreeMap;
use std::sync::Arc;
use axum::extract::State;
use axum::response::{Html, IntoResponse};
use axum_template::engine::Engine;
use axum_template::TemplateEngine;
use cached::proc_macro::cached;
use serde::Serialize;
use tera::Tera;
use crate::reddit::{SubredditDelta, SubredditState};
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
    history: Vec<SubredditDelta>,
    dark_states: Vec<SubredditState>,
    light_states: Vec<SubredditState>,
    state_map: BTreeMap<SubredditState, String>,
}

pub async fn make_app_engine() -> anyhow::Result<AppEngine> {
    let mut tera = Tera::default();
    tera.add_template_file("templates/index.html", Some("index"))?;
    Ok(Engine::from(tera))
}

#[cached(time = 30, sync_writes = true, key = "String", convert = r#"{ "A".to_string() }"#)]
async fn render_index(state: Arc<AppState>) -> String {
    let subs = state.redis_helper.get_current_state().await.unwrap();
    let dark_subs = subs.iter().filter(|s| s.state.is_dark()).count();
    let total_subs = subs.len();
    let sections = state.redis_helper.get_sections().await.unwrap();
    let history = state.redis_helper.get_hist_delta().await.unwrap_or_else(|_| Vec::new());
    let params = Params {
        perc_subs: format!("{:.2}", (dark_subs as f32 / total_subs as f32) * 100.0),
        total_subs,
        dark_subs,
        history,
        dark_states: SubredditState::dark_states(),
        light_states: SubredditState::light_states(),
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
        state_map: SubredditState::state_map(),
    };

    state.engine.render("index", params).unwrap()
}

pub async fn get_index(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    Html(render_index(state).await).into_response()
}