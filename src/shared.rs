use std::{collections::HashSet, str::FromStr};

use anyhow::Context as _;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::{Transaction, routes, sql};

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
    Rejected,
    Passed {
        groups: HashSet<Group>,
        replaced: bool,
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
            [true, false, false] => Self::Rejected,
            [false, false, true] => Self::Replaced,
            [false, true, replaced] => {
                let groups =
                    Group::get_passed_groups_by_submit_id(trans, sid)?;

                Self::Passed { groups, replaced }
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
