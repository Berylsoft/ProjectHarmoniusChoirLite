use std::{fs, str::FromStr};

use anyhow::Context as _;
use axum::{
    extract::{
        Multipart,
        multipart::{self, Field, MultipartError},
    },
    http::StatusCode,
    response::IntoResponse,
};
use mime::Mime;
use problem_details::ProblemDetails;
use tempfile::NamedTempFile;
use tokio::io::AsyncWriteExt as _;

use super::submit::SIZE_RANGE;
use crate::{
    RedirPrefix, TransactionDeferBegin,
    routes::{
        self,
        auth::access_token::{self, AccessToken},
        file,
    },
    sql,
    utils::{
        hash_to_storage_path, length_check_quick, res_see_other,
        res_see_other_err_res, warn_problem_general,
    },
};

pub const MAX_COUNT: u64 = 5;

#[derive(Debug)]
pub struct ExtraFileReq {
    file: Option<NamedTempFile<tokio::fs::File>>,
    mime: Mime,
    hash: blake3::Hash,
    file_name: String,
}

impl ExtraFileReq {
    fn move_to_storage(&mut self) -> anyhow::Result<()> {
        let path = hash_to_storage_path(self.hash);
        let parent =
            path.parent().context("get parent of storage path")?;
        if !parent.exists() {
            fs::create_dir_all(parent).context("create parent dir")?;
        }

        self.file
            .take()
            .context("expect haven't been moved")?
            .persist(path)
            .context("persist file into storage")?;
        Ok(())
    }
}

#[expect(clippy::missing_errors_doc)]
pub async fn handler(
    token: AccessToken<access_token::User>,
    trans: TransactionDeferBegin,
    redir_prefix: RedirPrefix,
    multipart: Multipart,
) -> routes::Result<impl IntoResponse> {
    tracing::debug!("handling extra file by (uid){}", token.uid());

    let mut req = receive_into_tmp(multipart, &redir_prefix).await?;

    let trans = trans.begin().await?;

    let user_have_passed =
        sql::is_user_have_passed_submit(&trans, token.uid())
            .context("is_user_have_passed_submit")?
            .context("is_user_have_passed_submit option")?
            .have_passed;

    if !user_have_passed {
        trans.commit().await?;

        warn_problem_general("user doesn't have passed submit");

        return Err(ProblemDetails::from_status_code(
            StatusCode::CONFLICT,
        )
        .with_detail("doesn't have passed submit")
        .into());
    }

    let existing_count =
        sql::get_extra_file_count_by_user_id(&trans, token.uid())
            .context("get_extra_file_count_by_user_id")?
            .context("get_extra_file_count_by_user_id option")?
            .count;

    if existing_count.cast_unsigned() >= 5 {
        trans.commit().await?;

        return res_see_other_err_res(
            &redir_prefix,
            "/user/error/max_extra_file",
        );
    }

    req.move_to_storage().context("move_to_storage")?;

    let created_at = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .context("format current time as RFC3339")?;

    let ext_file_id = sql::ins_extra_file(
        &trans,
        token.uid(),
        req.hash.as_bytes().to_vec(),
        req.file_name,
        req.mime.to_string(),
        created_at,
    )
    .context("ins_extra_file")?
    .context("ins_extra_file should return id")?
    .id;

    trans.commit().await?;

    tracing::debug!("uploaded: (ext_file_id){ext_file_id}");

    Ok(res_see_other(&redir_prefix, "/user/submits"))
}

async fn receive_into_tmp(
    mut multipart: Multipart,
    redir_prefix: &RedirPrefix,
) -> Result<ExtraFileReq, routes::Error> {
    let mut file = None;

    let res = async {
        while let Some(mut field) =
            multipart.next_field().await.map_err(map_multipart_err)?
        {
            let name = field.name().ok_or_else(|| {
                tracing::debug!("expect field name in multipart body");
                ProblemDetails::from_status_code(StatusCode::BAD_REQUEST)
                    .with_detail("expect field name")
            })?;

            macro_rules! check_dup {
                ($var:ident) => {
                    if $var.is_some() {
                        tracing::debug!(concat!(
                            "duplicated field: ",
                            stringify!($var)
                        ));
                        drain_field(&mut field).await?;
                        return bad_req(concat!(
                            "duplicated field: ",
                            stringify!($var)
                        ));
                    }
                };
            }

            if name == "file" {
                check_dup!(file);
                file = Some(receive_file_tmp(field, redir_prefix).await?);
            } else {
                let name = name.to_owned();
                tracing::debug!("unknown field name {name}");
                drain_field(&mut field).await?;
                return bad_req(&format!("unknown field name: {name}"));
            }
        }
        Ok(())
    }
    .await;

    match res {
        Ok(()) => {}
        Err(err @ routes::Error::Problem(_)) => {
            drain_multipart(multipart).await?;
            return Err(err);
        }
        Err(err) => return Err(err),
    }

    let Some(file) = file else {
        tracing::warn!("missing file");
        return bad_req("missing file");
    };

    let (file, mime, hasher, file_name) = file;
    let hash = hasher.finalize();

    tracing::debug!(
        "file size={}, hash={}",
        hasher.count(),
        hash.to_hex()
    );

    Ok(ExtraFileReq {
        file: Some(file),
        mime,
        hash,
        file_name,
    })
}

