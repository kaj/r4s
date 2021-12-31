//! This module is used both by the builder and by the program!

use lazy_static::lazy_static;
use syntect::highlighting::ThemeSet;
use syntect::html::css_for_theme_with_class_style;
use syntect::html::ClassStyle;
pub use syntect::html::ClassedHTMLGenerator;
use syntect::parsing::SyntaxSet;
pub use syntect::util::LinesWithEndings;

const STYLE: ClassStyle = ClassStyle::SpacedPrefixed { prefix: "syh" };
lazy_static! {
    static ref SYNSET: SyntaxSet = SyntaxSet::load_defaults_newlines();
}

#[allow(unused)]
pub fn for_lang(lang: &str) -> Option<ClassedHTMLGenerator> {
    let sr = if let Some(sr) = SYNSET.find_syntax_by_token(lang) {
        sr
    } else {
        eprintln!("WARNING: Unknown language {:?}.  No highlighting for this block.", lang);
        return None;
    };

    Some(ClassedHTMLGenerator::new_with_class_style(
        sr, &SYNSET, STYLE,
    ))
}

#[allow(unused)]
pub fn get_css(theme: &str) -> Option<String> {
    let themeset = ThemeSet::load_defaults();
    if let Some(theme) = themeset.themes.get(theme) {
        Some(css_for_theme_with_class_style(theme, STYLE))
    } else {
        println!(
            "cargo:warning=No theme {:?}. Known: {:?}",
            theme,
            themeset.themes.keys()
        );
        None
    }
}
