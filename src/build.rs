mod syntax_hl;
use ructe::{Result, Ructe};
use syntax_hl::get_css;

fn main() -> Result<()> {
    let mut ructe = Ructe::from_env()?;
    let mut statics = ructe.statics()?;
    // Use a standard synect style, but replace some colors for improved contrast.
    statics.add_file_data(
        "shl.css",
        get_css("Solarized (light)")
            .unwrap()
            .replace("#268bd2;", "#1578bd;")
            .replace("#2aa198;", "#115e58;")
            .replace("#657b83;", "#48585e;")
            .replace("#6c71c4;", "#575db7;")
            .replace("#839496;", "#115e58;")
            .replace("#859900;", "#5b6805;")
            .replace("#b58900;", "#806100;")
            .replace("#d33682;", "#bb246d;")
            .replace("#93a1a1;", "#863135;")
            .replace("#cb4b16;", "#b73d0b;")
            .as_ref(),
    )?;
    statics.add_files("res/img")?;
    statics.add_files("res/fonts")?;
    statics.add_files("res/js")?;
    statics.add_sass_file("res/scss/r4s.scss")?;
    ructe.compile_templates("templates")
}
