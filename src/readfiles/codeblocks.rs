use crate::syntax_hl::ClassedHTMLGenerator;
use crate::syntax_hl::LinesWithEndings;
use anyhow::{bail, Result};
use pulldown_cmark::escape::escape_html;

pub trait BlockHandler {
    fn push(&mut self, content: &str) -> Result<()>;
    fn end(self);
}

pub enum DynBlock<'a> {
    Leaflet(LeafletHandler<'a>),
    Code(CodeBlock<'a>),
}
impl<'a> DynBlock<'a> {
    pub fn for_kind(
        out: &'a mut String,
        lang: Option<&'a str>,
    ) -> Result<DynBlock<'a>> {
        match lang.and_then(|l| l.strip_prefix('!')) {
            Some("leaflet") => {
                Ok(DynBlock::Leaflet(LeafletHandler::open(out)))
            }
            Some(bang) => {
                bail!("Magic for !{:?} not implemented", bang);
            }
            None => Ok(DynBlock::Code(CodeBlock::open(out, lang)?)),
        }
    }
}

impl<'a> BlockHandler for DynBlock<'a> {
    fn push(&mut self, content: &str) -> Result<()> {
        match self {
            DynBlock::Leaflet(x) => x.push(content),
            DynBlock::Code(x) => x.push(content),
        }
    }
    fn end(self) {
        match self {
            DynBlock::Leaflet(x) => x.end(),
            DynBlock::Code(x) => x.end(),
        }
    }
}

pub struct LeafletHandler<'a> {
    out: &'a mut String,
}

impl<'a> LeafletHandler<'a> {
    fn open(out: &mut String) -> LeafletHandler {
        out.push_str(r#"
<div id="llmap">
<p>There should be a map here.</p>
</div>
<script type="text/javascript">
  function initmap() {
  var map = L.map('llmap', {scrollWheelZoom: false})
  .addLayer(L.tileLayer('//{s}.tile.openstreetmap.org/{z}/{x}/{y}.png', {
  attribution: '&#xA9; <a href="http://osm.org/copyright">OpenStreetMaps bidragsgivare</a>',
  }));
"#);
        LeafletHandler { out }
    }
}
impl<'a> BlockHandler for LeafletHandler<'a> {
    fn push(&mut self, content: &str) -> Result<()> {
        self.out.push_str(content);
        Ok(())
    }

    fn end(self) {
        self.out.push_str("}\n</script>\n");
    }
}

pub struct CodeBlock<'a> {
    out: &'a mut String,
    gen: Option<ClassedHTMLGenerator<'a>>,
}
impl<'a> CodeBlock<'a> {
    fn open(
        out: &'a mut String,
        lang: Option<&'a str>,
    ) -> Result<CodeBlock<'a>> {
        out.push_str("<pre");
        if let Some(lang) = lang {
            out.push_str(" data-lang=\"");
            escape_html(&mut *out, lang)?;
            out.push('"');
        }
        out.push('>');
        Ok(CodeBlock {
            out,
            gen: lang.and_then(crate::syntax_hl::for_lang),
        })
    }
}
impl<'a> BlockHandler for CodeBlock<'a> {
    fn push(&mut self, content: &str) -> Result<()> {
        if let Some(gen) = &mut self.gen {
            for line in LinesWithEndings::from(content) {
                gen.parse_html_for_line_which_includes_newline(line);
            }
        } else {
            escape_html(&mut *self.out, content)?;
        }
        Ok(())
    }
    fn end(self) {
        if let Some(gen) = self.gen {
            self.out.push_str(&gen.finalize())
        }
        self.out.push_str("</pre>\n");
    }
}
