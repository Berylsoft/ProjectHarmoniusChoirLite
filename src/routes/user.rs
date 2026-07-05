use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};

use crate::ServerState;

pub mod error;
pub mod extra_file;
pub mod extra_file_get;
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
        .route(
            "/extra_file",
            post(extra_file::handler)
            // limited in multipart receive
            .layer(DefaultBodyLimit::max(1_000_000_000) )
            .get(extra_file_get::handler),
        )
        .route("/error/{error}", get(error::handler))
}
