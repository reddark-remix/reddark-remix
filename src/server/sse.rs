use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Sse;
use axum::response::sse::Event;
use futures_util::{Stream, TryFutureExt};
use futures_util::StreamExt;
use futures_util::TryStreamExt;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use crate::server::AppState;
use crate::server::model::PushMessage;

pub async fn sse_handler(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item=Result<Event, anyhow::Error>>> {
    let receiver = state.broadcast_channel.subscribe();

    let receiver = BroadcastStream::new(receiver);

    let stream = receiver
        .map(|message: Result<PushMessage, BroadcastStreamRecvError>| -> Result<Event, _> {
            let message = message?;
            let data = serde_json::to_string(&message)?;
            Ok(Event::default().data(data))
        });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(1))
            .text("keep-alive-text"),
    )
}