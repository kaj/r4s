use anyhow::Result;
use ructe::Ructe;

fn main() -> Result<()> {
    let mut ructe = Ructe::from_env()?;
    let mut statics = ructe.statics()?;
    // Use a standard synect style, but replace some colors for improved contrast.
    statics.add_file_data("shl.css", b"/* No longer used. */\n")?;
    statics.add_files("res/img")?;
    statics.add_files("res/fonts")?;
    statics.add_files("res/js")?;
    statics.add_files_as("res/leaflet-1.7.1", "ll171")?;
    statics.add_sass_file("res/scss/r4s.scss")?;
    Ok(ructe.compile_templates("templates")?)
}
