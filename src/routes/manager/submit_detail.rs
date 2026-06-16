use anyhow::Context as _;
use askama::Template;
use axum::{
    extract::{self},
    http::StatusCode,
    response::{Html, IntoResponse},
};
use problem_details::ProblemDetails;

use crate::{
    Transaction,
    routes::{
        self,
        auth::access_token::{self, AccessToken},
    },
    shared::{Status, StatusFlat},
    sql,
    utils::{reformat_time_cn, warn_problem_general},
};

#[expect(clippy::missing_errors_doc)]
pub async fn handler(
    _token: AccessToken<access_token::Manager>,
    extract::Path(sid): extract::Path<i64>,
    trans: Transaction,
) -> routes::Result<impl IntoResponse> {
    let submit = sql::get_submit_with_thirdparty_id_by_id(&trans, sid)
        .context("get_submit_by_id")?;

    let Some(submit) = submit else {
        warn_problem_general("submit not found");
        return Err(ProblemDetails::from_status_code(
            StatusCode::NOT_FOUND,
        )
        .with_detail("submit not found")
        .into());
    };

    let status = Status::from_submit(
        &trans,
        submit.id,
        submit.rejected,
        submit.passed,
        submit.replaced,
    )?;

    trans.commit().await?;

    let mut status = StatusFlat::from(status);
    status.rejected |= status.pending;
    let readonly = !status.pending;

    Ok(Html(
        ViewTemplate {
            id: submit.id,
            user_id: submit.user_id,
            nth: submit.nth,
            signature: &submit.user_signature,
            hgi: submit.harmony_group_intention,
            comment: &submit.comment,
            created_at: reformat_time_cn(&submit.created_at)
                .context("reformat_time_cn")?,
            status,
            readonly,
            thirdparty_id: &submit.thirdparty_id,
        }
        .render()?,
    ))
}

#[derive(Debug, Template)]
#[template(path = "manager/submit_detail.html")]
struct ViewTemplate<'a> {
    id: i64,
    user_id: i64,
    nth: i64,
    signature: &'a str,
    hgi: bool,
    comment: &'a str,
    created_at: Box<str>,
    status: StatusFlat,
    readonly: bool,
    thirdparty_id: &'a str,
}
