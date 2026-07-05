use askama::Template;
use axum::response::{Html, IntoResponse};

use crate::routes::{
    self,
    auth::access_token::{self, AccessToken},
    user::submit::SIZE_RANGE,
};

#[derive(Debug, askama::Template)]
#[template(path = "user/extra_file.html")]
struct ViewTemplate {
    size_min: usize,
    size_max: usize,
}

#[expect(clippy::missing_errors_doc)]
pub async fn handler(
    _token: AccessToken<access_token::User>,
) -> routes::Result<impl IntoResponse> {
    let size_min = *SIZE_RANGE.start();
    let size_max = *SIZE_RANGE.end();

    Ok(Html(ViewTemplate { size_min, size_max }.render()?))
}
