use anyhow::Context as _;
use askama::Template;
use axum::response::{Html, IntoResponse};

use crate::{
    Transaction,
    routes::{
        self,
        auth::access_token::{self, AccessToken},
    },
    shared::Status,
    sql,
};

#[expect(clippy::missing_errors_doc)]
pub async fn handler(
    token: AccessToken<access_token::User>,
    trans: Transaction,
) -> routes::Result<impl IntoResponse> {
    let submits = sql::get_all_submits_by_user_id(&trans, token.uid())
        .context("get_all_submits_by_user_id")?;

    let mut items = Vec::with_capacity(submits.len());

    for submit in &submits {
        let status = Status::from_submit(
            &trans,
            submit.id,
            submit.rejected,
            submit.passed,
            submit.replaced,
        )?;

        items.push(Item {
            id: submit.id,
            nth: submit.nth,
            hgi: submit.harmony_group_intention,
            signature: submit.passed.then_some(&submit.user_signature),
            created_at: &submit.created_at,
            status,
        });
    }

    trans.commit().await?;

    Ok(Html(ViewTemplate { items: &items }.render()?))
}

#[derive(Debug, askama::Template)]
#[template(path = "user/submits.html")]
struct ViewTemplate<'a> {
    items: &'a [Item<'a>],
}

#[derive(Debug)]
struct Item<'a> {
    id: i64,
    nth: i64,
    signature: Option<&'a str>,
    hgi: bool,
    created_at: &'a str,
    status: Status,
}
