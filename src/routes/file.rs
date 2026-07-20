use anyhow::Context as _;
use mime::Mime;
use rusqlite::Connection;
use time::OffsetDateTime;

use crate::{routes, sql, utils::parse_rfc3339};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, strum::EnumIter)]
pub enum Type {
    Wav,
    Flac,
    Ogg,
    Mp3,
    Mp4,
    M4a,
    _3gpp,
    Mpeg4Generic,
    Aac,
}

impl Type {
    pub const MIN_SIZE: usize = 0x20;

    #[must_use]
    pub fn detect(head: &[u8; Self::MIN_SIZE]) -> Option<Self> {
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
        } else if &head[4..12] == b"ftypmp42" {
            Some(Self::Mp4)
        } else if &head[4..12] == b"ftypM4A " {
            Some(Self::M4a)
        } else if &head[4..12] == b"ftyp3gp4" {
            Some(Self::_3gpp)
        } else if &head[4..8] == b"ftyp" && Self::is_mpeg4_generic(head) {
            Some(Self::Mpeg4Generic)
        } else if head[..2] == [0xff, 0xf1] || head[..2] == [0xff, 0xf9] {
            Some(Self::Aac)
        } else {
            None
        }
    }

    fn is_mpeg4_generic(head: &[u8; Self::MIN_SIZE]) -> bool {
        let box_size =
            (u32::from_be_bytes((&head[..4]).try_into().unwrap())
                as usize)
                .min(Self::MIN_SIZE);
        if !(box_size > 0x10 && box_size.is_multiple_of(4)) {
            return false;
        }

        (0..((box_size - 0x10) / 4)).any(|idx| {
            let start = 0x10 + idx * 4;
            let end = start + 4;
            matches!(&head[start..end], b"iso6" | b"isom" | b"avc1")
        })
    }

    #[must_use]
    pub const fn to_ext(self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Flac => "flac",
            Self::Ogg => "ogg",
            Self::Mp3 => "mp3",
            Self::Mp4 => "mp4",
            Self::M4a => "m4a",
            Self::_3gpp => "3gp",
            Self::Mpeg4Generic => "mpeg4",
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
            Self::Mp4 => "audio/mp4",
            Self::M4a => "audio/x-m4a",
            Self::_3gpp => "audio/3gpp",
            Self::Mpeg4Generic => "application/mpeg4-generic",
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
            ("audio", "mp4") => Self::Mp4,
            ("audio", "x-m4a") => Self::M4a,
            ("audio", "3gpp") => Self::_3gpp,
            ("application", "mpeg4-generic") => Self::Mpeg4Generic,
            ("audio", "aac") => Self::Aac,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Info {
    pub hash: blake3::Hash,

    pub uid: i64,
    pub user_signature: Box<str>,
    pub hgi: bool,
    pub nth: i64,
    pub ty: Type,
    pub created_at: OffsetDateTime,

    pub extra_id: Option<u64>,
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
            created_at: parse_rfc3339(&info.created_at)
                .context("parse created_at")?,
            extra_id: None,
        }))
    }

    #[expect(clippy::missing_errors_doc)]
    pub fn get_all_passed(
        conn: &Connection,
    ) -> routes::Result<Box<[Self]>> {
        let infos = sql::get_file_infos_of_all_passed_submits(conn)
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
                    created_at: parse_rfc3339(&info.created_at)
                        .context("parse created_at")?,
                    extra_id: None,
                })
            })
            .collect::<routes::Result<Box<[_]>>>()?;

        let mut result = Vec::with_capacity(infos.len());

        for info in infos {
            let mut extra_files =
                sql::get_all_extra_files_by_user_id(conn, info.uid)
                    .context("get_all_extra_files_by_user_id")?;

            extra_files.sort_by_key(|it| it.id);

            for extra_file in extra_files {
                let mut extra_info = info.clone();
                extra_info.hash =
                    blake3::Hash::from_slice(&extra_file.file_hash)
                        .context("extra file hash")?;
                extra_info.ty = Type::try_from_mime(
                    &extra_file
                        .file_mime_type
                        .parse()
                        .context("parse mime")?,
                )
                .context("type from mime")?;
                extra_info.created_at =
                    parse_rfc3339(&extra_file.created_at)
                        .context("parse created_at")?;
                extra_info.extra_id = Some(extra_file.id.cast_unsigned());

                result.push(extra_info);
            }
            result.push(info);
        }

        Ok(result.into())
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

        let extra = self
            .extra_id
            .map_or_else(String::new, |extra_id| format!("_{extra_id}"));

        format!("第四届_{uid}_{name}_{hgi}_第{nth}次{extra}.{ext}")
            .into_boxed_str()
    }
}
