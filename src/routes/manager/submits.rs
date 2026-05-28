use std::convert::identity;

use anyhow::Context as _;
use askama::Template;
use axum::{
    extract::Query,
    response::{Html, IntoResponse},
};
use serde::Deserialize;

use crate::{
    Transaction,
    routes::{
        self,
        auth::access_token::{self, AccessToken},
    },
    shared::Status,
    sql,
};

#[expect(clippy::struct_excessive_bools)]
#[derive(Debug, Deserialize)]
pub struct Filter {
    #[serde(default)]
    pending: bool,
    #[serde(default)]
    rejected: bool,
    #[serde(default)]
    passed: bool,
    #[serde(default)]
    replaced: bool,
}

#[expect(clippy::missing_errors_doc)]
pub async fn handler(
    _token: AccessToken<access_token::Manager>,
    Query(filter): Query<Filter>,
    trans: Transaction,
) -> routes::Result<impl IntoResponse> {
    let submits =
        sql::get_all_submits(&trans).context("get_all_submits")?;

    let mut items = Vec::with_capacity(submits.len());

    let filter_mask = [filter.rejected, filter.passed, filter.replaced];
    for submit in &submits {
        let status = [submit.rejected, submit.passed, submit.replaced];
        if filter.pending && status.into_iter().any(identity) {
            continue;
        }

        if filter_mask.into_iter().any(identity)
            && status.into_iter().zip(filter_mask).any(|(a, b)| a ^ b)
        {
            continue;
        }

        let status = Status::from_submit(
            &trans,
            submit.id,
            submit.rejected,
            submit.passed,
            submit.replaced,
        )?;

        items.push(Item {
            id: submit.id,
            user_id: submit.user_id,
            nth: submit.nth,
            hgi: submit.harmony_group_intention,
            signature: &submit.user_signature,
            created_at: &submit.created_at,
            status,
        });
    }

    let enabled_filters = filter_mask
        .into_iter()
        .enumerate()
        .filter(|&(_, enabled)| enabled)
        .map(|(idx, _)| match idx {
            0 => "已拒绝",
            1 => "已通过",
            2 => "被替代",
            _ => unreachable!(),
        })
        .collect::<Box<_>>();

    Ok(Html(
        ViewTemplate {
            filter,
            enabled_filters: &enabled_filters,
            items: &items,
        }
        .render()?,
    ))
}

#[derive(Debug, Template)]
#[template(path = "manager/submits.html")]
struct ViewTemplate<'a> {
    filter: Filter,
    enabled_filters: &'a [&'static str],
    items: &'a [Item<'a>],
}

#[derive(Debug)]
struct Item<'a> {
    id: i64,
    user_id: i64,
    nth: i64,
    signature: &'a str,
    hgi: bool,
    created_at: &'a str,
    status: Status,
}
