use std::{marker::PhantomData, ops::Add as _};

use anyhow::Context as _;
use axum::{
    Extension, RequestPartsExt, extract::FromRequestParts,
    http::StatusCode, response::IntoResponse,
};
use axum_extra::extract::cookie::Cookie;
use axum_extractors::token::{CustomToken, Token, TokenRejection};
use problem_details::ProblemDetails;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::{ServerState, Transaction, routes, sql, utils::warn_problem};

#[derive(Debug, Clone, Copy)]
pub struct User;
#[derive(Debug, Clone, Copy)]
pub struct Manager;
#[derive(Debug, Clone, Copy)]
pub struct Root;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AccessToken<V> {
    uid: i64,
    tid: i64,
    expires: time::OffsetDateTime,
    #[serde(skip)]
    _verify: PhantomData<V>,
}

impl<V> AccessToken<V> {
    #[must_use]
    pub fn new(uid: i64, tid: i64) -> Self {
        let expires =
            OffsetDateTime::now_utc().add(time::Duration::days(7));
        Self {
            uid,
            tid,
            expires,
            _verify: PhantomData,
        }
    }

    #[must_use]
    const fn into_another<T>(self) -> AccessToken<T> {
        let Self {
            uid,
            tid,
            expires,
            _verify: _,
        } = self;
        AccessToken {
            uid,
            tid,
            expires,
            _verify: PhantomData,
        }
    }

    /// # Errors
    ///
    /// if [`Self`] failed to serialize
    pub fn into_cookie_with(
        self,
        state: &ServerState,
    ) -> anyhow::Result<Cookie<'static>> {
        Token::new(self)
            .to_cookie_with(state)
            .with_context(|| format!("{} into cookie", Self::COOKIE_NAME))
    }

    #[must_use]
    pub const fn uid(&self) -> i64 {
        self.uid
    }
}

trait Verify {
    fn verify(
        &self,
        trans: &Transaction,
    ) -> Result<(), AccessTokenRejection>;
}

impl Verify for AccessToken<User> {
    fn verify(
        &self,
        trans: &Transaction,
    ) -> Result<(), AccessTokenRejection> {
        tracing::debug!("uid: {}, utid: {}", self.uid, self.tid);

        let token_id = sql::get_user_token_id_by_user_id(trans, self.uid)
            .context("get_user_token_id_by_user_id")
            .map_err(routes::Error::from)?
            .map(|it| it.token_id);

        let Some(token_id) = token_id else {
            return Err(AccessTokenRejection::UserNotExists);
        };

        if self.tid != token_id {
            return Err(AccessTokenRejection::Rotated);
        }

        Ok(())
    }
}

impl Verify for AccessToken<Manager> {
    fn verify(
        &self,
        trans: &Transaction,
    ) -> Result<(), AccessTokenRejection> {
        self.into_another::<User>().verify(trans)?;

        let is_manager = sql::is_manager_by_user_id(trans, self.uid)
            .context("is_manager_by_user_id")
            .map_err(routes::Error::from)?
            .context("exists should always return a row")
            .map_err(routes::Error::from)?
            .is_manager;

        if !is_manager {
            return Err(AccessTokenRejection::NotManager);
        }

        Ok(())
    }
}

impl Verify for AccessToken<Root> {
    fn verify(
        &self,
        trans: &Transaction,
    ) -> Result<(), AccessTokenRejection> {
        self.into_another::<User>().verify(trans)?;

        if self.uid != 0 {
            return Err(AccessTokenRejection::NotRoot);
        }

        Ok(())
    }
}

impl<V> CustomToken<ServerState> for AccessToken<V> {
    const COOKIE_NAME: &'static str = "token";

    fn get_expires_when(&self) -> OffsetDateTime {
        self.expires
    }
}

impl<T> FromRequestParts<ServerState> for AccessToken<T>
where
    T: Send + 'static,
    Self: Verify,
{
    type Rejection = AccessTokenRejection;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &ServerState,
    ) -> Result<Self, Self::Rejection> {
        if cfg!(feature = "mock_token") {
            return Ok(Self::new(0, 0));
        }

        let token: Token<Self, _> =
            parts.extract_with_state(state).await?;
        let trans: Transaction = parts.extract_with_state(state).await?;

        token.verify(&trans)?;

        trans.commit().await?;

        Ok(token.into_inner())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AccessTokenRejection {
    #[error("token: {0}")]
    Token(#[from] TokenRejection),
    #[error("other: {0}")]
    Other(#[from] routes::Error),
    #[error("user not exists")]
    UserNotExists,
    #[error("rotated")]
    Rotated,
    #[error("user isn't manager")]
    NotManager,
    #[error("user isn't root")]
    NotRoot,
}

impl IntoResponse for AccessTokenRejection {
    fn into_response(self) -> axum::response::Response {
        fn invalidated_res() -> axum::response::Response {
            Extension(
                ProblemDetails::from_status_code(
                    StatusCode::UNAUTHORIZED,
                )
                .with_detail("token invalidated"),
            )
            .into_response()
        }

        fn forbidden_res() -> axum::response::Response {
            Extension(
                ProblemDetails::from_status_code(StatusCode::FORBIDDEN)
                    .with_detail("forbidden"),
            )
            .into_response()
        }

        match self {
            Self::Token(token) => token.into_response(),
            Self::Other(err) => err.into_response(),
            Self::UserNotExists => {
                warn_problem("user not exists", "broken invariant");

                invalidated_res()
            }
            Self::Rotated => {
                warn_problem(
                    "rotated token",
                    "incorrect client impl, \
                    or abnormal behavior",
                );

                invalidated_res()
            }
            Self::NotManager => {
                warn_problem(
                    "NotManager",
                    "incorrect client impl, \
                    or abnormal behavior",
                );

                forbidden_res()
            }
            Self::NotRoot => {
                warn_problem(
                    "NotRoot",
                    "incorrect client impl, \
                    or abnormal behavior",
                );

                forbidden_res()
            }
        }
    }
}
