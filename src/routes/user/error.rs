use askama::Template;
use axum::{
    extract::Path,
    http::StatusCode,
    response::{Html, IntoResponse},
};
use problem_details::ProblemDetails;
use strum::IntoEnumIterator as _;

use crate::routes::{
    self,
    auth::access_token::{self, AccessToken},
    file,
    user::{extra_file, submit::SIZE_RANGE},
};

#[derive(Debug, askama::Template)]
#[template(path = "user/error/file_type.html")]
struct FileTypeTemplate<'a> {
    file_types: &'a [&'a str],
}

#[derive(Debug, askama::Template)]
#[template(path = "user/error/file_size.html")]
struct FileSizeTemplate {
    size_min: usize,
    size_max: usize,
}

#[derive(Debug, askama::Template)]
#[template(path = "user/error/max_pending.html")]
struct MaxPendingTemplate;

#[derive(Debug, askama::Template)]
#[template(path = "user/error/max_extra_file.html")]
struct MaxExtraFileTemplate {
    limit: u64,
}

#[expect(clippy::missing_errors_doc)]
pub async fn handler(
    _token: AccessToken<access_token::User>,
    Path(error): Path<Box<str>>,
) -> routes::Result<impl IntoResponse> {
    let page = match &*error {
        "file_type" => {
            let file_types = file::Type::iter()
                .map(file::Type::to_ext)
                .collect::<Box<_>>();

            FileTypeTemplate {
                file_types: &file_types,
            }
            .render()?
        }
        "file_size" => FileSizeTemplate {
            size_min: *SIZE_RANGE.start(),
            size_max: *SIZE_RANGE.end(),
        }
        .render()?,
        "max_pending" => MaxPendingTemplate.render()?,
        "max_extra_file" => MaxExtraFileTemplate {
            limit: extra_file::MAX_COUNT,
        }
        .render()?,
        name => {
            tracing::info!("unknown error page: {name:?}");
            return Err(ProblemDetails::from_status_code(
                StatusCode::NOT_FOUND,
            )
            .into());
        }
    };

    Ok(Html(page))
}
