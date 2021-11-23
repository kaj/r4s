use super::language;
use super::templates::{self, RenderRucte};
use warp::http::response::Builder;
use warp::http::status::StatusCode;
use warp::reply::Response;
use warp::{self, Reply};

#[derive(Debug)]
pub enum ViewError {
    NotFound,
    BadRequest(String),
    Err(String),
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
        ViewError::Err(e.to_string())
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
