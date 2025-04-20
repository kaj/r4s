mod codeblocks;
mod html;
mod imgcli;
mod markdown;
mod summary;

use self::markdown::{Body, ContentParser, Ctx};
use crate::dbopt::DbOpt;
use crate::models::year_of_date;
use crate::schema::assets::dsl as a;
use crate::schema::metapages::dsl as m;
use crate::schema::post_tags::dsl as pt;
use crate::schema::posts::dsl as p;
use crate::schema::tags::dsl as t;
use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use diesel::prelude::*;
use lazy_regex::regex_captures;
use reqwest::blocking::Client;
use reqwest::header::CONTENT_TYPE;
use slug::slugify;
use std::fmt;
use std::fs::{read, read_to_string};
use std::path::{Path, PathBuf};
use tracing::{debug, info, trace, warn};
use warp::hyper::body::Bytes;

type DateTime = chrono::DateTime<chrono::FixedOffset>;

#[derive(clap::Parser)]
pub struct Args {
    #[clap(flatten)]
    db: DbOpt,

    #[clap(flatten)]
    img: ImgClientOpt,

    /// The paths to read content from.
    #[clap(value_parser)]
    files: Vec<PathBuf>,

    /// Update content even if it has not changed.
    ///
    /// Mainly usefull while developing r4s itself.
    #[clap(long)]
    force: bool,

    /// Include drafts.
    ///
    /// Posts without a publication date are drafts, and normally
    /// ignored.  With this flag, they are included.
    /// Not for use on the production server.
    #[clap(long)]
    include_drafts: bool,
}

impl Args {
    pub fn run(self) -> Result<()> {
        let web = Client::builder()
            .user_agent("r4s https://github.com/kaj/r4s")
            .build()?;
        let mut loader = Loader {
            include_drafts: self.include_drafts,
            force: self.force,
            db: self.db.get_db()?,
            imgcli: self.img.client(web.clone()),
            web,
        };
        for path in &self.files {
            debug!("Searching path {path:?}");
            if path.is_file() {
                loader
                    .read_file(path)
                    .with_context(|| format!("Reading file {path:?}"))?;
            } else {
                loader
                    .read_dir(path)
                    .with_context(|| format!("Reading dir {path:?}"))?;
            }
        }
        Ok(())
    }
}

