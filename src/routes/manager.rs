use axum::{
    Router,
    routing::{get, post},
};

use crate::ServerState;

pub mod review;
pub mod submit_detail;
pub mod submits;

pub fn routes() -> Router<ServerState> {
    Router::new()
        .route("/review", post(review::handler))
        .route("/submits", get(submits::handler))
        .route("/submits/{submit}", get(submit_detail::handler))
}
