use crate::dbopt::DbOpt;
use crate::models::PostComment;
use crate::schema::comments::dsl as c;
use anyhow::{ensure, Result};
use chrono::{DateTime, Utc};
use diesel::dsl::sql;
use diesel::prelude::*;
use std::fmt::{self, Display};
use std::io::{stdin, stdout, Write};
use structopt::StructOpt;

#[derive(StructOpt)]
pub struct Args {
    #[structopt(flatten)]
    db: DbOpt,

    /// Only list the status and moderation queue.
    ///
    /// Does not wait for input, does not modify anything.
    #[structopt(long, short)]
    list: bool,

    /// Be silent if there is no pending comments.
    #[structopt(long, short)]
    silent: bool,
}

impl Args {
    pub fn run(self) -> Result<()> {
        let db = self.db.get_db()?;
        let (public, spam, pending) = c::comments
            .select(((c::is_public, c::is_spam), sql("count(*)")))
            .group_by((c::is_public, c::is_spam))
            .load::<((bool, bool), i64)>(&db)
            .map(|raw| {
                let mut public = 0;
                let mut spam = 0;
                let mut pending = 0;
                for ((is_public, is_spam), count) in raw {
                    if is_public {
                        public += count;
                    } else if is_spam {
                        spam += count;
                    } else {
                        pending += count;
                    }
                }
                (public, spam, pending)
            })?;

        if pending > 0 || !self.silent {
            println!(
                "There are {} pending, {} public, and {} spam comments.",
                pending, public, spam
            );
        }

        for comment in PostComment::mod_queue(&db)? {
            let p = comment.p();
            println!(
                "\n{} by {:?} <{}> {:?}\nOn {} ({})",
                Ago(comment.posted_at.raw()),
                comment.name,
                comment.email,
                comment.url,
                p.title,
                p.year
            );
            showlimited(&comment.content);

            if !self.list {
                match prompt(
                    "How about this comment?",
                    &["ok", "spam", "quit"],
                )? {
                    "ok" => {
                        println!("Should allow this");
                        do_moderate(comment.id(), false, &db)?;
                    }
                    "spam" => {
                        println!("Should disallow this");
                        do_moderate(comment.id(), true, &db)?;
                    }
                    _ => {
                        println!("Giving up for now");
                        return Ok(());
                    }
                }
            }
        }

        Ok(())
    }
}

struct Ago(DateTime<Utc>);

impl Display for Ago {
    fn fmt(&self, out: &mut fmt::Formatter) -> fmt::Result {
        let date = self.0;
        let elapsed_mins = (Utc::now() - date).num_minutes();
        if elapsed_mins == 0 {
            out.write_str("Now")
        } else if elapsed_mins < 45 {
            write!(out, "{} min ago", elapsed_mins)
        } else if elapsed_mins < 60 * 18 {
            date.format("%H:%M").fmt(out)
        } else {
            date.format("%Y-%m-%d %H:%M").fmt(out)
        }
    }
}
fn showlimited(content: &str) {
    for (i, line) in content.trim().lines().enumerate() {
        print!(" > ");
        let mut chars = line.chars();
        for c in (&mut chars).take(72) {
            print!("{}", c);
        }
        if chars.next().is_some() {
            print!(" …");
        }
        println!();

        if i > 4 {
            return;
        }
    }
}

fn do_moderate(comment: i32, spam: bool, db: &PgConnection) -> Result<()> {
    diesel::update(c::comments)
        .filter(c::id.eq(comment))
        .set((c::is_public.eq(!spam), c::is_spam.eq(spam)))
        .execute(db)?;
    Ok(())
}

fn prompt<'v>(prompt: &str, alternatives: &[&'v str]) -> Result<&'v str> {
    let input = stdin();
    let mut buf = String::new();
    loop {
        print!("{} {:?} ", prompt, alternatives);
        stdout().flush()?;
        buf.clear();
        ensure!(input.read_line(&mut buf)? > 0, "Expected some input");
        let buf = buf.trim();
        if !buf.is_empty() {
            for alt in alternatives.iter() {
                if alt.starts_with(buf) {
                    return Ok(alt);
                }
            }
        }
    }
}