struct Loader {
    include_drafts: bool,
    force: bool,
    db: PgConnection,
    web: Client,
    imgcli: ImgClient,
}
impl Loader {
    fn read_dir(&mut self, path: &Path) -> Result<()> {
        for entry in path.read_dir()? {
            let entry = entry?;
            let path = entry.path();
            if is_dotfile(&path) {
                continue;
            }
            if entry.file_type()?.is_dir() {
                self.read_dir(&path)?;
            } else if path.extension().unwrap_or_default() == "md" {
                self.read_file(&path)
                    .with_context(|| format!("Reading file {path:?}"))?;
            }
        }
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    fn read_file(&mut self, path: &Path) -> Result<()> {
        let (slug, lang) = path
            .file_stem()
            .and_then(std::ffi::OsStr::to_str)
            .context("Bad file name")?
            .split_once('.')
            .context("No language in file name")?;
        let contents = read_to_string(path)?;

        let ctx = Ctx::new(&contents, slug, lang);
        let post_src = ctx.parser()?;

        if post_src.meta().is_meta {
            return self
                .read_meta_page(post_src, path, slug, lang, &contents);
        } else if post_src.meta().pubdate.is_none() && !self.include_drafts {
            debug!("Skipping draft {:?}", path);
            return Ok(());
        }

        let pubdate = post_src.meta().pubdate;
        let update = post_src.meta().update.as_ref().map(|u| u.date);

        if update
            .or(pubdate)
            .is_none_or(|d| (Utc::now() - d.to_utc()).num_days() < 200)
        {
            diesel::delete(
                p::posts
                    .filter(p::slug.eq(slug))
                    .filter(p::lang.eq(lang))
                    .filter(p::title.like("% \u{1f58b}"))
                    .filter(p::orig_md.ne(&contents)),
            )
            .execute(&mut self.db)?;
        }

        if let Some((id, old_md)) = p::posts
            .select((p::id, p::orig_md))
            .filter(year_of_date(p::posted_at).eq(&post_src.year))
            .filter(p::slug.eq(slug))
            .filter(p::lang.eq(lang))
            .first::<(i32, String)>(&mut self.db)
            .optional()?
        {
            if has_changed(&old_md, &contents) || self.force {
                info!(
                    "Post #{} {} exists, but should be updated.",
                    id,
                    post_src.get_url(),
                );
                post_src.load_assets(path, self)?;
                let tags = post_src.meta().tags.clone();
                let post = Body::load(post_src, self)?;

                diesel::update(p::posts)
                    .filter(p::id.eq(id))
                    .set((
                        update.map(|u| p::updated_at.eq(u)),
                        p::title.eq(&post.title),
                        p::teaser.eq(&post.teaser),
                        p::content.eq(&post.body),
                        p::front_image.eq(post.front_image),
                        p::description.eq(post.summary),
                        p::use_leaflet.eq(post.use_leaflet),
                        p::orig_md.eq(&contents),
                    ))
                    .execute(&mut self.db)
                    .with_context(|| format!("Update #{id}"))?;

                if let Some(tags) = &tags {
                    tag_post(id, tags, &mut self.db)?;
                }
            } else {
                trace!("No change in #{id} {}", post_src.get_url());
            }
        } else {
            info!("New post {}", post_src.get_url());

            post_src.load_assets(path, self)?;
            let tags = post_src.meta().tags.clone();
            let post = Body::load(post_src, self)?;

            let post_id = diesel::insert_into(p::posts)
                .values((
                    pubdate.map(|date| p::posted_at.eq(date)),
                    update
                        .as_ref()
                        .or(pubdate.as_ref())
                        .map(|date| p::updated_at.eq(date)),
                    p::slug.eq(slug),
                    p::lang.eq(lang),
                    p::title.eq(&post.title),
                    p::teaser.eq(&post.teaser),
                    p::content.eq(&post.body),
                    p::front_image.eq(&post.front_image),
                    p::description.eq(&post.summary),
                    p::use_leaflet.eq(post.use_leaflet),
                    p::orig_md.eq(&contents),
                ))
                .returning(p::id)
                .get_result::<i32>(&mut self.db)
                .context("Insert post")?;
            if let Some(tags) = &tags {
                tag_post(post_id, tags, &mut self.db)?;
            }
        }
        Ok(())
    }

    fn read_meta_page(
        &mut self,
        mut src: ContentParser,
        path: &Path,
        slug: &str,
        lang: &str,
        contents: &str,
    ) -> Result<()> {
        if let Some(tags) = &src.meta().tags {
            bail!("Meta pages should not have tags, got {tags:?}");
        }
        if let Some((id, old_md)) = m::metapages
            .select((m::id, m::orig_md))
            .filter(m::slug.eq(slug))
            .filter(m::lang.eq(lang))
            .first::<(i32, String)>(&mut self.db)
            .optional()?
        {
            if has_changed(&old_md, contents) || self.force {
                src.load_assets(path, self)?;
                let title = src.load_title(self)?;
                let body = src.into_html(self)?;

                diesel::update(m::metapages)
                    .set((
                        m::title.eq(&title),
                        m::content.eq(&body),
                        m::orig_md.eq(contents),
                    ))
                    .filter(m::id.eq(id))
                    .execute(&mut self.db)
                    .context("Upadte metapage")?;
                info!("Updated metadata page /{}.{}", slug, lang);
            }
        } else {
            src.load_assets(path, self)?;
            let title = src.load_title(self)?;
            let body = src.into_html(self)?;

            diesel::insert_into(m::metapages)
                .values((
                    m::slug.eq(slug),
                    m::lang.eq(lang),
                    m::title.eq(&title),
                    m::content.eq(&body),
                    m::orig_md.eq(&contents),
                ))
                .execute(&mut self.db)
                .context("Insert metapage")?;
            info!("Created metapage /{}.{}: {}", slug, lang, title);
        }
        Ok(())
    }

    fn handle_asset(
        &mut self,
        path: &Path,
        spec: &str,
        year: i16,
    ) -> Result<(String, String)> {
        let (_all, name, _, mime) =
            regex_captures!(r"^([\w_\.-]+)\s+(\{([\w-]+/[\w-]+)\})$", spec)
                .context("Bad asset spec")?;
        let path = path.parent().unwrap_or_else(|| Path::new(".")).join(name);
        let content =
            read(&path).with_context(|| path.display().to_string())?;
        let url = self.store_asset(year, name, mime, &content)?;
        Ok((name.into(), url))
    }

    fn fetch_content(&self, url: &str) -> Result<(String, Bytes)> {
        let resp = self.web.get(url).send()?.error_for_status()?;
        let ctype = resp
            .headers()
            .get(CONTENT_TYPE)
            .context("content-type")?
            .to_str()?;
        Ok((ctype.into(), resp.bytes()?))
    }

    fn store_asset(
        &mut self,
        year: i16,
        name: &str,
        mime: &str,
        content: &[u8],
    ) -> Result<String> {
        if let Some((id, old_mime, old_content)) = a::assets
            .select((a::id, a::mime, a::content))
            .filter(a::year.eq(year))
            .filter(a::name.eq(name))
            .first::<(i32, String, Vec<u8>)>(&mut self.db)
            .optional()?
        {
            if mime != old_mime || content != old_content {
                println!("Content #{id} ({name}) updating");
                diesel::update(a::assets)
                    .filter(a::id.eq(id))
                    .set((
                        a::year.eq(year),
                        a::name.eq(name),
                        a::mime.eq(mime),
                        a::content.eq(&content),
                    ))
                    .execute(&mut self.db)
                    .with_context(|| {
                        format!("Update asset #{id} {year}/{name}")
                    })?;
            }
        } else {
            diesel::insert_into(a::assets)
                .values((
                    a::year.eq(year),
                    a::name.eq(name),
                    a::mime.eq(mime),
                    a::content.eq(content),
                ))
                .execute(&mut self.db)
                .with_context(|| format!("Create asset {year}/{name}"))?;
        }
        Ok(format!("/s/{year}/{name}"))
    }
}

fn is_dotfile(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|n| n.as_encoded_bytes().starts_with(b"."))
}

