use std::{fs, str::FromStr};

use anyhow::Context as _;
use axum::{
    extract::{
        Multipart,
        multipart::{self, MultipartError},
    },
    http::StatusCode,
    response::IntoResponse,
};
use mime::Mime;
use problem_details::ProblemDetails;
use tempfile::NamedTempFile;
use tokio::io::AsyncWriteExt as _;

use crate::{
    TransactionDeferBegin,
    routes::{
        self,
        auth::access_token::{self, AccessToken},
    },
    sql,
    utils::{hash_to_storage_path, is_valid_user_signature},
};

#[expect(clippy::missing_errors_doc)]
pub async fn handler(
    token: AccessToken<access_token::User>,
    trans: TransactionDeferBegin,
    multipart: Multipart,
) -> routes::Result<impl IntoResponse> {
    tracing::debug!("handling submit by (uid){}", token.uid());
    let (signature, mime, hash) = receive_into_storage(multipart).await?;

    // TODO: limit submit count?

    let trans = trans.begin().await?;

    let pending = sql::get_pending_submit_by_user_id(&trans, token.uid())
        .context("get_pending_submit_by_user_id")?;

    let cur_nth = if let Some(pending) = pending {
        sql::ins_submit_replace(&trans, pending.id)
            .context("ins_submit_replace")?;
        pending.nth
    } else {
        sql::get_max_submit_nth_by_user_id(&trans, token.uid())
            .context("get_max_submit_nth_by_user_id")?
            .context("should always return nth")?
            .nth
    };

    let nth = cur_nth + 1;

    let created_at = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .context("format current time as RFC3339")?;

    let sid = sql::ins_submit(
        &trans,
        token.uid(),
        nth,
        signature,
        hash.as_bytes().to_vec(),
        mime.to_string(),
        created_at,
    )
    .context("ins_submit")?
    .context("should return id")?
    .id;

    trans.commit().await?;

    tracing::debug!("submitted: (sid){sid}");

    Ok(())
}

async fn receive_into_storage(
    mut multipart: Multipart,
) -> Result<(String, Mime, blake3::Hash), routes::Error> {
    let mut signature = None;
    let mut file = None;

    while let Some(field) =
        multipart.next_field().await.map_err(map_multipart_err)?
    {
        let name = field.name().ok_or_else(|| {
            tracing::debug!("expect field name in multipart body");
            ProblemDetails::from_status_code(StatusCode::BAD_REQUEST)
                .with_detail("expect field name")
        })?;

        match name {
            "signature" => {
                if signature.is_some() {
                    tracing::debug!("duplicated field: signature");
                    return bad_req("duplicated field: signature");
                }

                let bytes =
                    field.bytes().await.map_err(map_multipart_err)?;
                let sig = str::from_utf8(&bytes);
                match sig {
                    Ok(sig) => {
                        if !is_valid_user_signature(sig) {
                            tracing::debug!("invalid user signature");
                            return bad_req("invalid signature");
                        }

                        signature = Some(sig.to_owned());
                    }
                    Err(err) => {
                        tracing::debug!(
                            "expect utf8 in field signature, but: {err}"
                        );
                        return bad_req("expect utf8 in field signature");
                    }
                }
            }
            "file" => {
                if file.is_some() {
                    tracing::debug!("duplicated field: file");
                    return bad_req("duplicated field: file");
                }

                file = Some(receive_file_tmp(field).await?);
            }
            _ => {
                tracing::debug!("unknown field name {name}");
                return bad_req(&format!("unknown field name: {name}"));
            }
        }
    }

    let exists = (signature.is_some(), file.is_some());
    let (Some(signature), Some(file)) = (signature, file) else {
        tracing::warn!(
            "missing field, exists: signature={}, file={}",
            exists.0,
            exists.1
        );
        return bad_req("missing field");
    };

    let (file, mime, hasher) = file;
    let hash = hasher.finalize();

    let path = hash_to_storage_path(hash);
    let parent = path.parent().context("get parent of storage path")?;
    if !parent.exists() {
        fs::create_dir_all(parent).context("create parent dir")?;
    }
    file.persist(path).context("persist file into storage")?;

    tracing::debug!(
        "user signature={signature:?}, file size={}, hash={}",
        hasher.count(),
        hash.to_hex()
    );

    Ok((signature, mime, hash))
}

async fn receive_file_tmp(
    mut field: multipart::Field<'_>,
) -> Result<
    (NamedTempFile<tokio::fs::File>, Mime, blake3::Hasher),
    routes::Error,
> {
    let Some(content_type) = field.content_type() else {
        tracing::debug!("expect content type for file");
        return bad_req("expect content type");
    };

    let content_type = match Mime::from_str(content_type) {
        Ok(content_type) => content_type,
        Err(err) => {
            tracing::debug!("invalid content type for file: {err}");
            return bad_req("invalid content type");
        }
    };

    // TODO: check content type is accepted

    let mut hasher = blake3::Hasher::new();
    let temp_file = tempfile::Builder::default()
        .tempfile_in("./tmp")
        .context("create temp file for receive")?;
    let mut temp_file = {
        let (file, path) = temp_file.into_parts();
        let file = tokio::fs::File::from_std(file);
        NamedTempFile::from_parts(file, path)
    };

    while let Some(chunk) =
        field.chunk().await.map_err(map_multipart_err)?
    {
        hasher.update(&chunk);
        temp_file
            .as_file_mut()
            .write_all(&chunk)
            .await
            .context("write into temp file")?;
    }

    // TODO: check file size in range

    Ok((temp_file, content_type, hasher))
}

#[expect(clippy::needless_pass_by_value, reason = "for map_err")]
fn map_multipart_err(err: MultipartError) -> routes::Error {
    tracing::debug!("failed to receive multipart body: {err}");
    ProblemDetails::from_status_code(StatusCode::BAD_REQUEST)
        .with_detail("bad body")
        .into()
}

fn bad_req<T>(msg: &str) -> Result<T, routes::Error> {
    Err(ProblemDetails::from_status_code(StatusCode::BAD_REQUEST)
        .with_detail(msg)
        .into())
}
