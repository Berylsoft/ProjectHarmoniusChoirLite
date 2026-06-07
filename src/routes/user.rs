use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};

use crate::ServerState;

pub mod error;
pub mod submit;
pub mod submit_get;
pub mod submits;

pub fn routes() -> Router<ServerState> {
    Router::new()
        .route(
            "/submit",
            post(submit::handler)
            // limited in multipart receive
            .layer(DefaultBodyLimit::max(1_000_000_000) )
            .get(submit_get::handler),
        )
        .route("/submits", get(submits::handler))
        .route("/error/{error}", get(error::handler))
}
