use axum::{Router, routing::get};

use crate::ServerState;

pub mod access_token;
pub mod login;

pub fn routes() -> Router<ServerState> {
    Router::new().route("/login", get(login::handler))
}
