mod codeblocks;
mod html;
mod imgcli;
mod summary;

use crate::dbopt::DbOpt;
use crate::models::year_of_date;
use crate::schema::assets::dsl as a;
use crate::schema::metapages::dsl as m;
use crate::schema::post_tags::dsl as pt;
use crate::schema::posts::dsl as p;
use crate::schema::tags::dsl as t;
use crate::server::language;
use anyhow::{anyhow, Context, Result};
use chrono::{Datelike, Local};
use diesel::prelude::*;
use i18n_embed_fl::fl;
use lazy_regex::{regex_captures, regex_find, regex_replace_all};
use pulldown_cmark::{BrokenLink, Event, HeadingLevel, Options, Parser, Tag};
use reqwest::blocking::Client;
use reqwest::header::CONTENT_TYPE;
use slug::slugify;
use std::collections::BTreeMap;
use std::fs::{read, read_to_string};
use std::path::{Path, PathBuf};
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
            if path.is_file() {
                loader
                    .read_file(path)
                    .with_context(|| format!("Reading file {:?}", path))?;
            } else {
                loader
                    .read_dir(path)
                    .with_context(|| format!("Reading dir {:?}", path))?;
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
                    .with_context(|| format!("Reading file {:?}", path))?;
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
        let (metadata, contents_md) = extract_metadata(&contents);

        if metadata.get("meta").is_some() {
            return self.read_meta_page(slug, lang, contents_md);
        }
        let pubdate = metadata
            .get("pubdate")
            .map(|v| v.parse::<DateTime>().context("pubdate"))
            .transpose()?;

        if pubdate.is_none() && !self.include_drafts {
            println!("Skipping draft {:?}", path);
            return Ok(());
        }

        let current_year: i16 = Local::now().year().try_into()?;
        let year: i16 = pubdate
            .and_then(|d| d.year().try_into().ok())
            .unwrap_or(current_year);

        if year == current_year {
            // Recent or draft
            diesel::delete(
                p::posts
                    .filter(p::slug.eq(slug))
                    .filter(p::title.like("% \u{1f58b}"))
                    .filter(p::orig_md.ne(&contents)),
            )
            .execute(&mut self.db)?;
        }

        let update = metadata
            .get("update")
            .map(|v| v.parse::<UpdateInfo>().context("update"))
            .transpose()?;

        let files = if let Some(res) = metadata.get("res") {
            res.split(',')
                .map(|s| s.trim())
                .map(|s| {
                    self.handle_asset(path, s, year)
                        .with_context(|| format!("Asset {:?}", s))
                })
                .collect::<Result<Vec<_>, _>>()?
        } else {
            Vec::new()
        };

        if let Some((id, old_md)) = p::posts
            .select((p::id, p::orig_md))
            .filter(year_of_date(p::posted_at).eq(&year))
            .filter(p::slug.eq(slug))
            .filter(p::lang.eq(lang))
            .first::<(i32, String)>(&mut self.db)
            .optional()?
        {
            if old_md != contents || self.force {
                println!(
                    "Post #{} /{}/{}.{} exists, but should be updated.",
                    id, year, slug, lang
                );
                let (
                    mut title,
                    teaser,
                    body,
                    front_image,
                    description,
                    use_leaflet,
                ) = extract_parts(
                    year,
                    slug,
                    lang,
                    contents_md,
                    update.as_ref(),
                    &files,
                    self,
                )?;
                if pubdate.is_none() {
                    title.push_str(" \u{1f58b}");
                }
                diesel::update(p::posts)
                    .filter(p::id.eq(id))
                    .set((
                        update.as_ref().map(|u| p::updated_at.eq(&u.date)),
                        p::title.eq(&title),
                        p::teaser.eq(&teaser),
                        p::content.eq(&body),
                        p::front_image.eq(front_image),
                        p::description.eq(description),
                        p::use_leaflet.eq(use_leaflet),
                        p::orig_md.eq(&contents),
                    ))
                    .execute(&mut self.db)
                    .with_context(|| format!("Update #{}", id))?;
                if let Some(tags) = metadata.get("tags") {
                    tag_post(id, tags, &mut self.db)?;
                }
            }
        } else {
            println!("New post /{}/{}.{}", year, slug, lang);
            let (
                mut title,
                teaser,
                body,
                front_image,
                description,
                use_leaflet,
            ) = extract_parts(
                year,
                slug,
                lang,
                contents_md,
                update.as_ref(),
                &files,
                self,
            )?;
            if pubdate.is_none() {
                title.push_str(" \u{1f58b}");
            }
            let post_id = diesel::insert_into(p::posts)
                .values((
                    pubdate.map(|date| p::posted_at.eq(date)),
                    update
                        .as_ref()
                        .map(|u| &u.date)
                        .or(pubdate.as_ref())
                        .map(|date| p::updated_at.eq(date)),
                    p::slug.eq(slug),
                    p::lang.eq(lang),
                    p::title.eq(&title),
                    p::teaser.eq(&teaser),
                    p::content.eq(&body),
                    p::front_image.eq(front_image),
                    p::description.eq(description),
                    p::use_leaflet.eq(use_leaflet),
                    p::orig_md.eq(&contents),
                ))
                .returning(p::id)
                .get_result::<i32>(&mut self.db)
                .context("Insert post")?;
            if let Some(tags) = metadata.get("tags") {
                tag_post(post_id, tags, &mut self.db)?;
            }
        }
        Ok(())
    }

    fn read_meta_page(
        &mut self,
        slug: &str,
        lang: &str,
        contents: &str,
    ) -> Result<()> {
        if let Some((id, old_md)) = m::metapages
            .select((m::id, m::orig_md))
            .filter(m::slug.eq(slug))
            .filter(m::lang.eq(lang))
            .first::<(i32, String)>(&mut self.db)
            .optional()?
        {
            if old_md != contents || self.force {
                let (title, body, _) =
                    Ctx::new(contents, 0, lang).md_to_html(self)?;
                diesel::update(m::metapages)
                    .set((
                        m::title.eq(&title),
                        m::content.eq(&body),
                        m::orig_md.eq(&contents),
                    ))
                    .filter(m::id.eq(id))
                    .execute(&mut self.db)
                    .context("Upadte metapage")?;
                println!("Updated metadata page /{}.{}", slug, lang);
            }
        } else {
            let (title, body, _) =
                Ctx::new(contents, 0, lang).md_to_html(self)?;
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
            println!("Created metapage /{}.{}: {}", slug, lang, title);
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
                println!("Content #{} ({}) updating", id, name);
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
                        format!("Update asset #{} {}/{}", id, year, name)
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
                .with_context(|| format!("Create asset {}/{}", year, name))?;
        }
        Ok(format!("/s/{}/{}", year, name))
    }
}

