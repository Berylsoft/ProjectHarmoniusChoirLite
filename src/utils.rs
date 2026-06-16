use std::{
    env::{self, VarError},
    ffi::OsStr,
    fmt::Write as _,
    path::{Path, PathBuf},
};

use anyhow::Context;
use axum::{
    http::{StatusCode, header},
    response::IntoResponse as _,
};
use time::OffsetDateTime;
use tower_http::request_id::{MakeRequestId, RequestId};
use tracing::level_filters::LevelFilter;
use tracing_appender::{non_blocking, rolling::Rotation};
use tracing_subscriber::{
    EnvFilter, Layer, layer::SubscriberExt, util::SubscriberInitExt,
};
use ulid::Ulid;

use crate::{RedirPrefix, routes};

#[expect(clippy::missing_errors_doc)]
pub fn init_env() -> anyhow::Result<LogGuard> {
    dotenvy::dotenv().ok();

    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();

    let (stdout_writer, stdout_writer_guard) =
        non_blocking::NonBlockingBuilder::default()
            .lossy(false)
            .finish(std::io::stdout());

    let stdout_layer = tracing_subscriber::fmt::Layer::new()
        .with_writer(stdout_writer)
        .with_filter(filter.clone());

    let file_writer = tracing_appender::rolling::Builder::new()
        .rotation(Rotation::HOURLY)
        .filename_suffix("log")
        .build("./logs")
        .context("build rolling appender")?;
    let (file_writer, file_writer_guard) =
        non_blocking::NonBlockingBuilder::default()
            .lossy(false)
            .finish(file_writer);

    let file_layer = tracing_subscriber::fmt::Layer::new()
        .with_writer(file_writer)
        .with_ansi(false)
        .with_filter(filter);

    // NOTE: order currently matters for ansi=false on span works,
    // see also: https://github.com/tokio-rs/tracing/pull/3221
    tracing_subscriber::registry()
        .with(file_layer)
        .with(stdout_layer)
        .init();

    Ok(LogGuard {
        file_writer_guard,
        stdout_writer_guard,
    })
}

#[expect(unused, reason = "for drop only")]
#[derive(Debug)]
pub struct LogGuard {
    file_writer_guard: non_blocking::WorkerGuard,
    stdout_writer_guard: non_blocking::WorkerGuard,
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

pub fn warn_problem(problem: &str, indicate: &str) {
    tracing::warn!("encountered {problem}, may indicate {indicate}");
}

pub fn warn_problem_general(problem: &str) {
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

pub fn rfc5987_utf8(data: impl AsRef<str>) -> Box<str> {
    let data = data.as_ref();
    let mut encoded = String::with_capacity(data.len());

    encoded.push_str("UTF-8''");

    let mut buf = [0_u8; 4];
    for ch in data.chars() {
        match ch {
            ch if ch.is_ascii_alphanumeric() => {
                encoded.push(ch);
            }
            '!' | '#' | '$' | '&' | '+' | '-' | '.' | '^' | '_' | '`'
            | '|' | '~' => {
                encoded.push(ch);
            }
            ch => {
                let utf8_bytes = ch.encode_utf8(&mut buf).as_bytes();
                for byte in utf8_bytes {
                    write!(&mut encoded, "%{byte:0>2x}")
                        .expect("no error when write to string");
                }
            }
        }
    }

    encoded.into_boxed_str()
}

/// true if within limit
#[must_use]
#[inline]
pub fn length_check_quick(s: &str, limit: usize) -> bool {
    if s.len() > limit * 4 {
        return false;
    }

    let mut cnt = 0;
    for _ in s.chars() {
        cnt += 1;
        if cnt > limit {
            return false;
        }
    }

    true
}

#[expect(clippy::missing_errors_doc)]
pub fn parse_rfc3339(
    t: &str,
) -> Result<OffsetDateTime, time::error::Parse> {
    OffsetDateTime::parse(
        t,
        &time::format_description::well_known::Rfc3339,
    )
}

#[expect(clippy::missing_errors_doc)]
pub fn format_time_cn(t: OffsetDateTime) -> anyhow::Result<Box<str>> {
    let t = t.to_offset(time::macros::offset!(+8));
    t.format(time::macros::format_description!(
        "[year]-[month]-[day] \
[hour repr:24]:[minute]:[second] \
[offset_hour sign:mandatory]"
    ))
    .context("format time")
    .map(String::into_boxed_str)
}

#[expect(clippy::missing_errors_doc)]
pub fn reformat_time_cn(t: &str) -> anyhow::Result<Box<str>> {
    let t = parse_rfc3339(t).context("parse_rfc3339")?;
    format_time_cn(t)
}
