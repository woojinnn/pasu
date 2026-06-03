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

use crate::app::ShutdownRx;
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
    shutdown: Option<Extension<ShutdownRx>>,
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
    // Drain on shutdown: when the `ShutdownRx` extension flips to `true`
    // (SIGTERM), end the stream so graceful shutdown doesn't wait out the
    // 30s keepalive. Without the extension (e.g. tests) the stream stays
    // unbounded, preserving prior behavior.
    let body = prelude.chain(stream).take_until(async move {
        match shutdown {
            Some(Extension(ShutdownRx(mut rx))) => {
                let _ = rx.wait_for(|v| *v).await;
            }
            None => std::future::pending::<()>().await,
        }
    });
    Sse::new(body).keep_alive(KeepAlive::new().interval(Duration::from_secs(30)))
}
