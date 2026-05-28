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
    size_min_text: &'a str,
    size_max_text: &'a str,
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
    let size_min_text = format!("{}MB", size_min / 1_000_000);
    let size_max_text = format!("{}MB", size_max / 1_000_000);

    Ok(Html(
        ViewTemplate {
            prev_signature: &prev_signature,
            size_min,
            size_max,
            size_min_text: &size_min_text,
            size_max_text: &size_max_text,
        }
        .render()?,
    ))
}