fn is_dotfile(path: &Path) -> bool {
    path.file_name()
        .and_then(std::ffi::OsStr::to_str)
        .map_or(false, |name| name.starts_with('.'))
}

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
            .with_context(|| format!("Bad date: {:?}", date))?;
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
            .filter(t::name.ilike(&tag))
            .first::<Tag>(db)
            .or_else(|_| {
                diesel::insert_into(t::tags)
                    .values((t::name.eq(&tag), t::slug.eq(&slugify(&tag))))
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

fn extract_parts(
    year: i16,
    slug: &str,
    lang: &str,
    markdown: &str,
    update: Option<&UpdateInfo>,
    files: &[(String, String)],
    loader: &mut Loader,
) -> Result<(String, String, String, Option<String>, String, bool)> {
    let (title, body, description) = Ctx::new(markdown, year, lang)
        .with_files(files)
        .md_to_html(loader)?;
    // Split at "more" marker, or try to find a good place if the text is long.
    let end = markdown.find("<!-- more -->").or_else(|| {
        let h1end = markdown.find('\n').unwrap_or(0);
        if markdown.len() < h1end + 900 {
            None
        } else {
            let mut end = h1end + 600;
            while !markdown.is_char_boundary(end) {
                end -= 1;
            }
            markdown[h1end..end].find("\n#").map(|e| h1end + e).or_else(
                || {
                    let e2 = h1end
                        + markdown[h1end..end].rfind("\n\n").unwrap_or(0);
                    if e2 > h1end + 50 {
                        Some(e2)
                    } else {
                        markdown[e2 + 2..]
                            .find("\n\n")
                            .map(|extra| e2 + 2 + extra)
                    }
                },
            )
        }
    });
    let (teaser, description) = if let Some(teaser) =
        end.map(|e| &markdown[..e])
    {
        // If teaser don't have an image and there is a "front" image ...
        let mut teaser = if let Some(img) = (!teaser.contains("\n!["))
            .then_some(())
            .and_then(|()| {
                regex_find!(
                    r#"!\[[^\]]*\]\[[^\]\{]*\{[^\}]*\bfront\b[^\}]*\}[^\]]*\]"#s,
                    markdown
                )
            })
        {
            // ... try to put the front image directly after the header.
            let s = teaser.find("\n\n").map_or(0, |s| s + 1);
            format!(
                "{}\n{}\n{}",
                &teaser[..s],
                img.replace("gallery", "sidebar"),
                &teaser[s..],
            )
        } else {
            teaser.into()
        };
        let fluent = language::load(lang).unwrap();
        if let Some(update) = update {
            if !update.info.is_empty() {
                teaser.push_str("\n\n**");
                teaser.push_str(&fl!(
                    fluent,
                    "update-at",
                    date =
                        (&crate::models::DateTime::wrap(update.date.into()))
                ));
                teaser.push_str("** ");
                teaser.push_str(&update.info);
            }
        }
        let teaser = regex_replace_all!(
            r#"\]\(\#([a-z0-0_]+)\)"#,
            &teaser,
            |_, target| format!("](/{}/{}.{}#{})", year, slug, lang, target),
        );
        Ctx::new(&teaser, year, lang)
            .md_to_html(loader)
            .map(|(_, teaser, desc)| (teaser, desc))?
    } else {
        (body.clone(), description)
    };
    let front_image = regex_captures!(
        "<figure[^>]*><(?:a href|img[^>]src)=['\"]([^'\"]+)['\"]",
        &teaser
    )
    .map(|(_, url)| url.to_string());
    let use_leaflet = body.contains("function initmap()");
    Ok((title, teaser, body, front_image, description, use_leaflet))
}

use pulldown_cmark::CowStr;
struct Ctx<'a> {
    markdown: &'a str,
    year: i16,
    lang: &'a str,
    files: &'a [(String, String)],
}
impl<'a> Ctx<'a> {
    fn new(markdown: &'a str, year: i16, lang: &'a str) -> Self {
        Ctx {
            markdown,
            year,
            lang,
            files: &[],
        }
    }
    fn with_files(mut self, files: &'a [(String, String)]) -> Self {
        self.files = files;
        self
    }
    fn fixlink(&self, link: BrokenLink) -> Option<(CowStr, CowStr)> {
        let reff = link.reference.trim_matches('`');
        self.files
            .iter()
            .find(|f| f.0 == reff)
            .map(|(_name, url)| (url.clone().into(), "".into()))
            .or_else(|| fa_link(reff).map(|url| (url.into(), "".into())))
            .or_else(|| {
                link_ext(&link, self.markdown, self.lang)
                    .map(|(url, title)| (url.into(), title.into()))
            })
            .or_else(|| Some((link.reference.to_string().into(), "".into())))
    }

    /// Convert my flavour of markdown to my preferred html.
    ///
    /// Returns the title and full content html markup separately.
    fn md_to_html(
        &self,
        loader: &mut Loader,
    ) -> Result<(String, String, String)> {
        let mut fixlink = |broken_link: BrokenLink| self.fixlink(broken_link);
        let mut items = Parser::new_with_broken_link_callback(
            self.markdown,
            Options::all(),
            Some(&mut fixlink),
        )
        .collect::<Vec<_>>();

        let prefix = items_until(
            &mut items,
            &Event::Start(Tag::Heading(HeadingLevel::H1, None, vec![])),
        )
        .ok_or_else(|| anyhow!("No start of h1"))?;
        anyhow::ensure!(prefix.is_empty(), "Unexpected prefix: {:?}", prefix);

        let title = items_until(
            &mut items,
            &Event::End(Tag::Heading(HeadingLevel::H1, None, vec![])),
        )
        .ok_or_else(|| anyhow!("No end of h1"))?;

        let title = html::collect(title, loader, self.year, self.lang)?;
        let summary = summary::collect(items.clone().into_iter())?;
        let body =
            html::collect(items.into_iter(), loader, self.year, self.lang)?;
        Ok((title, body, summary))
    }
}

fn items_until<'a>(
    all: &mut Vec<Event<'a>>,
    delimiter: &Event,
) -> Option<Vec<Event<'a>>> {
    let pos = all.iter().position(|e| e == delimiter)?;
    let mut prefix = all.split_off(pos + 1);
    std::mem::swap(all, &mut prefix);
    prefix.pop(); // get rid of the delimiter itself
    Some(prefix)
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

