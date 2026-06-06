use axum::{
    Extension, Router, extract,
    http::{StatusCode, Uri, header, uri::Authority},
    middleware,
    response::IntoResponse,
    routing::get,
};
use include_dir::Dir;
use problem_details::{JsonProblemDetails, ProblemDetails};
use tower_http::{
    request_id::{PropagateRequestIdLayer, RequestId, SetRequestIdLayer},
    trace::TraceLayer,
};

use crate::{ServerState, utils::MakeRequestUlid};

pub mod auth;
pub mod bot;
pub mod file;
pub mod manager;
pub mod notify;
pub mod user;

pub type Payload<T> = axum::extract::Json<T>;

pub fn routes(state: ServerState) -> Router {
    Router::new()
        .route("/healthcheck", get(healthcheck))
        .route("/favicon.ico", get(favicon_ico))
        .route("/favicon.webp", get(favicon_webp))
        .route("/static/{*path}", get(static_file))
        .nest("/auth", auth::routes())
        .nest("/user", user::routes())
        .nest("/manager", manager::routes())
        .nest("/notify", notify::routes())
        .layer((
            SetRequestIdLayer::x_request_id(MakeRequestUlid),
            PropagateRequestIdLayer::x_request_id(),
            TraceLayer::new_for_http().make_span_with(make_span),
            middleware::from_fn(problem_detail_with_req_id),
        ))
        .with_state(state)
}

async fn healthcheck() -> impl IntoResponse {
    (
        StatusCode::NO_CONTENT,
        [(header::CACHE_CONTROL, "no-store")],
    )
}

async fn favicon_ico() -> Result<impl IntoResponse> {
    static_file(extract::Path("favicon.ico".into())).await
}

async fn favicon_webp() -> Result<impl IntoResponse> {
    static_file(extract::Path("favicon.webp".into())).await
}

async fn static_file(
    extract::Path(path): extract::Path<String>,
) -> Result<impl IntoResponse> {
    const STATIC_DIR: Dir<'_> =
        include_dir::include_dir!("$CARGO_MANIFEST_DIR/static");

    let Some(file) = STATIC_DIR.get_file(&path) else {
        return Err(ProblemDetails::from_status_code(
            StatusCode::NOT_FOUND,
        )
        .with_detail(path)
        .into());
    };

    let content_type = file
        .path()
        .extension()
        .and_then(|it| it.to_str())
        .map_or(mime::APPLICATION_OCTET_STREAM, |ext| match ext {
            "css" => mime::TEXT_CSS_UTF_8,
            "js" => mime::APPLICATION_JAVASCRIPT_UTF_8,
            "webp" => "image/webp".parse().unwrap(),
            "ico" => "image/vnd.microsoft.icon".parse().unwrap(),
            _ => mime::APPLICATION_OCTET_STREAM,
        });

    Ok((
        [(header::CONTENT_TYPE, content_type.to_string())],
        file.contents(),
    ))
}

fn make_span(
    req: &axum::http::Request<axum::body::Body>,
) -> tracing::Span {
    let req_id = req
        .extensions()
        .get::<RequestId>()
        .map(|it| it.header_value().as_bytes())
        .map(|it| String::from_utf8_lossy(it))
        .unwrap_or_default();

    tracing::info_span!(
        "request",
        method = %req.method(),
        uri = %req.uri(),
        version = ?req.version(),
        request_id = %req_id
    )
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("problem: {0}")]
    Problem(#[from] Box<ProblemDetails>),
    #[error("database: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("rendR: {0}")]
    Render(#[from] askama::Error),
    #[error("unknown: {0}")]
    Unknown(#[from] anyhow::Error),
    #[error("custom error response: {0:?}")]
    Custom(Box<axum::http::Response<axum::body::Body>>),
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        fn unexpected_err(err: &anyhow::Error) -> ProblemDetails {
            tracing::error!("unhandled error: {err:?}");
            ProblemDetails::from_status_code(
                StatusCode::INTERNAL_SERVER_ERROR,
            )
            .with_detail("please contact server administrator")
        }

        let problem = match self {
            Self::Problem(it) => *it,
            Self::Database(err) => unexpected_err(&err.into()),
            Self::Render(err) => unexpected_err(&err.into()),
            Self::Unknown(err) => unexpected_err(&err),
            Self::Custom(res) => return *res,
        };

        Extension(problem).into_response()
    }
}

impl From<ProblemDetails> for Error {
    fn from(value: ProblemDetails) -> Self {
        Self::Problem(value.into())
    }
}

impl From<axum::http::Response<axum::body::Body>> for Error {
    fn from(value: axum::http::Response<axum::body::Body>) -> Self {
        Self::Custom(value.into())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

async fn problem_detail_with_req_id(
    req: axum::http::Request<axum::body::Body>,
    next: middleware::Next,
) -> axum::http::Response<axum::body::Body> {
    let req_id = req.extensions().get::<RequestId>().cloned();
    let mut res = next.run(req).await;

    let Some(req_id) = req_id else {
        return res;
    };

    let Some(mut problem) =
        res.extensions_mut().remove::<ProblemDetails>()
    else {
        return res;
    };

    if problem.instance.is_none() {
        let id =
            String::from_utf8_lossy(req_id.header_value().as_bytes());

        let uri = Uri::builder()
            .scheme("reqid")
            .authority(Authority::from_static("reqid"))
            .path_and_query(format!("/{id}"))
            .build()
            .unwrap_or_default();

        problem.instance = Some(uri);
    }

    let problem: JsonProblemDetails = problem.into();
    problem.into_response()
}
