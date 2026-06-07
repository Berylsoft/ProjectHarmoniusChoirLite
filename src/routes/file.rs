use anyhow::Context as _;
use mime::Mime;
use rusqlite::Connection;
use time::OffsetDateTime;

use crate::{routes, sql};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, strum::EnumIter)]
pub enum Type {
    Wav,
    Flac,
    Ogg,
    Mp3,
    M4a,
    Aac,
}

impl Type {
    #[must_use]
    pub fn detect(head: &[u8; 12]) -> Option<Self> {
        if &head[..4] == b"RIFF" && &head[8..12] == b"WAVE" {
            Some(Self::Wav)
        } else if &head[..4] == b"fLaC" {
            Some(Self::Flac)
        } else if &head[..4] == b"OggS" {
            Some(Self::Ogg)
        } else if &head[..3] == b"ID3"
            || (head[..2] == [0xff, 0xfb]
                || head[..2] == [0xff, 0xf3]
                || head[..2] == [0xff, 0xf2])
        {
            Some(Self::Mp3)
        } else if &head[4..12] == b"ftypM4A " {
            Some(Self::M4a)
        } else if head[..2] == [0xff, 0xf1] || head[..2] == [0xff, 0xf9] {
            Some(Self::Aac)
        } else {
            None
        }
    }

    #[must_use]
    pub const fn to_ext(self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Flac => "flac",
            Self::Ogg => "ogg",
            Self::Mp3 => "mp3",
            Self::M4a => "m4a",
            Self::Aac => "aac",
        }
    }

    #[expect(clippy::missing_panics_doc)]
    #[must_use]
    pub fn to_mime(self) -> Mime {
        match self {
            Self::Wav => "audio/wav",
            Self::Flac => "audio/flac",
            Self::Ogg => "application/ogg",
            Self::Mp3 => "audio/mpeg",
            Self::M4a => "audio/mp4",
            Self::Aac => "audio/aac",
        }
        .parse()
        .unwrap()
    }

    #[must_use]
    pub fn try_from_mime(mime: &Mime) -> Option<Self> {
        Some(match (mime.type_().as_str(), mime.subtype().as_str()) {
            ("audio", "wav") => Self::Wav,
            ("audio", "flac") => Self::Flac,
            ("application", "ogg") => Self::Ogg,
            ("audio", "mpeg") => Self::Mp3,
            ("audio", "mp4") => Self::M4a,
            ("audio", "aac") => Self::Aac,
            _ => return None,
        })
    }
}

#[derive(Debug)]
pub struct Info {
    pub hash: blake3::Hash,

    pub uid: i64,
    pub user_signature: Box<str>,
    pub hgi: bool,
    pub nth: i64,
    pub ty: Type,
    pub created_at: OffsetDateTime,
}

impl Info {
    #[expect(clippy::missing_errors_doc)]
    pub fn get_by_submit_id(
        conn: &Connection,
        sid: i64,
    ) -> routes::Result<Option<Self>> {
        let info = sql::get_file_info_by_submit_id(conn, sid)
            .context("get_file_info_by_submit_id")?;
        let Some(info) = info else {
            return Ok(None);
        };
        Ok(Some(Self {
            hash: blake3::Hash::from_slice(&info.file_hash)
                .context("file hash")?,
            uid: info.user_id,
            user_signature: info.user_signature.into(),
            hgi: info.harmony_group_intention,
            nth: info.nth,
            ty: Type::try_from_mime(
                &info.file_mime_type.parse().context("parse mime")?,
            )
            .context("type from mime")?,
            created_at: OffsetDateTime::parse(
                &info.created_at,
                &time::format_description::well_known::Rfc3339,
            )
            .context("parse created_at")?,
        }))
    }

    #[expect(clippy::missing_errors_doc)]
    pub fn get_all_passed(
        conn: &Connection,
    ) -> routes::Result<Box<[Self]>> {
        sql::get_file_infos_of_all_passed_submits(conn)
            .context("get_file_infos_of_all_passed_submits")?
            .into_iter()
            .map(|info| {
                Ok(Self {
                    hash: blake3::Hash::from_slice(&info.file_hash)
                        .context("file hash")?,
                    uid: info.user_id,
                    user_signature: info.user_signature.into(),
                    hgi: info.harmony_group_intention,
                    nth: info.nth,
                    ty: Type::try_from_mime(
                        &info
                            .file_mime_type
                            .parse()
                            .context("parse mime")?,
                    )
                    .context("type from mime")?,
                    created_at: OffsetDateTime::parse(
                        &info.created_at,
                        &time::format_description::well_known::Rfc3339,
                    )
                    .context("parse created_at")?,
                })
            })
            .collect::<Result<Box<[_]>, _>>()
    }

    #[must_use]
    pub fn to_sanitized_file_name(&self) -> Box<str> {
        let uid = self.uid;
        let name = {
            const REPLACE: char = '_';

            let mut out =
                String::with_capacity(self.user_signature.len());
            for ch in self.user_signature.chars() {
                match ch {
                    '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>'
                    | '|' | '\0' => {
                        out.push(REPLACE);
                    }
                    ch => {
                        out.push(ch);
                    }
                }
            }
            out.into_boxed_str()
        };
        let hgi = if self.hgi { "有" } else { "无" };
        let nth = self.nth;
        let ext = self.ty.to_ext();

        format!("第四届_{uid}_{name}_{hgi}_第{nth}次.{ext}")
            .into_boxed_str()
    }
}
