use axum::{Router, routing::any};
use tokio::sync::Semaphore;

use crate::ServerState;

pub mod bot;

pub fn routes() -> Router<ServerState> {
    Router::new().route("/bot", any(bot::handler))
}

#[derive(Debug)]
pub struct State {
    max_running: u32,
    running: Semaphore,
    cmd_bus: tokio::sync::broadcast::Sender<Command>,
}

impl State {
    #[must_use]
    pub fn new() -> Self {
        let max_running =
            u32::try_from(Semaphore::MAX_PERMITS).unwrap_or(u32::MAX);

        Self {
            max_running,
            running: Semaphore::new(
                usize::try_from(max_running)
                    .unwrap_or(Semaphore::MAX_PERMITS)
                    .min(Semaphore::MAX_PERMITS),
            ),
            cmd_bus: tokio::sync::broadcast::channel(1024).0,
        }
    }

    pub async fn stop_and_wait(&self) {
        tracing::info!("stop and wait");
        let _ = self.cmd_bus.send(Command::Stop);
        let _ = self.running.acquire_many(self.max_running).await;
    }

    pub fn notify_bot(&self, payload: bot::Payload) {
        tracing::info!("sending notification to bot: {payload:?}");
        let res = self.cmd_bus.send(Command::NotifyBot(payload));
        if res.is_err() {
            tracing::warn!("no bot connected");
        }
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub enum Command {
    Stop,
    NotifyBot(bot::Payload),
}
