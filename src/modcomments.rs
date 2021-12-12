use crate::dbopt::DbOpt;
use crate::models::PostComment;
use crate::schema::comments::dsl as c;
use anyhow::{ensure, Result};
use diesel::dsl::sql;
use diesel::prelude::*;
use std::io::{stdin, stdout, Write};
use structopt::StructOpt;

#[derive(StructOpt)]
pub struct Args {
    #[structopt(flatten)]
    db: DbOpt,
}

impl Args {
    pub fn run(self) -> Result<()> {
        let db = self.db.get_db()?;
        let (public, spam, unmod) = c::comments
            .select(((c::is_public, c::is_spam), sql("count(*)")))
            .group_by((c::is_public, c::is_spam))
            .load::<((bool, bool), i64)>(&db)
            .map(|raw| {
                let mut public = 0;
                let mut spam = 0;
                let mut unmod = 0;
                for ((is_public, is_spam), count) in raw {
                    if is_public {
                        public += count;
                    } else if is_spam {
                        spam += count;
                    } else {
                        unmod += count;
                    }
                }
                (public, spam, unmod)
            })?;
        println!(
            "There are {} public, {} unmoderated and {} spam comments.",
            public, unmod, spam
        );

        for comment in PostComment::mod_queue(&db)? {
            //dbg!(&comment);
            let c = comment.c();
            let p = comment.p();
            println!("\nBy {:?} <{}> {:?}", c.name, c.email, c.url);
            println!("On {} ({})", p.title, p.year);
            for line in c.content.lines() {
                println!(" > {}", line);
            }

            match prompt("How about this comment?", &["ok", "spam", "quit"])?
            {
                0 => {
                    println!("Should allow this");
                    do_moderate(c.id(), false, &db)?;
                }
                1 => {
                    println!("Should disallow this");
                    do_moderate(c.id(), true, &db)?;
                }
                _ => {
                    println!("Giving up for now");
                    return Ok(());
                }
            }
        }

        Ok(())
    }
}

fn do_moderate(comment: i32, spam: bool, db: &PgConnection) -> Result<()> {
    diesel::update(c::comments)
        .filter(c::id.eq(comment))
        .set((c::is_public.eq(!spam), c::is_spam.eq(spam)))
        .execute(db)?;
    Ok(())
}

fn prompt(prompt: &str, alternatives: &[&str]) -> Result<usize> {
    let input = stdin();
    let mut buf = String::new();
    loop {
        print!("{} {:?} ", prompt, alternatives);
        stdout().flush()?;
        buf.clear();
        ensure!(input.read_line(&mut buf)? > 0, "Expected some input");
        let buf = buf.trim();
        if !buf.is_empty() {
            for (i, alt) in alternatives.iter().enumerate() {
                if alt.starts_with(buf) {
                    return Ok(i);
                }
            }
        }
    }
}
