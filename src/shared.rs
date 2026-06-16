use std::{collections::HashSet, str::FromStr};

use anyhow::Context as _;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::{Transaction, routes, sql};

pub const COMMENT_LEN_LIMIT_BYTES: usize = 256;

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    strum::Display,
    strum::EnumString,
)]
pub enum Group {
    Choir,
    Lead,
    Harmony,
}

impl Group {
    pub const REQUIRED: Self = Self::Choir;

    /// # Errors
    ///
    /// if submit not exists or
    /// not passed with groups or
    /// stored group name is invalid.
    pub fn get_passed_groups_by_submit_id(
        conn: &Connection,
        submit_id: i64,
    ) -> routes::Result<HashSet<Self>> {
        let groups = sql::get_passed_groups_by_submit_id(conn, submit_id)
            .context("get_passed_groups_by_submit_id")?;

        if groups.is_empty() {
            return Err(anyhow::anyhow!(
                "submit {submit_id} should exists \
and passed with groups"
            )
            .into());
        }

        groups
            .into_iter()
            .map(|it| {
                Self::from_str(&it.group_name).with_context(|| {
                    format!("parsing group name: {}", it.group_name)
                })
            })
            .collect::<Result<HashSet<_>, _>>()
            .map_err(Into::into)
    }
}

#[derive(Debug)]
pub enum Status {
    Pending,
    Rejected {
        comment: Box<str>,
    },
    Passed {
        groups: HashSet<Group>,
        replaced: bool,
        comment: Box<str>,
    },
    Replaced,
}

impl Status {
    /// # Errors
    ///
    /// if input is invalid status or see [`Group::get_passed_groups_by_submit_id`].
    pub fn from_submit(
        trans: &Transaction,
        sid: i64,
        rejected: bool,
        passed: bool,
        replaced: bool,
    ) -> routes::Result<Self> {
        let status = [rejected, passed, replaced];
        Ok(match status {
            [false, false, false] => Self::Pending,
            [true, false, false] => {
                let comment =
                    sql::get_comment_of_reject_by_submit_id(trans, sid)
                        .context("get_comment_of_reject_by_submit_id")?
                        .context("comment of reject")?
                        .comment;
                Self::Rejected {
                    comment: comment.into(),
                }
            }
            [false, false, true] => Self::Replaced,
            [false, true, replaced] => {
                let groups =
                    Group::get_passed_groups_by_submit_id(trans, sid)?;
                let comment =
                    sql::get_comment_of_pass_by_submit_id(trans, sid)
                        .context("get_comment_of_pass_by_submit_id")?
                        .context("comment of pass")?
                        .comment;

                Self::Passed {
                    groups,
                    replaced,
                    comment: comment.into(),
                }
            }
            _ => {
                return Err(anyhow::anyhow!(
                    "unknown status: {status:?}, (sid){sid}"
                )
                .into());
            }
        })
    }
}

#[expect(clippy::struct_excessive_bools)]
#[derive(Debug)]
pub struct StatusFlat {
    pub pending: bool,
    pub rejected: bool,
    pub replaced: bool,
    pub passed: bool,
    pub lead: bool,
    pub harmony: bool,
    pub comment: Box<str>,
}

impl From<Status> for StatusFlat {
    fn from(value: Status) -> Self {
        let pending = matches!(value, Status::Pending);
        let rejected = matches!(value, Status::Rejected { .. });

        let mut replaced = matches!(value, Status::Replaced);
        let (passed, lead, harmony) = if let Status::Passed {
            groups,
            replaced: r,
            ..
        } = &value
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

        let comment = match value {
            Status::Rejected { comment }
            | Status::Passed { comment, .. } => comment,
            Status::Pending | Status::Replaced => Default::default(),
        };

        Self {
            pending,
            rejected,
            replaced,
            passed,
            lead,
            harmony,
            comment,
        }
    }
}