#[derive(Clone, Debug)]
struct UpdateInfo {
    date: DateTime,
    info: String,
}

impl FromStr for UpdateInfo {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        let (date, info) = s.split_once(' ').unwrap_or((s, ""));
        let date = date
            .trim()
            .parse()
            .with_context(|| format!("Bad date: {date:?}"))?;
        let info = info.trim().to_string();
        Ok(UpdateInfo { date, info })
    }
}

fn tag_post(post_id: i32, tags: &str, db: &mut PgConnection) -> Result<()> {
    use crate::models::Tag;
    diesel::delete(pt::post_tags)
        .filter(pt::post_id.eq(post_id))
        .execute(db)
        .context("delete old tags")?;
    for tag in tags.split(',') {
        let tag = tag.trim();
        let tag = t::tags
            .filter(t::name.ilike(tag))
            .first::<Tag>(db)
            .or_else(|_| {
                diesel::insert_into(t::tags)
                    .values((t::name.eq(tag), t::slug.eq(&slugify(tag))))
                    .get_result::<Tag>(db)
            })
            .context("Find or create tag")?;
        diesel::insert_into(pt::post_tags)
            .values((pt::post_id.eq(post_id), pt::tag_id.eq(tag.id)))
            .execute(db)
            .context("tag post")?;
    }
    Ok(())
}

struct FaRef {
    issue: i8,
    year: i16,
}

use std::str::FromStr;
impl FromStr for FaRef {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        regex_captures!(
            r"\b[Ff]a (?P<ii>(?P<i>[1-9]\d?)(-[1-9]\d?)?)[ /](?P<y>(19|20)\d{2})\b",
            s,
        )
            .map(|(_, _, i, _, y, _)| FaRef {
                issue: i.parse().unwrap(),
                year: y.parse().unwrap()
            })
            .ok_or(())
    }
}
impl FaRef {
    fn url(&self) -> String {
        format!(
            "https://fantomenindex.krats.se/{}/{}",
            self.year, self.issue,
        )
    }
    fn cover(&self) -> String {
        format!(
            "https://fantomenindex.krats.se/c/f{}-{}.jpg",
            self.year, self.issue,
        )
    }
}

