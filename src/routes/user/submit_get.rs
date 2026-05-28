use anyhow::Context as _;
use askama::Template;
use axum::response::{Html, IntoResponse};

use crate::{
    Transaction,
    routes::{
        self,
        auth::access_token::{self, AccessToken},
        user::submit::SIZE_RANGE,
    },
    sql,
};

#[derive(Debug, askama::Template)]
#[template(path = "user/submit.html")]
struct ViewTemplate<'a> {
    prev_signature: &'a str,
    size_min: usize,
    size_max: usize,
}

#[expect(clippy::missing_errors_doc)]
pub async fn handler(
    token: AccessToken<access_token::User>,
    trans: Transaction,
) -> routes::Result<impl IntoResponse> {
    let prev_signature = sql::get_previous_submit_signature_by_user_id(
        &trans,
        token.uid(),
    )
    .context("get_previous_submit_signature_by_user_id")?
    .map_or_else(String::new, |it| it.user_signature);

    trans.commit().await?;

    let size_min = *SIZE_RANGE.start();
    let size_max = *SIZE_RANGE.end();

    Ok(Html(
        ViewTemplate {
            prev_signature: &prev_signature,
            size_min,
            size_max,
        }
        .render()?,
    ))
}
