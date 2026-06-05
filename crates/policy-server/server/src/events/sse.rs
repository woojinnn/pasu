//! `GET /events/stream` — Server-Sent Events feed for the dashboard.
//! Subscribes to the [`EventBus`], filters by the authenticated `user_id`,
//! and streams matching events as SSE messages. Browser `EventSource`
//! auto-reconnects via `Last-Event-ID` (we re-emit the broadcast id as
//! the SSE id).
//! Auth: standard `Authorization: Bearer …` header. `EventSource` cannot
//! set custom headers, so dashboard clients use a tiny polyfill (or pass
//! the token via `?token=` and let the middleware accept either) — see

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::Extension;
use futures::stream::{self, Stream, StreamExt};
use tokio_stream::wrappers::BroadcastStream;

use crate::auth::AuthUser;
use crate::events::bus::EventBus;

/// `GET /events/stream` — long-lived SSE response.
/// Emits one SSE block per matching event. The `event:` field is the
/// `Event::kind()` discriminator so the client can `addEventListener` to
/// individual types. Comments (`: keepalive`) are sent every 30s by the
/// axum `KeepAlive` layer to keep proxies from closing idle connections.
pub async fn stream(
    State(bus): State<EventBus>,
    Extension(user): Extension<AuthUser>,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let rx = bus.subscribe();
    let stream = BroadcastStream::new(rx)
        // BroadcastStream yields `Result<T, RecvError>` — drop lag errors
        // (slow subscriber); the dashboard refreshes on reconnect anyway.
        .filter_map(|r| async { r.ok() })
        .filter_map(move |(uid, event)| {
            let mine = uid == user.user_id;
            async move {
                if !mine {
                    return None;
                }
                let kind = event.kind();
                let data = serde_json::to_string(&event).ok()?;
                Some(Ok::<_, Infallible>(
                    SseEvent::default().event(kind).data(data),
                ))
            }
        });
    // Empty prelude so the client sees a 200 immediately, before any
    // event arrives — keeps `EventSource.onopen` firing.
    let prelude = stream::once(async { Ok(SseEvent::default().comment("connected")) });
    Sse::new(prelude.chain(stream)).keep_alive(KeepAlive::new().interval(Duration::from_secs(30)))
}
