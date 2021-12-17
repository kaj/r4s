use super::language;
use super::templates::{self, RenderRucte};
use warp::http::response::Builder;
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
    fn ise(self) -> Result<T, ViewError>;
}

impl<T, E> ViewResult<T> for Result<T, E>
where
    E: std::error::Error,
{
    fn ise(self) -> Result<T, ViewError> {
        self.map_err(|e| ViewError::Err(e.to_string()))
    }
}

impl Reply for ViewError {
    fn into_response(self) -> Response {
        match self {
            ViewError::NotFound => {
                error_response(StatusCode::NOT_FOUND, "Not found")
            }
            ViewError::BadRequest(msg) => {
                error_response(StatusCode::BAD_REQUEST, &msg)
            }
            ViewError::ServiceUnavailable => error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "The server is exhausted and can't handle your request \
                 right now. Sorry. \
                 Please try again later.",
            ),
            ViewError::Err(e) => {
                error_response(StatusCode::INTERNAL_SERVER_ERROR, &e)
            }
        }
    }
}

fn error_response(code: StatusCode, message: &str) -> Response {
    let fluent = language::load("en").unwrap();
    Builder::new()
        .status(code)
        .html(|o| templates::error(o, &fluent, code, message))
        .unwrap()
}

use deadpool_diesel::{InteractError, PoolError};

impl From<anyhow::Error> for ViewError {
    fn from(e: anyhow::Error) -> Self {
        ViewError::Err(e.to_string())
    }
}

impl From<diesel::result::Error> for ViewError {
    fn from(e: diesel::result::Error) -> Self {
        println!("Database error: {}\n    {:?}", e, e);
        ViewError::Err("Database error".to_string())
    }
}

impl From<PoolError> for ViewError {
    fn from(e: PoolError) -> Self {
        match e {
            PoolError::Timeout(kind) => {
                eprintln!("Pool timeout: {:?}", kind);
                ViewError::ServiceUnavailable
            }
            e => ViewError::Err(e.to_string()),
        }
    }
}

impl From<InteractError> for ViewError {
    fn from(e: InteractError) -> Self {
        match e {
            InteractError::Panic(panic) => {
                anyhow::anyhow!("Panic {:?}", panic).into()
            }
            InteractError::Aborted => {
                ViewError::Err("Interaction aborted".into())
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
