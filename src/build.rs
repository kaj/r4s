use ructe::{Result, Ructe};

fn main() -> Result<()> {
    let mut ructe = Ructe::from_env()?;
    //let mut statics = ructe.statics()?;
    //statics.add_files("res/img")?;
    //statics.add_file("res/search.js")?;
    //statics.add_file("res/sortable.js")?;
    //statics.add_sass_file("res/style.scss")?;
    ructe.compile_templates("templates")
}
