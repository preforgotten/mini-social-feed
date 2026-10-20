use axum::{extract::State, response::sse::Event, response::Sse};
use futures::stream::Stream;
use std::convert::Infallible;
use tokio_stream::StreamExt;

use crate::state::AppState;

/// GET /api/events — SSE stream of system events.
pub async fn event_stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.event.subscribe().await.unwrap();

    let stream = rx.into_stream().filter_map(|result| match result {
        Ok(event) => {
            let json = serde_json::to_string(&event).unwrap_or_default();
            Some(Ok(Event::default().data(json)))
        }
        Err(_) => None,
    });

    Sse::new(stream)
}
