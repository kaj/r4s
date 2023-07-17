use super::{goh, response, wrap, App, Result, ViewError, ViewResult};
use crate::schema::assets::dsl as a;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use tracing::instrument;
use warp::filters::BoxedFilter;
use warp::path::{end, param, tail, Tail};
use warp::reply::Response;
use warp::{self, Filter, Reply};

/// Handle static and dynamic assets.
///
/// Static assets are stylesheets, images and some javascript that is
/// compiled into the server.
/// Dynamic assets are things that belongs to specific posts, and are
/// loaded from the database.
pub fn routes(s: BoxedFilter<(App,)>) -> BoxedFilter<(impl Reply,)> {
    warp::any()
        .and(param())
        .and(param())
        .and(end())
        .and(goh())
        .and(s)
        .then(asset_file)
        .or(tail().and(goh()).map(static_file))
        .unify()
        .map(wrap)
        .boxed()
}

#[instrument]
async fn asset_file(year: i16, name: String, app: App) -> Result<Response> {
    use chrono::{Duration, Utc};
    use warp::http::header::{CONTENT_TYPE, EXPIRES};
    let mut db = app.db().await?;
    let far_expires = Utc::now() + Duration::days(180);

    let (mime, content) = a::assets
        .select((a::mime, a::content))
        .filter(a::year.eq(year))
        .filter(a::name.eq(name))
        .first::<(String, Vec<u8>)>(&mut db)
        .await
        .optional()?
        .ok_or(ViewError::NotFound)?;

    response()
        .header(CONTENT_TYPE, mime)
        .header(EXPIRES, far_expires.to_rfc2822())
        .body(content.into())
        .or_ise()
}

/// Handler for static files.
/// Create a response from the file data with a correct content type
/// and a far expires header (or a 404 if the file does not exist).
#[instrument]
fn static_file(name: Tail) -> Result<Response> {
    use super::templates::statics::StaticFile;
    use chrono::{Duration, Utc};
    use warp::http::header::{CONTENT_TYPE, EXPIRES};
    let data = StaticFile::get(name.as_str()).ok_or(ViewError::NotFound)?;
    let far_expires = Utc::now() + Duration::days(180);
    response()
        .header(CONTENT_TYPE, data.mime.as_ref())
        .header(EXPIRES, far_expires.to_rfc2822())
        .body(data.content.into())
        .or_ise()
}
