mod syntax_hl;
use ructe::{Result, Ructe};
use syntax_hl::get_css;

fn main() -> Result<()> {
    let mut ructe = Ructe::from_env()?;
    let mut statics = ructe.statics()?;
    statics.add_file_data(
        "shl.css",
        get_css("Solarized (light)").unwrap().as_ref(),
    )?;
    statics.add_files("res/img")?;
    statics.add_files("res/fonts")?;
    //statics.add_file("res/search.js")?;
    //statics.add_file("res/sortable.js")?;
    statics.add_sass_file("res/scss/r4s.scss")?;
    ructe.compile_templates("templates")
}
