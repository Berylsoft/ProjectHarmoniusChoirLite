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
    shared::{Group, Status},
    sql,
    utils::warn_problem_general,
};

#[expect(clippy::missing_errors_doc)]
pub async fn handler(
    _token: AccessToken<access_token::Manager>,
    extract::Path(sid): extract::Path<i64>,
    trans: Transaction,
) -> routes::Result<impl IntoResponse> {
    let submit =
        sql::get_submit_by_id(&trans, sid).context("get_submit_by_id")?;

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

    let mut replaced = matches!(status, Status::Replaced);
    let readonly = !matches!(status, Status::Pending);

    let rejected = matches!(status, Status::Rejected) || !readonly;
    let (passed, lead, harmony) = if let Status::Passed {
        groups,
        replaced: r,
    } = &status
    {
        replaced |= *r;
        (
            true,
            groups.contains(&Group::Lead),
            groups.contains(&Group::Harmony),
        )
    } else {
        Default::default()
    };

    Ok(Html(
        ViewTemplate {
            id: submit.id,
            user_id: submit.user_id,
            nth: submit.nth,
            signature: &submit.user_signature,
            hgi: submit.harmony_group_intention,
            created_at: &submit.created_at,
            // statuse
            replaced,
            readonly,
            passed,
            lead,
            harmony,
            rejected,
        }
        .render()?,
    ))
}

#[expect(clippy::struct_excessive_bools)]
#[derive(Debug, Template)]
#[template(path = "manager/submit_detail.html")]
struct ViewTemplate<'a> {
    id: i64,
    user_id: i64,
    nth: i64,
    signature: &'a str,
    hgi: bool,
    created_at: &'a str,
    // status
    replaced: bool,
    readonly: bool,
    passed: bool,
    lead: bool,
    harmony: bool,
    rejected: bool,
}
