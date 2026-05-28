use std::{
    env::{self, VarError},
    ffi::OsStr,
    path::{Path, PathBuf},
};

use axum::{
    http::{StatusCode, header},
    response::IntoResponse as _,
};
use tower_http::request_id::{MakeRequestId, RequestId};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;
use ulid::Ulid;

use crate::{RedirPrefix, routes};

pub mod file;

pub fn init_env() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();
}

/// # Panics
///
/// failed to register listeners.
pub async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("tokio::signal::ctrl_c()");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(
            tokio::signal::unix::SignalKind::terminate(),
        )
        .expect("tokio::signal::unix::signal(SignalKind::terminate())")
        .recv()
        .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }

    tracing::info!("shutdown");
}

#[derive(Debug, Clone, Copy)]
pub struct MakeRequestUlid;

impl MakeRequestId for MakeRequestUlid {
    fn make_request_id<B>(
        &mut self,
        _request: &axum::http::Request<B>,
    ) -> Option<RequestId> {
        Some(RequestId::new(Ulid::new().to_string().parse().unwrap()))
    }
}

/// # Errors
///
/// if not valid utf8
pub fn var_opt(
    key: impl AsRef<OsStr>,
) -> Result<Option<String>, VarError> {
    match env::var(key) {
        Ok(var) => Ok(Some(var)),
        Err(VarError::NotPresent) => Ok(None),
        Err(err) => Err(err),
    }
}

pub const MAX_USER_SIGNATURE_LENGTH: usize = 20;

#[must_use]
pub fn is_valid_user_signature(user_signature: &str) -> bool {
    if user_signature.is_empty() {
        return false;
    }

    if user_signature.len() > MAX_USER_SIGNATURE_LENGTH * 4 {
        return false;
    }

    let mut cnt = 0;
    for ch in user_signature.chars() {
        cnt += 1;
        if cnt > MAX_USER_SIGNATURE_LENGTH {
            return false;
        }

        // check for Cc,Cs,Co
        // Cs is not allowed in UTF-8, so checked by rust
        // Cc:
        if ch.is_control() {
            return false;
        }
        // Co:
        if matches!(ch, '\u{E000}'..='\u{F8FF}')
            | matches!(ch, '\u{F_0000}'..='\u{F_FFFD}')
            | matches!(ch, '\u{10_0000}'..='\u{10_FFFD}')
        {
            return false;
        }
    }

    true
}

#[must_use]
pub fn hash_to_storage_path(hash: blake3::Hash) -> Box<Path> {
    let path = format!(
        "./storage/{:0>2x}/{:0>2x}/{}",
        &(hash.as_bytes())[0],
        &(hash.as_bytes())[1],
        &hash.to_hex()[4..]
    );
    PathBuf::from(path).into_boxed_path()
}

pub fn warn_problem(problem: &'static str, indicate: &'static str) {
    tracing::warn!("encountered {problem}, may indicate {indicate}");
}

pub fn warn_problem_general(problem: &'static str) {
    let indicate = "broken invariant or abnormal behavior";
    warn_problem(problem, indicate);
}

#[must_use]
pub fn res_see_other(
    prefix: &RedirPrefix,
    path: &str,
) -> axum::http::Response<axum::body::Body> {
    (
        StatusCode::SEE_OTHER,
        [(header::LOCATION, format!("{}{path}", &**prefix))],
    )
        .into_response()
}

#[expect(clippy::missing_errors_doc)]
pub fn res_see_other_err_res<T>(
    prefix: &RedirPrefix,
    path: &str,
) -> routes::Result<T> {
    Err(res_see_other(prefix, path).into())
}
