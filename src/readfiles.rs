use crate::dbopt::DbOpt;
use crate::imgcli::ImageInfo;
use crate::models::year_of_date;
use crate::schema::assets::dsl as a;
use crate::schema::post_tags::dsl as pt;
use crate::schema::posts::dsl as p;
use crate::schema::tags::dsl as t;
use crate::server::language;
use anyhow::{anyhow, Context, Result};
use async_recursion::async_recursion;
use chrono::{Datelike, Local};
use diesel::prelude::*;
use i18n_embed_fl::fl;
use lazy_regex::{regex_captures, regex_find, regex_replace_all};
use pulldown_cmark::escape::{escape_href, escape_html};
use pulldown_cmark::{
    BrokenLink, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag,
};
use slug::slugify;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::fs::{read, read_to_string};
use std::path::{Path, PathBuf};
use structopt::StructOpt;

type DateTime = chrono::DateTime<chrono::FixedOffset>;

#[derive(StructOpt)]
pub struct Args {
    #[structopt(flatten)]
    db: DbOpt,

    /// The paths to read content from.
    #[structopt(parse(from_os_str))]
    files: Vec<PathBuf>,

    /// Update content even if it has not changed.
    ///
    /// Mainly usefull while developing r4s itself.
    #[structopt(long)]
    force: bool,

    /// Include drafts.
    ///
    /// Posts without a publication date are drafts, and normally
    /// ignored.  With this flag, they are included.
    /// Not for use on the production server.
    #[structopt(long)]
    include_drafts: bool,
}

impl Args {
    pub async fn run(self) -> Result<()> {
        let db = self.db.get_db()?;
        for path in &self.files {
            if path.is_file() {
                self.read_file(path, &db)
                    .await
                    .with_context(|| format!("Reading file {:?}", path))?;
            } else {
                self.read_dir(path, &db)
                    .await
                    .with_context(|| format!("Reading dir {:?}", path))?;
            }
        }
        Ok(())
    }

    #[async_recursion(?Send)]
    async fn read_dir(&self, path: &Path, db: &PgConnection) -> Result<()> {
        for entry in path.read_dir()? {
            let entry = entry?;
            let path = entry.path();
            if is_dotfile(&path) {
                continue;
            }
            if entry.file_type()?.is_dir() {
                self.read_dir(&path, db).await?;
            } else if path.extension().unwrap_or_default() == "md" {
                self.read_file(&path, db)
                    .await
                    .with_context(|| format!("Reading file {:?}", path))?;
            }
        }
        Ok(())
    }

    async fn read_file(&self, path: &Path, db: &PgConnection) -> Result<()> {
        let (slug, lang) = path
            .file_stem()
            .and_then(std::ffi::OsStr::to_str)
            .ok_or_else(|| anyhow!("Bad name"))?
            .split_once('.')
            .ok_or_else(|| anyhow!("No language in file name"))?;
        let contents = read_to_string(path)?;
        let (metadata, contents_md) = extract_metadata(&contents);

        let pubdate = metadata
            .get("pubdate")
            .map(|v| v.parse::<DateTime>().context("pubdate"))
            .transpose()?;

        if pubdate.is_none() && !self.include_drafts {
            eprintln!("Skipping draft {:?}", path);
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
            .execute(db)?;
        }

        let update = metadata
            .get("update")
            .map(|v| v.parse::<UpdateInfo>().context("update"))
            .transpose()?;

        if let Some(res) = metadata.get("res") {
            for spec in res.split(',').map(|s| s.trim()) {
                handle_assets(path, spec, year, db)
                    .with_context(|| format!("Asset {:?}", spec))?;
            }
        }

        if let Some((id, old_md)) = p::posts
            .select((p::id, p::orig_md))
            .filter(year_of_date(p::posted_at).eq(&year))
            .filter(p::slug.eq(slug))
            .filter(p::lang.eq(lang))
            .first::<(i32, String)>(db)
            .optional()?
        {
            if old_md != contents || self.force {
                println!(
                    "Post #{} /{}/{}.{} exists, but should be updated.\n   {:?}",
                    id, year, slug, lang, metadata
                );
                let (mut title, teaser, body) = extract_parts(
                    year,
                    slug,
                    lang,
                    contents_md,
                    update.as_ref(),
                )
                .await?;
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
                        p::orig_md.eq(&contents),
                    ))
                    .execute(db)
                    .with_context(|| format!("Update #{}", id))?;
                if let Some(tags) = metadata.get("tags") {
                    tag_post(id, tags, db)?;
                }
            }
        } else {
            println!(
                "New post /{}/{}.{}\n   {:?}",
                year, slug, lang, metadata
            );
            let (mut title, teaser, body) =
                extract_parts(year, slug, lang, contents_md, update.as_ref())
                    .await?;
            if pubdate.is_none() {
                title.push_str(" \u{1f58b}");
            }
            let post_id = diesel::insert_into(p::posts)
                .values((
                    pubdate.map(|date| p::posted_at.eq(date)),
                    update
                        .as_ref()
                        .map(|u| &u.date)
                        .or_else(|| pubdate.as_ref())
                        .map(|date| p::updated_at.eq(date)),
                    p::slug.eq(slug),
                    p::lang.eq(lang),
                    p::title.eq(&title),
                    p::teaser.eq(&teaser),
                    p::content.eq(&body),
                    p::orig_md.eq(&contents),
                ))
                .returning(p::id)
                .get_result::<i32>(db)
                .context("Insert post")?;
            if let Some(tags) = metadata.get("tags") {
                tag_post(post_id, tags, db)?;
            }
        }
        Ok(())
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