/// Check if `s` is a phantom issue reference.
///
/// Strings like `"Fa 1/1950"` or `"Fa 2-3 2019"` gets an index url.
fn fa_link(s: &str) -> Option<String> {
    FaRef::from_str(s).ok().map(|fa| fa.url())
}
#[test]
fn fa_link_a() {
    assert_eq!(fa_link("Hello"), None)
}
#[test]
fn fa_link_b() {
    assert_eq!(
        fa_link("Fa 17/1984").as_deref(),
        Some("https://fantomenindex.krats.se/1984/17")
    )
}
#[test]
fn fa_link_c() {
    assert_eq!(
        fa_link("Fa 1-2/2021").as_deref(),
        Some("https://fantomenindex.krats.se/2021/1")
    )
}

fn link_data(
    kind: &str,
    text: &str,
    attr_0: &str,
    attrs: &str,
    lang: &str,
) -> Option<(String, String)> {
    match kind {
        "personname" | "wp" => {
            let lang = if attr_0.is_empty() { lang } else { attr_0 };
            Some(wikilink(text, lang, attrs))
        }
        "sw" => Some((
            format!(
                "https://seriewikin.serieframjandet.se/index.php/{}",
                text.replace(' ', "_")
            ),
            format!("Se {text} på seriewikin"),
        )),
        "cargo" => {
            Some((format!("https://lib.rs/crates/{text}"), String::new()))
        }
        "foldoc" => Some((
            format!("https://foldoc.org/{text}"),
            format!("Se {text} i free online dictionary of computing"),
        )),
        "rfc" => Some((
            format!("http://www.faqs.org/rfcs/rfc{attr_0}.html"),
            format!("RFC {attr_0}"),
        )),
        _ => None,
    }
}

fn wikilink(text: &str, lang: &str, disambig: &str) -> (String, String) {
    let t = if disambig.is_empty() {
        text.to_string()
    } else {
        format!("{text} ({disambig})")
    };
    (
        format!(
            "https://{}.wikipedia.org/wiki/{}",
            lang,
            t.replace(' ', "_").replace('\u{ad}', ""),
        ),
        // TODO: Translate this to page (not link) language!
        format!("Se {t} på wikipedia"),
    )
}

#[derive(Clone, clap::Parser)]
struct ImgClientOpt {
    /// Base url for rphotos image api client.
    #[clap(long = "image-base", env = "IMG_URL")]
    base: String,
    /// User for rphotos api.
    #[clap(long = "image-user", env = "IMG_USER")]
    user: String,
    /// Password for rphotos api.
    #[clap(
        long = "image-password",
        env = "IMG_PASSWORD",
        hide_env_values = true
    )]
    password: String,
    /// Make referenced images public (otherwise a warning is issued
    /// when private images are referenced).
    #[clap(long)]
    make_images_public: bool,
}
impl ImgClientOpt {
    fn client(&self, web: Client) -> ImgClient {
        ImgClient {
            options: self.clone(),
            web,
            client: None,
        }
    }
}

pub struct ImgClient {
    options: ImgClientOpt,
    web: Client,
    client: Option<self::imgcli::ImgClient>,
}
impl ImgClient {
    fn fetch(&mut self, imgref: &str) -> Result<self::imgcli::ImageInfo> {
        if self.client.is_none() {
            self.client = Some(self::imgcli::ImgClient::login(
                self.web.clone(),
                &self.options.base,
                &self.options.user,
                &self.options.password,
            )?);
        }
        let cli = self.client.as_ref().unwrap();
        if self.options.make_images_public {
            cli.make_image_public(imgref).map_err(|e| {
                anyhow!("Failed to make image {:?} public: {}", imgref, e)
            })
        } else {
            cli.fetch_image(imgref).map_err(|e| {
                anyhow!("Failed to fetch image {:?}: {}", imgref, e)
            })
        }
    }
}

fn has_changed(old: &str, new: &str) -> bool {
    fn nometa(t: &str) -> &str {
        if let Some(i) = t.find("\n\n# ") {
            &t[i + 2..]
        } else {
            t
        }
    }
    nometa(old) != nometa(new)
}

struct PageRef {
    year: i16,
    slug: String,
    lang: String,
}

impl fmt::Display for PageRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.year != 0 {
            write!(f, "/{}/{}.{}", self.year, self.slug, self.lang)
        } else {
            write!(f, "/{}.{}", self.slug, self.lang)
        }
    }
}
