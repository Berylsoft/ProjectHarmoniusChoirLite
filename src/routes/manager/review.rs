use std::collections::HashSet;

use anyhow::Context as _;
use axum::{http::StatusCode, response::IntoResponse};
use problem_details::ProblemDetails;
use serde::Deserialize;

use crate::{
    Notify, Transaction, TransactionDeferBegin,
    routes::{
        self, Payload,
        auth::access_token::{self, AccessToken},
        notify::{self, bot::ReviewResult},
    },
    shared::Group,
    sql,
    utils::warn_problem_general,
};

#[derive(Debug, Deserialize)]
pub struct ReviewReq {
    sid: i64,
    action: ReviewAction,
}

#[derive(Debug, Deserialize)]
pub enum ReviewAction {
    Reject,
    Pass(HashSet<Group>),
}

#[expect(clippy::missing_errors_doc)]
pub async fn handler(
    token: AccessToken<access_token::Manager>,
    notify: Notify,
    trans: TransactionDeferBegin,
    req: Payload<ReviewReq>,
) -> routes::Result<impl IntoResponse> {
    tracing::debug!(
        "executing review: {:?}, by (uid){}",
        &*req,
        token.uid()
    );

    if let ReviewAction::Pass(groups) = &req.action {
        if groups.is_empty() {
            warn_problem_general("empty groups in review pass");
            return Err(ProblemDetails::from_status_code(
                StatusCode::BAD_REQUEST,
            )
            .with_detail("empty groups")
            .into());
        } else if !groups.contains(&Group::REQUIRED) {
            warn_problem_general("missing required group in review pass");
            return Err(ProblemDetails::from_status_code(
                StatusCode::BAD_REQUEST,
            )
            .with_detail("missing required groups")
            .into());
        }
    }

    let trans = trans.begin().await?;

    let submit = sql::get_submit_for_review_by_submit_id(&trans, req.sid)
        .context("get_submit_for_review_by_submit_id")?;
    let Some(submit) = submit else {
        warn_problem_general("submit not found");
        return Err(ProblemDetails::from_status_code(
            StatusCode::NOT_FOUND,
        )
        .with_detail("submit not found")
        .into());
    };

    if !submit.pending {
        warn_problem_general("submit not pending");
        return Err(ProblemDetails::from_status_code(
            StatusCode::CONFLICT,
        )
        .with_detail("submit not pending")
        .into());
    }

    let thirdparty_id =
        sql::get_user_thirdparty_id_by_submit_id(&trans, req.sid)
            .context("get_user_thirdparty_id_by_submit_id")?
            .context("should return thirdparty_id")?
            .thirdparty_id;
    let result = execute_review(&trans, &req, submit.user_id)?;

    trans.commit().await?;

    notify.notify_bot(notify::bot::Payload::Review {
        id: thirdparty_id.into_boxed_str(),
        result,
    });

    Ok(())
}

fn execute_review(
    trans: &Transaction,
    req: &ReviewReq,
    submit_user_id: i64,
) -> Result<ReviewResult, routes::Error> {
    let sid = req.sid;

    let res = match &req.action {
        ReviewAction::Reject => {
            sql::ins_submit_reject(trans, sid)
                .context("ins_submit_reject")?;

            ReviewResult::Reject
        }
        ReviewAction::Pass(groups) => {
            let last_pass_sid =
                sql::get_last_passed_submit_id_by_user_id(
                    trans,
                    submit_user_id,
                )
                .context("get_last_passed_submit_id_by_user_id")?
                .map(|it| it.id);

            let res = sql::ins_submit_pass(trans, sid)
                .context("ins_submit_pass")?
                .context("success should return id")?;
            let pass_id = res.id;

            for group in groups {
                sql::ins_submit_pass_group(
                    trans,
                    pass_id,
                    group.to_string(),
                )
                .context("ins_submit_pass_group")?;
            }

            let ignored = if let Some(last_pass_sid) = last_pass_sid {
                let last_pass_groups =
                    Group::get_passed_groups_by_submit_id(
                        trans,
                        last_pass_sid,
                    )?;

                tracing::debug!("last groups: {last_pass_groups:?}");

                let have_new =
                    groups.difference(&last_pass_groups).next().is_some();
                let missing_old =
                    last_pass_groups.difference(groups).next().is_some();

                if missing_old && have_new {
                    let new = groups
                        .difference(&last_pass_groups)
                        .collect::<Box<_>>();
                    let old = last_pass_groups
                        .difference(groups)
                        .collect::<Box<_>>();
                    tracing::debug!(
                        "both missing old group({old:?}) \
and have new group({new:?})"
                    );

                    let msg = format!(
                        "expect not both \
missing old group({old:?}) and have new group({new:?})"
                    );

                    return Err(ProblemDetails::from_status_code(
                        StatusCode::CONFLICT,
                    )
                    .with_detail(msg)
                    .into());
                } else if !missing_old {
                    tracing::debug!(
                        "replacing last one: (sid){last_pass_sid}"
                    );
                    sql::ins_submit_replace(trans, last_pass_sid)
                        .context("ins_submit_replace last")?;
                    false
                } else {
                    tracing::debug!("ignore current one: (sid){sid}");
                    sql::ins_submit_replace(trans, sid)
                        .context("ins_submit_replace current")?;
                    true
                }
            } else {
                false
            };

            ReviewResult::Pass {
                groups: groups.clone(),
                ignored,
            }
        }
    };

    Ok(res)
}
