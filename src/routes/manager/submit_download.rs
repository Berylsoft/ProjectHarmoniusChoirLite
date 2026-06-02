use anyhow::Context as _;
use axum::{
    extract,
    http::{StatusCode, header},
    response::IntoResponse,
};
use problem_details::ProblemDetails;

use crate::{
    Transaction,
    routes::{
        self,
        auth::access_token::{self, AccessToken},
        file,
    },
    utils::{hash_to_storage_path, rfc5987_utf8, warn_problem_general},
};

#[expect(clippy::missing_errors_doc)]
pub async fn handler(
    _token: AccessToken<access_token::Manager>,
    extract::Path(sid): extract::Path<i64>,
    trans: Transaction,
) -> routes::Result<impl IntoResponse> {
    let info = file::Info::get_by_submit_id(&trans, sid)?;

    let Some(info) = info else {
        warn_problem_general("submit not found");
        return Err(ProblemDetails::from_status_code(
            StatusCode::NOT_FOUND,
        )
        .with_detail("submit not found")
        .into());
    };

    trans.commit().await?;

    let path = hash_to_storage_path(info.hash);
    let len =
        std::fs::metadata(&path).context("get file metadata")?.len();
    let name = info.to_sanitized_file_name();
    let name_rfc5987 = rfc5987_utf8(&name);
    let name = name
        .chars()
        .filter(|it| !it.is_ascii_control())
        .collect::<Box<str>>()
        .replace('"', "\\\"");

    let content_length = (header::CONTENT_LENGTH, len.to_string());
    let content_type =
        (header::CONTENT_TYPE, info.ty.to_mime().to_string());
    let disposition = (
        header::CONTENT_DISPOSITION,
        format!(r#"inline; filename="{name}"; filename*={name_rfc5987}"#),
    );

    let file = tokio::fs::OpenOptions::new()
        .read(true)
        .open(path)
        .await
        .context("open file")?;
    let stream = tokio_util::io::ReaderStream::new(file);
    let body = axum::body::Body::from_stream(stream);
    Ok(([content_length, content_type, disposition], body))
}
