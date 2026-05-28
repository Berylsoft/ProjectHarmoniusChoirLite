use anyhow::Context as _;
use axum::{
    extract::{Query, State},
    http::{StatusCode, header},
    response::IntoResponse,
};
use axum_extra::extract::CookieJar;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use problem_details::ProblemDetails;
use serde::Deserialize;
use signed_data::SignedData;
use time::OffsetDateTime;

use crate::{
    BotKey, RedirPrefix, ServerState, TransactionDeferBegin,
    routes::{
        self,
        auth::access_token::{self, AccessToken},
    },
    sql,
};

#[derive(Debug, Deserialize)]
pub struct LoginToken {
    pub id: Box<str>,
    pub expires: time::OffsetDateTime,
}

#[derive(Debug, Deserialize)]
pub struct LoginReqQuery {
    token: Box<str>,
}

#[expect(clippy::missing_errors_doc)]
pub async fn handler(
    Query(params): Query<LoginReqQuery>,
    bot_key: BotKey,
    State(state): State<ServerState>,
    jar: CookieJar,
    trans: TransactionDeferBegin,
    redir_prefix: RedirPrefix,
) -> routes::Result<impl IntoResponse> {
    let token = if cfg!(feature = "mock_login") {
        LoginToken {
            id: params.token,
            expires: OffsetDateTime::UNIX_EPOCH,
        }
    } else {
        let invalid_token = || {
            Err(ProblemDetails::from_status_code(
                StatusCode::UNAUTHORIZED,
            )
            .with_detail("invalid token")
            .into())
        };

        let data = SignedData::<LoginToken>::try_from_base64(
            &*params.token,
            &URL_SAFE_NO_PAD,
        );
        let data = match data {
            Ok(data) => data,
            Err(err) => {
                tracing::debug!("invalid token encoding: {err}");
                return invalid_token();
            }
        };

        let token = data.to_verified(&bot_key);
        let token = match token {
            Ok(token) => token,
            Err(err) => {
                tracing::debug!("invalid token: {err}");
                return invalid_token();
            }
        };

        if token.expires < OffsetDateTime::now_utc() {
            tracing::debug!("invalid token: expired");
            return invalid_token();
        }

        token
    };

    let trans = trans.begin().await?;

    let user_id =
        sql::get_user_id_by_thirdparty_id(&trans, token.id.to_string())
            .context("get_user_id_by_thirdparty_id")?
            .map(|it| it.id);

    let (user_id, token_id) = if let Some(user_id) = user_id {
        let res = sql::inc_user_token_id_by_user_id(&trans, user_id)
            .context("inc_user_token_id_by_user_id")?
            .context("should return new token_id")?;
        (user_id, res.token_id)
    } else {
        let res = sql::ins_user(&trans, token.id.into())
            .context("ins_user")?
            .context("should return id and token_id")?;
        (res.id, res.token_id)
    };

    trans.commit().await?;

    let access_token =
        AccessToken::<access_token::User>::new(user_id, token_id);

    let jar = jar.add(access_token.into_cookie_with(&state)?);
    Ok((
        StatusCode::SEE_OTHER,
        jar,
        [
            (
                header::LOCATION,
                format!("{}/user/submits", &*redir_prefix),
            ),
        ],
    ))
}
