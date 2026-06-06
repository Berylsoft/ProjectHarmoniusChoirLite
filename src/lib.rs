#![warn(missing_debug_implementations)]
#![warn(clippy::pedantic, clippy::nursery)]
// #![clippy::too_many_line_threshold = 60]
#![allow(clippy::default_trait_access)]
#![allow(clippy::unnecessary_debug_formatting)]

use std::{borrow::Cow, path::Path, sync::Arc};

use anyhow::{Context, anyhow};
use axum::extract::FromRequestParts;
use ed25519_dalek::{SigningKey, VerifyingKey};
use rusqlite::{OpenFlags, ffi::ErrorCode as SqlErrorCode};

pub mod routes;
pub mod shared;
pub mod sql;
pub mod utils;

#[derive(Debug, Clone)]
pub struct ServerState {
    pub inner: Arc<State>,
}

#[derive(Debug)]
pub struct State {
    pub cfg: Config,
    pub notify: routes::notify::State,
}

#[derive(Debug)]
pub struct Config {
    pub db_path: Box<Path>,
    pub key: SigningKey,
    pub bot_key: VerifyingKey,
    pub redir_prefix: Box<str>,
}

impl axum_extractors::token::State for ServerState {
    fn get_signing_key(&self) -> Cow<'_, SigningKey> {
        Cow::Borrowed(&self.inner.cfg.key)
    }
}

macro_rules! impl_extract_state {
    ($ty:ty) => {
        impl FromRequestParts<ServerState> for $ty {
            type Rejection = ();

            async fn from_request_parts(
                _parts: &mut axum::http::request::Parts,
                state: &ServerState,
            ) -> Result<Self, Self::Rejection> {
                Ok(Self(state.inner.clone()))
            }
        }
    };
}

#[derive(Debug)]
pub struct Key(Arc<State>);
simple_deref::impl_deref!(ref Key => SigningKey = .0.cfg.key);
impl_extract_state!(Key);

#[derive(Debug)]
pub struct BotKey(Arc<State>);
simple_deref::impl_deref!(ref BotKey => VerifyingKey = .0.cfg.bot_key);
impl_extract_state!(BotKey);

#[derive(Debug)]
pub struct RedirPrefix(Arc<State>);
simple_deref::impl_deref!(ref RedirPrefix => str = .0.cfg.redir_prefix);
impl_extract_state!(RedirPrefix);

#[derive(Debug)]
pub struct Notify(Arc<State>);
simple_deref::impl_deref!(ref Notify => routes::notify::State = .0.notify);
impl_extract_state!(Notify);

#[derive(Debug, derive_more::Deref)]
pub struct Transaction(pub sql::Transaction);

impl Transaction {
    /// # Errors
    ///
    /// if underlying rusqlite call fails or
    /// unable to begin within retry limit
    pub async fn begin(state: Arc<State>) -> routes::Result<Self> {
        let begin = move || -> routes::Result<_> {
            let conn =
                sql::db_open(&state.cfg.db_path, OpenFlags::default())
                    .context("open connetion")?;

            let retry_limit: i32 = 100;
            let mut count = 0;
            loop {
                if count > retry_limit {
                    return Err(anyhow!(
                        "unable to begin transaction within retry limit"
                    ))
                    .map_err(Into::into);
                }
                if count > 0 {
                    tracing::trace!(
                        "retry begin transaction, {count}/{retry_limit}"
                    );
                }
                count += 1;

                match conn.execute("BEGIN IMMEDIATE", ()) {
                    Ok(_) => return Ok(sql::Transaction::wrap(conn)),
                    Err(rusqlite::Error::SqliteFailure(
                        rusqlite::ffi::Error {
                            code:
                                SqlErrorCode::DatabaseBusy
                                | SqlErrorCode::DatabaseLocked,
                            ..
                        },
                        _,
                    )) => {}
                    Err(err) => {
                        return Err(err)
                            .context("begin transaction")
                            .map_err(Into::into);
                    }
                }
            }
        };
        let trans = tokio::task::spawn_blocking(begin)
            .await
            .context("join task")??;

        Ok(Self(trans))
    }

    /// # Errors
    ///
    /// if underlying rusqlite call fails.
    pub async fn commit(self) -> routes::Result<()> {
        tokio::task::spawn_blocking(move || {
            self.0.commit().context("commit")?;
            Ok(())
        })
        .await
        .context("join task")?
    }
}

impl FromRequestParts<ServerState> for Transaction {
    type Rejection = routes::Error;

    async fn from_request_parts(
        _parts: &mut axum::http::request::Parts,
        state: &ServerState,
    ) -> Result<Self, Self::Rejection> {
        Self::begin(state.inner.clone()).await
    }
}

#[derive(Debug, derive_more::Deref)]
pub struct TransactionDeferBegin(Arc<State>);

impl TransactionDeferBegin {
    #[expect(clippy::missing_errors_doc, reason = "see the target fn")]
    pub async fn begin(self) -> routes::Result<Transaction> {
        Transaction::begin(self.0).await
    }
}

impl FromRequestParts<ServerState> for TransactionDeferBegin {
    type Rejection = ();

    async fn from_request_parts(
        _parts: &mut axum::http::request::Parts,
        state: &ServerState,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self(state.inner.clone()))
    }
}
