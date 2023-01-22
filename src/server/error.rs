use super::templates::{self, RenderError, RenderRucte};
use super::{language, response};
use diesel_async::pooled_connection::deadpool::PoolError;
use tracing::{event, Level};
use warp::http::status::StatusCode;
use warp::reply::Response;
use warp::{self, Rejection, Reply};

#[derive(Debug)]
pub enum ViewError {
    /// 404
    NotFound,
    /// 400
    BadRequest(String),
    /// 503
    ServiceUnavailable,
    /// 500
    Err(String),
}

pub trait ViewResult<T> {
    fn or_ise(self) -> Result<T, ViewError>;
}

impl<T, E> ViewResult<T> for Result<T, E>
where
    E: std::error::Error,
{
    fn or_ise(self) -> Result<T, ViewError> {
        self.map_err(|e| {
            event!(Level::ERROR, "Internal server error: {}", e);
            ViewError::Err("Something went wrong".into())
        })
    }
}

impl Reply for ViewError {
    fn into_response(self) -> Response {
        match self {
            ViewError::NotFound => error_response(
                StatusCode::NOT_FOUND,
                "Not found",
                "The page you tried to view does not exist. \
                 If you typed the url manually, maybe you did not type it \
                 correcly — or maybe you corrected a spelling error of mine?",
            ),
            ViewError::BadRequest(msg) => {
                error_response(StatusCode::BAD_REQUEST, &msg, "Sorry.")
            }
            ViewError::ServiceUnavailable => error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "Server exhausted",
                "The server is exhausted and can't handle your request \
                 right now. Sorry. \
                 Please try again later.",
            ),
            ViewError::Err(msg) => error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &msg,
                "This is an error in the server code or configuration. \
                 Sorry. \
                 The error has been logged and I will try to fix it.",
            ),
        }
    }
}

fn error_response(code: StatusCode, message: &str, detail: &str) -> Response {
    let fluent = language::load("en").unwrap();
    response()
        .status(code)
        .html(|o| templates::error_html(o, &fluent, code, message, detail))
        .unwrap()
}

impl From<RenderError> for ViewError {
    fn from(e: RenderError) -> Self {
        event!(Level::ERROR, "Rendering error: {}\n    {:?}", e, e);
        ViewError::Err("Rendering error".to_string())
    }
}

impl From<diesel::result::Error> for ViewError {
    fn from(e: diesel::result::Error) -> Self {
        event!(Level::ERROR, "Database error: {}\n    {:?}", e, e);
        ViewError::Err("Database error".to_string())
    }
}

impl From<PoolError> for ViewError {
    fn from(e: PoolError) -> Self {
        match e {
            PoolError::Timeout(kind) => {
                event!(Level::ERROR, "Db Pool timeout: {:?}", kind);
                ViewError::ServiceUnavailable
            }
            e => {
                event!(Level::ERROR, "Db Pool error: {:?}", e);
                ViewError::Err("Database error".to_string())
            }
        }
    }
}

/// Create custom errors for warp rejections.
///
/// Currently only handles 404, as there is no way of getting any
/// details out of the other build-in rejections in warp.
pub async fn for_rejection(err: Rejection) -> Result<Response, Rejection> {
    if err.is_not_found() {
        Ok(ViewError::NotFound.into_response())
    } else {
        Err(err)
    }
}
