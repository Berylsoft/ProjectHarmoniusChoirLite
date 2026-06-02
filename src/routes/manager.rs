use axum::{
    Router,
    routing::{get, post},
};

use crate::ServerState;

pub mod review;
pub mod submit_detail;
pub mod submit_download;
pub mod submits;
pub mod submits_download_passed;

pub fn routes() -> Router<ServerState> {
    Router::new()
        .route("/review", post(review::handler))
        .route("/submits", get(submits::handler))
        .route("/submits/download", get(submits::handler))
        .route(
            "/submits/download/passed",
            get(submits_download_passed::handler),
        )
        .route("/submits/{submit}", get(submit_detail::handler))
        .route("/submits/{submit}/file", get(submit_download::handler))
}
