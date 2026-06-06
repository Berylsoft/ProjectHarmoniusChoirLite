use anyhow::Context as _;
use axum::{
    extract::{Query, State},
    http::{StatusCode, header},
    response::IntoResponse,
};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use time::OffsetDateTime;

use crate::{
    BotKey, RedirPrefix, ServerState, TransactionDeferBegin,
    routes::{
        self,
        auth::access_token::{self, AccessToken},
        bot::validate_bot_token,
    },
    sql,
};

#[derive(Debug, Deserialize)]
pub struct LoginToken {
    pub id: Box<str>,
    pub is_manager: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
}

#[derive(Debug, Deserialize)]
pub struct LoginReqQuery {
    token: Box<str>,
    #[cfg(feature = "mock_bot_token")]
    is_manager: bool,
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
    let token = {
        #[cfg(feature = "mock_bot_token")]
        {
            LoginToken {
                id: params.token,
                is_manager: params.is_manager,
                created_at: OffsetDateTime::UNIX_EPOCH,
            }
        }
        #[cfg(not(feature = "mock_bot_token"))]
        {
            validate_bot_token::<LoginToken, _>(
                &params.token,
                &bot_key,
                |token| {
                    let now = OffsetDateTime::now_utc();
                    (now - time::Duration::minutes(10)..=now)
                        .contains(&token.created_at)
                },
            )?
        }
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

    let access_token = AccessToken::<access_token::User>::new(
        user_id,
        token_id,
        token.is_manager,
    );

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
