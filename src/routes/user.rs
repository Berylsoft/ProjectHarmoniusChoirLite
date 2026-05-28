use axum::{Router, extract::DefaultBodyLimit, routing::post};

use crate::ServerState;

pub mod submit;
pub mod submit_get;

pub fn routes() -> Router<ServerState> {
    Router::new().route(
        "/submit",
        post(submit::handler)
            .layer(DefaultBodyLimit::disable() /* limited within multipart receive */)
            .get(submit_get::handler),
    )
}