fn link_ext(
    link: &BrokenLink,
    source: &str,
    lang: &str,
) -> Option<(String, String)> {
    let (_all, text, kind, _, attr_0, _, attrs) = regex_captures!(
        r"^\[(.*)\]\[(\w+)(:(\w+))?([,\s]+(.*))?\]$"s,
        &source[link.span.clone()],
    )?;
    let text = &regex_replace_all!(r"\s+", text, |_| " ");
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
            format!("Se {} på seriewikin", text),
        )),
        "cargo" => {
            Some((format!("https://lib.rs/crates/{}", text), String::new()))
        }
        "foldoc" => Some((
            format!("https://foldoc.org/{}", text),
            format!("Se {} i free online dictionary of computing", text),
        )),
        "rfc" => Some((
            format!("http://www.faqs.org/rfcs/rfc{}.html", attr_0),
            format!("RFC {}", attr_0),
        )),
        _ => None,
    }
}

fn wikilink(text: &str, lang: &str, disambig: &str) -> (String, String) {
    let t = if disambig.is_empty() {
        text.to_string()
    } else {
        format!("{} ({})", text, disambig)
    };
    (
        format!(
            "https://{}.wikipedia.org/wiki/{}",
            lang,
            t.replace(' ', "_").replace('\u{ad}', ""),
        ),
        // TODO: Translate this to page (not link) language!
        format!("Se {} på wikipedia", t),
    )
}

fn extract_metadata(src: &str) -> (BTreeMap<&str, &str>, &str) {
    let mut meta = BTreeMap::new();
    let mut src = src;
    while let Some((k, v, reminder)) =
        src.split_once('\n').and_then(|(line, reminder)| {
            line.split_once(':').map(|(k, v)| (k, v, reminder))
        })
    {
        meta.insert(k.trim(), v.trim());
        src = reminder;
    }
    (meta, src.trim())
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