async fn receive_file_tmp(
    mut field: multipart::Field<'_>,
    redir_prefix: &RedirPrefix,
) -> Result<
    (NamedTempFile<tokio::fs::File>, Mime, blake3::Hasher, String),
    routes::Error,
> {
    let Some(content_type) = field.content_type() else {
        tracing::debug!("expect content type for file");
        return bad_req("expect content type");
    };

    let Some(file_name) = field.file_name() else {
        tracing::debug!("expect file name for file");
        return bad_req("expect file name");
    };

    if !length_check_quick(file_name, 255) {
        tracing::debug!("file name too long: {}bytes", file_name.len());
        return bad_req("file name too long");
    }

    let file_name = file_name.to_owned();

    let mut content_type = match Mime::from_str(content_type) {
        Ok(content_type) => content_type,
        Err(err) => {
            tracing::debug!("invalid content type for file: {err}");
            return bad_req("invalid content type");
        }
    };

    let mut hasher = blake3::Hasher::new();
    let temp_file = tempfile::Builder::default()
        .tempfile_in("./tmp")
        .context("create temp file for receive")?;
    let mut temp_file = {
        let (file, path) = temp_file.into_parts();
        let file = tokio::fs::File::from_std(file);
        NamedTempFile::from_parts(file, path)
    };

    let file_size_err =
        || res_see_other_err_res(redir_prefix, "/user/error/file_size");

    let mut head = Some([0_u8; file::Type::MIN_SIZE]);
    let mut head_idx = 0;
    while let Some(chunk) =
        field.chunk().await.map_err(map_multipart_err)?
    {
        let res = check_content_type(
            &mut content_type,
            &mut head,
            &mut head_idx,
            &chunk,
            redir_prefix,
        );
        if let Err(err) = res {
            drain_field(&mut field).await?;
            return Err(err);
        }

        if !usize::try_from(hasher.count())
            .is_ok_and(|it| it + chunk.len() < *SIZE_RANGE.end())
        {
            tracing::debug!("file size too large");
            drain_field(&mut field).await?;
            return file_size_err();
        }

        hasher.update(&chunk);
        temp_file
            .as_file_mut()
            .write_all(&chunk)
            .await
            .context("write into temp file")?;
    }

    if usize::try_from(hasher.count()).unwrap_or(usize::MAX)
        < *SIZE_RANGE.start()
        || head.is_some()
    {
        tracing::debug!("file size too small");
        return file_size_err();
    }

    Ok((temp_file, content_type, hasher, file_name))
}

#[inline]
fn check_content_type(
    content_type: &mut Mime,
    head: &mut Option<[u8; file::Type::MIN_SIZE]>,
    head_idx: &mut usize,
    chunk: &axum::body::Bytes,
    redir_prefix: &RedirPrefix,
) -> Result<(), routes::Error> {
    let Some(buf) = head else {
        return Ok(());
    };

    for ch in chunk {
        if *head_idx >= file::Type::MIN_SIZE {
            break;
        }
        buf[*head_idx] = *ch;
        *head_idx += 1;
    }

    if *head_idx < file::Type::MIN_SIZE {
        return Ok(());
    }

    if let Some(ty) = file::Type::detect(buf) {
        *content_type = ty.to_mime();
    } else {
        tracing::debug!(
            "unknown file content type, header: {content_type}"
        );
        return res_see_other_err_res(
            redir_prefix,
            "/user/error/file_type",
        );
    }

    *head = None;

    Ok(())
}

async fn drain_field(field: &mut Field<'_>) -> routes::Result<()> {
    while field.chunk().await.map_err(map_multipart_err)?.is_some() {}
    Ok(())
}

async fn drain_multipart(mut multipart: Multipart) -> routes::Result<()> {
    while multipart
        .next_field()
        .await
        .map_err(map_multipart_err)?
        .is_some()
    {}

    Ok(())
}

#[expect(clippy::needless_pass_by_value, reason = "for map_err")]
fn map_multipart_err(err: MultipartError) -> routes::Error {
    tracing::debug!("failed to receive multipart body: {err:?}");
    ProblemDetails::from_status_code(StatusCode::BAD_REQUEST)
        .with_detail("bad body")
        .into()
}

fn bad_req<T>(msg: &str) -> Result<T, routes::Error> {
    Err(ProblemDetails::from_status_code(StatusCode::BAD_REQUEST)
        .with_detail(msg)
        .into())
}