fn handle_assets(
    path: &Path,
    spec: &str,
    year: i16,
    db: &PgConnection,
) -> Result<()> {
    let (_all, name, _, mime) =
        regex_captures!(r"^([\w_\.-]+)\s+(\{([\w-]+/[\w-]+)\})$", spec)
            .ok_or_else(|| anyhow!("Bad asset spec"))?;
    let path = path.parent().unwrap_or_else(|| Path::new(".")).join(name);
    let content = read(&path).with_context(|| path.display().to_string())?;
    if let Some((id, old_mime, old_content)) = a::assets
        .select((a::id, a::mime, a::content))
        .filter(a::year.eq(year))
        .filter(a::name.eq(name))
        .first::<(i32, String, Vec<u8>)>(db)
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
                .execute(db)
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
            .execute(db)
            .with_context(|| format!("Create asset {}/{}", year, name))?;
    }
    Ok(())
}

fn tag_post(post_id: i32, tags: &str, db: &PgConnection) -> Result<()> {
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

async fn extract_parts(
    year: i16,
    slug: &str,
    lang: &str,
    markdown: &str,
    update: Option<&UpdateInfo>,
) -> Result<(String, String, String)> {
    let (title, body) = md_to_html(markdown, lang).await?;
    // Split at "more" marker, or try to find a good place if the text is long.
    let end = markdown.find("<!-- more -->").or_else(|| {
        if markdown.len() < 900 {
            None
        } else {
            let mut end = 700;
            while !markdown.is_char_boundary(end) {
                end -= 1;
            }
            let end = markdown[..end].rfind("\n\n").unwrap_or(0);
            let end = markdown[..end].rfind("\n## ").unwrap_or_else(|| {
                if end > 50 {
                    end
                } else {
                    end + 2 + markdown[end + 2..].find("\n\n").unwrap_or(0)
                }
            });
            Some(end)
        }
    });
    let teaser = if let Some(teaser) = end.map(|e| &markdown[..e]) {
        // If teaser don't have an image and there is a "front" image ...
        let mut teaser = if let Some(img) = (!teaser.contains("\n!["))
            .then(|| ())
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
        md_to_html(&teaser, lang).await.map(|(_, teaser)| teaser)?
    } else {
        body.clone()
    };
    Ok((dbg!(title), teaser, body))
}

/// Convert my flavour of markdown to my preferred html.
///
/// Returns the title and full content html markup separately.
async fn md_to_html(markdown: &str, lang: &str) -> Result<(String, String)> {
    let mut fixlink = |broken_link: BrokenLink| {
        Some(if let Some(url) = fa_link(&broken_link.reference) {
            (url.into(), String::new().into())
        } else {
            link_ext(&broken_link, markdown, lang)
                .map(|(url, title)| (url.into(), title.into()))
                .unwrap_or_else(|| {
                    (
                        broken_link.reference.to_string().into(),
                        String::new().into(),
                    )
                })
        })
    };
    let mut items = Parser::new_with_broken_link_callback(
        markdown,
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
    let title = collect_html(title).await?;

    let body = collect_html(items.into_iter()).await?;

    Ok((title, body))
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
        r"^\[(.*)\]\[(\w+)(:(\w+))?([,\s]+(.*))?\]$",
        &source[link.span.clone()],
    )?;
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

fn wikilink(text: &str, lang: &str, xyzzy: &str) -> (String, String) {
    let t = if xyzzy.is_empty() {
        text.to_string()
    } else {
        format!("{} ({})", text, xyzzy)
    };
    (
        format!(
            "https://{}.wikipedia.org/wiki/{}",
            lang,
            t.replace(' ', "_").replace('\u{ad}', ""),
        ),
        format!("Se {} på wikipedia", t),
    )
}

async fn collect_html<'a>(
    data: impl IntoIterator<Item = Event<'a>>,
) -> Result<String> {
    let mut result = String::new();
    let mut data = data.into_iter();
    let mut section_level = 1;
    while let Some(event) = data.next() {
        match event {
            Event::Text(text) => {
                escape_html(&mut result, &text)?;
            }
            Event::Start(Tag::Heading(level, id, classes)) => {
                {
                    let level = level as u32;
                    while section_level >= level {
                        result.push_str("</section>");
                        section_level -= 1;
                    }
                    result.push('\n');
                    while section_level + 1 < level {
                        result.push_str("<section>");
                        section_level += 1;
                    }
                }
                result.push_str("<section");
                if let Some(id) = id {
                    result.push_str(" id=\"");
                    escape_html(&mut result, id)?;
                    result.push('"');
                }
                if !classes.is_empty() {
                    result.push_str(" class=\"");
                    escape_html(&mut result, &classes.join(" "))?;
                    result.push('"');
                }
                result.push('>');
                section_level += 1;
                result.push_str(&format!("<{}>", level));
            }
            Event::End(Tag::Heading(level, _, _)) => {
                if !remove_end(&mut result, &format!("<{}>", level)) {
                    result.push_str(&format!("</{}>\n", level));
                }
            }
            Event::Start(Tag::CodeBlock(blocktype)) => {
                result.push_str("<pre");
                let lang = match blocktype {
                    CodeBlockKind::Fenced(lang) if !lang.is_empty() => {
                        result.push_str(" data-lang=\"");
                        escape_html(&mut result, &lang)?;
                        result.push('"');
                        Some(lang.to_string())
                    }
                    _ => None,
                };
                result.push('>');
                for event in &mut data {
                    match event {
                        Event::End(Tag::CodeBlock(_blocktype)) => break,
                        Event::Text(code) => {
                            use crate::syntax_hl::highlight;
                            if let Some(code) = lang
                                .as_ref()
                                .and_then(|lang| highlight(lang, &code))
                            {
                                result.push_str(&code);
                            } else {
                                escape_html(&mut result, &code)?;
                            }
                        }
                        x => panic!("Unexpeted in code: {:?}", x),
                    }
                }
                result.push_str("</pre>\n");
            }
            Event::End(Tag::CodeBlock(_blocktype)) => {
                result.push_str("</code></pre>\n");
            }
            Event::Start(Tag::Image(imgtype, imgref, title)) => {
                let _ = remove_end(&mut result, "<p>")
                    || remove_end(&mut result, "<p><!--no-p-->")
                    || remove_end(&mut result, "<p><!--no-p-->\n");
                let mut inner = String::new();
                for tag in &mut data {
                    match tag {
                        Event::End(Tag::Image(..)) => break,
                        Event::Text(text) => inner.push_str(&text),
                        Event::SoftBreak => inner.push(' '),
                        _ => inner.push_str(&format!("\n{:?}", tag)),
                    }
                }
                let (_all, imgref, _, classes, caption) = regex_captures!(
                    r"^([A-Za-z0-9/._-]*)\s*(\{([^}]*)\})?\s*(.*)$"m,
                    &imgref,
                )
                .with_context(|| {
                    format!("Bad image ref: {:?}", imgref.as_ref())
                })?;
                if imgref == "cover" {
                    let url = inner.parse::<FaRef>().unwrap().cover();
                    write!(
                        &mut result,
                        "<figure class='fa-cover {}'>\
                         <a href='{url}'><img alt='Omslagsbild {}' src='{url}' width='150'/></a>\
                         <figcaption>{} {} {}</figcaption></figure>\n<p><!--no-p-->",
                        classes, inner, inner, caption, title,
                        url = url,
                    )
                        .unwrap();
                } else {
                    let imgdata = ImageInfo::fetch(imgref)
                        .await
                        .context("Image api")?;
                    if !imgdata.is_public() {
                        println!("WARNING: Image {} is not public", imgref)
                    }
                    let alt = inner.trim();
                    let imgtag = if classes
                        .split_ascii_whitespace()
                        .any(|w| w == "scaled")
                    {
                        imgdata.markup_large(alt)
                    } else {
                        imgdata.markup(alt)
                    };
                    let class2 = if imgdata.is_portrait() {
                        " portrait"
                    } else {
                        ""
                    };
                    write!(
                        &mut result,
                        "<figure class='{}{}' data-type='{:?}'>{}\
                     <figcaption>{} {}</figcaption></figure>\n<p><!--no-p-->",
                        classes, class2, imgtype, imgtag, caption, title,
                    )
                    .unwrap();
                }
            }
            Event::End(Tag::Paragraph)
                if result.ends_with("<p><!--no-p-->") =>
            {
                result.truncate(result.len() - 14);
            }
            Event::Start(Tag::TableHead) => {
                result.push_str("<thead><tr>");
            }
            Event::End(Tag::TableHead) => {
                result.push_str("</tr></thead>\n");
            }
            Event::TaskListMarker(done) => {
                result.push_str("<input disabled type='checkbox'");
                if done {
                    result.push_str(" checked=''");
                }
                result.push_str("/>\n");
            }
            Event::Start(tag) => {
                result.push('<');
                result.push_str(tag_name(&tag));
                match tag {
                    Tag::Paragraph | Tag::Emphasis => (),
                    Tag::TableCell | Tag::TableRow => (),
                    Tag::List(None) => (),
                    Tag::List(Some(start)) => {
                        result.push_str(&format!(" start='{}'", start));
                    }
                    Tag::Item => (),
                    Tag::Link(linktype, href, title) => {
                        if !href.is_empty() {
                            result.push_str(" href=\"");
                            escape_href(&mut result, &href)?;
                            result.push('"');
                        }
                        if !title.is_empty() {
                            result.push_str(" title=\"");
                            escape_html(&mut result, &title)?;
                            result.push('"');
                        }
                        result.push_str(&format!(
                            " data-type='{:?}'",
                            linktype
                        ));
                    }
                    t => result.push_str(&format!("><!-- {:?} --", t)),
                }
                // TODO: Link attributes!
                result.push('>');
            }
            Event::End(tag) => {
                result.push_str("</");
                result.push_str(tag_name(&tag));
                result.push('>');
                if matches!(
                    tag,
                    Tag::Paragraph
                        | Tag::Table(..)
                        | Tag::Item
                        | Tag::List(_)
                ) {
                    // Maybe more?
                    result.push('\n');
                }
            }
            Event::SoftBreak => result.push('\n'),
            Event::Html(code) => result.push_str(&code),
            Event::Code(code) => {
                result.push_str("<code>");
                escape_html(&mut result, &code)?;
                result.push_str("</code>");
            }
            Event::HardBreak => {
                result.push_str("<br/>\n");
            }
            e => anyhow::bail!("Unhandled: {:?}", e),
        }
    }
    for _ in 2..=section_level {
        result.push_str("</section>");
    }
    Ok(result)
}

fn remove_end(s: &mut String, tail: &str) -> bool {
    if s.ends_with(tail) {
        s.truncate(s.len() - tail.len());
        true
    } else {
        false
    }
}
fn tag_name(tag: &Tag) -> &'static str {
    match tag {
        Tag::Paragraph => "p",
        Tag::Emphasis => "em",
        Tag::Strong => "strong",
        //Tag::Image(..) => "a", // no, not really!
        Tag::Link(..) => "a",
        Tag::Table(..) => "table",
        Tag::TableRow => "tr",
        Tag::TableCell => "td",
        Tag::List(Some(_)) => "ol",
        Tag::List(None) => "ul",
        Tag::Item => "li",
        Tag::BlockQuote => "blockquote",
        tag => panic!("Not a simple tag: {:?}", tag),
    }
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
