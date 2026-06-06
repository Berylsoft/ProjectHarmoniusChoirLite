use std::{
    collections::HashSet,
    range::RangeInclusive,
    time::{Duration, Instant},
};

use anyhow::Context;
use axum::{
    Extension,
    extract::{
        Query, WebSocketUpgrade,
        ws::{self, WebSocket},
    },
    response::IntoResponse,
};
use futures::{
    FutureExt as _, SinkExt as _, StreamExt as _, TryStreamExt,
    stream::{self, SplitSink, SplitStream},
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tokio::sync::broadcast;
use tower_http::request_id::RequestId;

use crate::{
    BotKey, Notify,
    routes::{self, bot::validate_bot_token, notify},
    shared::Group,
};

#[derive(Debug, Deserialize)]
pub struct ConnectToken {
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
}

#[derive(Debug, Deserialize)]
pub struct ConnectReqQuery {
    token: Box<str>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum Payload {
    Review { id: Box<str>, result: ReviewResult },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "action", content = "data")]
pub enum ReviewResult {
    Reject,
    Pass {
        groups: HashSet<Group>,
        ignored: bool,
    },
}

#[expect(clippy::missing_errors_doc)]
pub async fn handler(
    Query(params): Query<ConnectReqQuery>,
    bot_key: BotKey,
    notification_state: Notify,
    ws: WebSocketUpgrade,
    Extension(req_id): axum::Extension<RequestId>,
) -> routes::Result<impl IntoResponse> {
    if !cfg!(feature = "mock_bot_token") {
        validate_bot_token::<ConnectToken, _>(
            &params.token,
            &bot_key,
            |token| {
                let now = OffsetDateTime::now_utc();
                (now - time::Duration::minutes(2)..=now)
                    .contains(&token.created_at)
            },
        )?;
    }

    tracing::info!("new sub");
    let req_id = Box::<str>::from(
        str::from_utf8(req_id.into_header_value().as_bytes())
            .context("req_id to str")?,
    );

    Ok(ws
        .on_failed_upgrade(|err| {
            tracing::debug!("failed upgrade: {err}");
        })
        .on_upgrade(move |socket| async move {
            run(req_id, socket, notification_state).await;
        }))
}

#[tracing::instrument(level = "info", skip(socket, notification_state))]
async fn run(
    req_id: Box<str>,
    socket: WebSocket,
    notification_state: Notify,
) {
    let res = BotNotify::new(socket).run(notification_state).await;
    if let Err(err) = res {
        tracing::warn!("notification/bot err: {err:?}");
    }
}

struct BotNotify {
    state: State,
    ws_tx: SplitSink<WebSocket, ws::Message>,
    ws_rx: stream::Fuse<SplitStream<WebSocket>>,

    // ping-pong
    start: Instant,
    ping_range: RangeInclusive<u64>,
}

enum State {
    Running,
    Closing,
    Closed,
}

impl State {
    #[must_use]
    const fn is_running(&self) -> bool {
        matches!(self, Self::Running)
    }

    #[must_use]
    const fn is_closing(&self) -> bool {
        matches!(self, Self::Closing)
    }

    #[must_use]
    const fn is_closed(&self) -> bool {
        matches!(self, Self::Closed)
    }
}

impl BotNotify {
    const PING_INTERVAL: u64 = 20;
    const PING_TIMEOUT: u64 = 60;

    fn new(socket: WebSocket) -> Self {
        let (ws_tx, ws_rx) = socket.split();
        Self {
            state: State::Running,
            ws_tx,
            ws_rx: ws_rx.fuse(),

            start: Instant::now(),
            ping_range: (0..=0).into(),
        }
    }

    async fn run(
        &mut self,
        notification_state: Notify,
    ) -> anyhow::Result<()> {
        let permit = notification_state
            .running
            .acquire()
            .await
            .context("acquire running")?;
        let mut cmd_rx = notification_state.cmd_bus.subscribe();
        let mut interval = tokio::time::interval(Duration::from_secs(
            Self::PING_INTERVAL,
        ));

        while self.state.is_running() {
            futures::select_biased! {
                cmd = cmd_rx.recv().fuse() => {
                    self.handle_cmd(cmd).await.context("handle_cmd")?;
                }
                msg = self.ws_rx.try_next().fuse() => {
                    self.handle_msg(msg).context("handle_msg")?;
                }
                _ = interval.tick().fuse() => {
                    self.handle_ping().await.context("handle_ping")?;
                }
            };
        }

        if self.state.is_closing() {
            tracing::debug!("sending close frame");
            self.ws_tx
                .send(ws::Message::Close(Some(ws::CloseFrame {
                    code: ws::close_code::NORMAL,
                    reason: "server shutdown".into(),
                })))
                .await
                .context("shutdown")?;

            tracing::info!("draining remaining messages");
            let start = Instant::now();
            loop {
                let res = tokio::time::timeout(
                    Duration::from_secs(5),
                    self.ws_rx.try_next(),
                )
                .await;
                if let Ok(msg) = res {
                    let msg = msg.context("recv remaining message")?;
                    let Some(msg) = msg else {
                        let msg = "connection closed before \
                        received the reply lose frame, \
                        the other end may not be standard compliant";
                        tracing::warn!("{msg}");
                        break;
                    };

                    if matches!(msg, ws::Message::Close(_)) {
                        break;
                    }

                    if start.elapsed().as_secs() <= 10 {
                        continue;
                    }
                }

                let msg = "unable to receive \
                        the reply close frame within time limit, \
                        the other end may not be standard compliant";
                tracing::warn!("{msg}");
                break;
            }
        } else {
            assert!(self.state.is_closed());
            tracing::debug!("already closed, skip send close frame");
        }

        drop(permit);
        tracing::info!("connection stopped");
        Ok(())
    }

    async fn handle_cmd(
        &mut self,
        cmd: Result<notify::Command, broadcast::error::RecvError>,
    ) -> anyhow::Result<()> {
        let cmd = match cmd {
            Ok(cmd) => cmd,
            Err(broadcast::error::RecvError::Closed) => {
                tracing::error!("unexpected closed command broadcast");
                self.state = State::Closing;
                return Ok(());
            }
            Err(broadcast::error::RecvError::Lagged(count)) => {
                tracing::warn!("lagged command recv, {count} behind");
                return Ok(());
            }
        };

        match cmd {
            notify::Command::Stop => {
                tracing::info!("shutdown connection");
                self.state = State::Closing;
            }
            notify::Command::NotifyBot(payload) => {
                tracing::debug!("sending {payload:?}");
                let mut buf = Vec::with_capacity(256);
                ciborium::into_writer(&payload, &mut buf)
                    .context("serialize payload")?;
                self.ws_tx
                    .send(ws::Message::binary(buf))
                    .await
                    .context("send payload")?;
            }
        }

        Ok(())
    }

    fn handle_msg(
        &mut self,
        msg: Result<Option<ws::Message>, axum::Error>,
    ) -> anyhow::Result<()> {
        let msg = msg.context("recv message")?;
        let Some(msg) = msg else {
            tracing::info!("connection closed by the other end");
            self.state = State::Closed;
            return Ok(());
        };

        match msg {
            ws::Message::Text(text) => {
                tracing::warn!("unexpected text message: {text:?}");
            }
            ws::Message::Binary(bytes) => {
                tracing::warn!("unexpected binary message: {bytes:?}");
            }
            ws::Message::Pong(bytes) => {
                tracing::trace!("received pong: {bytes:?}");
                if let Ok(secs) =
                    (&bytes[..]).try_into().map(u64::from_ne_bytes)
                {
                    if self.ping_range.contains(&secs) {
                        self.ping_range.start = secs;
                    } else {
                        tracing::info!(
                            "out of range pong: {secs}, require: {:?}",
                            self.ping_range
                        );
                    }
                } else {
                    tracing::warn!("invalid pong payload: {bytes:?}");
                }
            }
            // handled by axum
            ws::Message::Ping(_) | ws::Message::Close(_) => {}
        }

        Ok(())
    }

    async fn handle_ping(&mut self) -> anyhow::Result<()> {
        let now = self.start.elapsed().as_secs();
        let deadline = now.saturating_sub(Self::PING_TIMEOUT);
        if self.ping_range.start < deadline {
            tracing::info!("ping timeout elapsed");
            self.state = State::Closing;
            return Ok(());
        }

        tracing::trace!("sending ping: {now}");
        self.ws_tx
            .send(ws::Message::Ping(
                Box::<[_]>::from(now.to_ne_bytes()).into(),
            ))
            .await
            .context("send ping")?;

        self.ping_range.last = now;

        Ok(())
    }
}
