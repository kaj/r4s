use pulldown_cmark::{html::push_html, Event, HeadingLevel, Parser, Tag};

pub fn safe_md2html(raw: &str) -> String {
    let below_level = lh(HeadingLevel::H3);
    let mut hdiff = 0;
    let markdown = Parser::new(raw).map(|e| match e {
        Event::Html(s) => Event::Text(s),
        Event::Start(Tag::Heading(h, id, cls)) => {
            let level = lh(h);
            hdiff = std::cmp::max(hdiff, below_level - level);
            Event::Start(Tag::Heading(hl(level + hdiff), id, cls))
        }
        Event::End(Tag::Heading(h, id, cls)) => {
            Event::End(Tag::Heading(hl(lh(h) + hdiff), id, cls))
        }
        e => e,
    });
    let mut html = String::new();
    push_html(&mut html, markdown);
    html
}

fn lh(h: HeadingLevel) -> i8 {
    match h {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}
pub fn hl(i: i8) -> HeadingLevel {
    match i {
        i if i <= 1 => HeadingLevel::H1,
        2 => HeadingLevel::H2,
        3 => HeadingLevel::H3,
        4 => HeadingLevel::H4,
        5 => HeadingLevel::H5,
        _ => HeadingLevel::H6,
    }
}

#[test]
fn markdown_no_html() {
    assert_eq!(
        safe_md2html(
            "Hej!\
             \r\n\r\nHär är <em>en</em> _kommentar_.\
             \r\n\r\n<script>evil</script>"
        ),
        "<p>Hej!</p>\
         \n<p>Här är &lt;em&gt;en&lt;/em&gt; <em>kommentar</em>.</p>\
         \n&lt;script&gt;evil&lt;/script&gt;",
    );
}

#[test]
fn heading_level() {
    assert_eq!(
        safe_md2html(
            "# Rubrik\
             \r\n\r\nRubriken ska hamna på rätt nivå.\
             \r\n\r\n## Underrubrik\
             \r\n\r\nOch underrubriken på nivån under."
        ),
        "<h3>Rubrik</h3>\
         \n<p>Rubriken ska hamna på rätt nivå.</p>\
         \n<h4>Underrubrik</h4>\
         \n<p>Och underrubriken på nivån under.</p>\n",
    );
}
#[test]
fn heading_level_2() {
    assert_eq!(
        safe_md2html(
            "### Rubrik\
             \r\n\r\nRubriken ska hamna på rätt nivå.\
             \r\n\r\n#### Underrubrik\
             \r\n\r\nOch underrubriken på nivån under."
        ),
        "<h3>Rubrik</h3>\
         \n<p>Rubriken ska hamna på rätt nivå.</p>\
         \n<h4>Underrubrik</h4>\
         \n<p>Och underrubriken på nivån under.</p>\n",
    );
}
