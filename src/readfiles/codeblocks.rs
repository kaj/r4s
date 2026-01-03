use super::{Loader, PageRef};
use anyhow::{bail, Result};
use arborium::{Error as ArbError, Highlighter};
use base64::prelude::*;
use i18n_embed_fl::fl;
use pulldown_cmark_escape::escape_html;
use qr_code::QrCode;
use serde::Deserialize;
use std::{fmt::Write, sync::LazyLock};

/// Write html to `out` for some `code` fenced as `lang`.
///
/// If `lang` starts with a `"!"`, the code is used rather than highlighted.
/// The `loader` and self `url` is needed for `!embed` that needs to store a
/// related file.
pub fn handle(
    out: &mut String,
    code: &str,
    lang: Option<&str>,
    loader: &mut Loader,
    url: &PageRef,
) -> Result<()> {
    if let Some(lang) = lang {
        if let Some(bang) = lang.strip_prefix('!') {
            match bang.split_once(' ').unwrap_or((bang, "")) {
                ("leaflet", "") => leaflet(out, code),
                ("qr", caption) => qr(out, caption, code),
                ("embed", "") => embed(out, loader, url, code),
                _ => bail!("Magic for {lang:?} not implemented"),
            }
        } else {
            highlight(out, lang, code)
        }
    } else {
        out.push_str("<pre>");
        escape_html(&mut *out, code)?;
        out.push_str("</pre>\n");
        Ok(())
    }
}

pub fn highlight(out: &mut String, lang: &str, code: &str) -> Result<()> {
    out.push_str("<pre data-lang=\"");
    escape_html(&mut *out, lang)?;
    out.push_str("\"><code>");

    match HL.clone().highlight(lang, code) {
        Err(ArbError::UnsupportedLanguage { language }) => {
            tracing::warn!(%language, "Unsupported language");
            escape_html(&mut *out, code)?;
        }
        html => out.push_str(&html?),
    }
    out.push_str("</code></pre>\n");
    Ok(())
}

static HL: LazyLock<Highlighter> = LazyLock::new(Highlighter::new);

fn leaflet(out: &mut String, content: &str) -> Result<()> {
    out.push_str(
        "<div id='llmap'>\
         <p>There should be a map here.</p>\
         </div>\n\
         <script type='text/javascript'>\n\
         function initmap() {\
         var map = L.map('llmap',{scrollWheelZoom:false})\
         .addLayer(L.tileLayer('//{s}.tile.openstreetmap.org/{z}/{x}/{y}.png',\
         {attribution:'© <a href=\"http://osm.org/copyright\">\
         OpenStreetMaps bidragsgivare</a>'}));\n"
    );
    out.push_str(content);
    out.push_str("}\n</script>\n");
    Ok(())
}

fn qr(out: &mut String, caption: &str, content: &str) -> Result<()> {
    let qr = QrCode::new(content)?;
    let width = qr.width();
    let mut imgdata = Vec::new();
    let mut img = png::Encoder::new(&mut imgdata, width as _, width as _);
    img.set_color(png::ColorType::Grayscale);
    img.set_depth(png::BitDepth::One);
    img.set_compression(png::Compression::High);
    let mut writer = img.write_header()?;
    writer.write_image_data(
        &qr.to_vec()
            .chunks(width)
            .flat_map(|line| {
                line.chunks(8).map(|byte| {
                    byte.iter().fold(0, |acc, elem| 2 * acc + u8::from(!elem))
                        * (1 << (8 - byte.len()))
                })
            })
            .collect::<Vec<_>>(),
    )?;
    writer.finish()?;

    let url = format!(
        "data:image/png;base64,{}",
        BASE64_STANDARD_NO_PAD.encode(imgdata)
    );
    writeln!(
        out,
        "<figure class='qr-code sidebar'>\
             <img alt='' src='{url}' width='{width}' height='{width}'/>"
    )?;
    if !caption.is_empty() {
        out.push_str("<figcaption>");
        escape_html(&mut *out, caption)?;
        out.push_str("</figcaption>");
    }
    out.push_str("</figure>");
    Ok(())
}

fn embed(
    out: &mut String,
    loader: &mut Loader,
    url: &PageRef,
    code: &str,
) -> Result<()> {
    let data = code.trim();
    if let Some(ytid) = data.strip_prefix("https://youtu.be/") {
        let id = format!("yt-{ytid}");
        let embed: EmbedData = loader
            .web
            .get("https://www.youtube.com/oembed")
            .query(&[("url", data), ("format", "json")])
            .send()?
            .error_for_status()?
            .json()?;
        let img = embed.thumbnail_url;
        let (ctype, img) = loader
            .fetch_content(&img.replace("hqdefault.jpg", "maxresdefault.jpg"))
            .or_else(|_| loader.fetch_content(&img))?;
        let img = loader.store_asset(
            url.year,
            &format!("{id}.jpg"),
            &ctype,
            &img,
        )?;
        let notice = fl!(url.lang.fluent(), "consent-youtube");
        writeln!(
            out,
            "<figure id='{id}' class='wrapiframe' \
                 style='padding-bottom: {aspect}%'>\
                 \n  <figcaption>{title}</figcaption>\
                 \n  <img class='ifrprev' src='{img}' \
                 width='{width}' height='{height}'>\
                 \n  <div class='ifrprev'><button \
                 onclick='document.getElementById(\"{id}\")\
                 .innerHTML=\"{iframe}\"'>⏵ Play</button>\
                 \n  <p>{notice}</p></div>\
                 </figure>",
            title = embed.title,
            aspect = 100. * f64::from(embed.height) / f64::from(embed.width),
            height = embed.height,
            width = embed.width,
            iframe = embed
                .html
                .replace("?feature=oembed", "?autoplay=1")
                .replace('\'', "\\\'")
                .replace('"', "\\\""),
        )?;
    } else {
        bail!("Unknown embed: {data:?}");
    }
    Ok(())
}

/// The interesting parts of an oembed response.
#[derive(Debug, Deserialize)]
struct EmbedData {
    title: String,
    height: u32,
    width: u32,
    thumbnail_url: String,
    html: String,
}
