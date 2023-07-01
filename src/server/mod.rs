use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use axum::routing::get;

use axum::Server;
use axum_prometheus::PrometheusMetricLayer;
use axum_template::engine::Engine;
use futures_util::{TryStreamExt, TryFutureExt};
use tera::Tera;
use tokio::sync::broadcast;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing::info;
use crate::reddit::SubredditState;

use crate::redis_helper::RedisHelper;
use crate::server::model::PushMessage;

mod model;
mod sse;
mod templ;

// Type alias for our engine. For this example, we are using Handlebars
pub type AppEngine = Engine<Tera>;

pub struct AppState {
    broadcast_channel: broadcast::Sender<PushMessage>,
    redis_helper: RedisHelper,
    engine: AppEngine,
}

async fn start_server(redis_helper: RedisHelper, broadcast_channel: broadcast::Sender<PushMessage>, listen: &str) -> anyhow::Result<impl Future<Output=anyhow::Result<()>>> {
    let serve_dir = ServeDir::new("public")
        .append_index_html_on_directories(true);

    let (prometheus_layer, metric_handle) = PrometheusMetricLayer::pair();

    let shared_state = Arc::new(AppState {
        broadcast_channel,
        redis_helper,
        engine: templ::make_app_engine().await?,
    });

    let app = axum::Router::new()
        .fallback_service(serve_dir)
        .route("/", get(templ::get_index))
        .route("/sse", get(sse::sse_handler))
        .with_state(shared_state)
        .route("/metrics", get(|| async move { metric_handle.render() }))
        .layer(prometheus_layer)
        .layer(TraceLayer::new_for_http());

    Ok(
        Server::bind(&listen.parse().unwrap())
            .serve(app.into_make_service())
            .map_err(|e| anyhow::Error::from(e))
    )
}

async fn start_periodic_job(redis_helper: RedisHelper, broadcast_channel: broadcast::Sender<PushMessage>) -> anyhow::Result<impl Future<Output=anyhow::Result<()>>> {
    Ok(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            // Wait period.
            interval.tick().await;

            // Fetch info
            let sections = redis_helper.get_sections().await?;
            let mut subreddits = redis_helper.get_current_state().await?;

            subreddits.sort_by(|a, b| a.name.to_uppercase().partial_cmp(&b.name.to_uppercase()).unwrap());

            let message = PushMessage::CurrentStateUpdate {
                sections,
                subreddits,
                dark_states: SubredditState::dark_states(),
                light_states: SubredditState::light_states(),
                state_map: SubredditState::state_map(),
            };
            broadcast_channel.send(message)?;
        }
        // Hint to type system
        #[allow(unreachable_code)]
        anyhow::Ok(())
    })
}

async fn start_pubsub(cli: &crate::Cli, broadcast_channel: broadcast::Sender<PushMessage>) -> anyhow::Result<impl Future<Output=anyhow::Result<()>>> {
    let mut stream = crate::redis_helper::new_delta_stream(cli).await?;
    Ok(async move {
        while let Some(delta) = stream.try_next().await? {
            let message = PushMessage::Delta {
                name: delta.subreddit.name.clone(),
                section: delta.subreddit.section.clone(),
                previous_state: delta.prev_state,
                state: delta.subreddit.state,
            };
            broadcast_channel.send(message)?;
        }

        anyhow::Ok(())
    })
}

async fn start_reload_pubsub(cli: &crate::Cli, broadcast_channel: broadcast::Sender<PushMessage>) -> anyhow::Result<impl Future<Output=anyhow::Result<()>>> {
    let mut stream = crate::redis_helper::new_reload_stream(cli).await?;
    Ok(async move {
        while let Some(_) = stream.try_next().await? {
            let message = PushMessage::Reload {};
            broadcast_channel.send(message)?;
        }

        anyhow::Ok(())
    })
}


pub async fn server(cli: &crate::Cli, listen: &str) -> anyhow::Result<()> {
    info!("Starting server");
    let redis_helper = RedisHelper::new(cli).await?;

    let (broadcast_channel, _recv) = broadcast::channel(4096);

    let server = start_server(redis_helper.clone(), broadcast_channel.clone(), listen).await?;
    let periodic_subreddits = start_periodic_job(redis_helper.clone(), broadcast_channel.clone()).await?;
    let pubsub = start_pubsub(cli, broadcast_channel.clone()).await?;
    let reload_pubsub = start_reload_pubsub(cli, broadcast_channel.clone()).await?;

    tokio::select! {
        val = pubsub => {
            val?;
        }
        val = reload_pubsub => {
            val?;
        }
        val = server => {
            val?;
        }
        val = periodic_subreddits => {
            val?;
        }
    }

    info!("Exited!");

    Ok(())
}