use axum::{Router, extract::DefaultBodyLimit, routing::post};

use crate::ServerState;

pub mod submit;

pub fn routes() -> Router<ServerState> {
    // TODO: body size
    Router::new().route(
        "/submit",
        post(submit::handler).layer(DefaultBodyLimit::max(100_000_000)),
    )
}
