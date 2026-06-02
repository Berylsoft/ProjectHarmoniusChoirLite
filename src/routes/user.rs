use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};

use crate::ServerState;

pub mod submit;
pub mod submit_get;
pub mod submits;

pub fn routes() -> Router<ServerState> {
    Router::new().route(
        "/submit",
        post(submit::handler)
            .layer(DefaultBodyLimit::max(1_000_000_000) /* limited within multipart receive */)
            .get(submit_get::handler),
    ).route("/submits", get(submits::handler))
}
