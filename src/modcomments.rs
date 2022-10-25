use crate::dbopt::DbOpt;
use crate::models::{year_of_date, PostComment};
use crate::schema::comments::dsl as c;
use crate::schema::posts::dsl as p;
use anyhow::{ensure, Result};
use chrono::{DateTime, Utc};
use clap::Parser;
use diesel::dsl::count_star;
use diesel::prelude::*;
use std::fmt::{self, Display};
use std::io::{stdin, stdout, Write};
use textwrap::wrap;

#[derive(Parser)]
pub struct Args {
    #[clap(flatten)]
    db: DbOpt,

    /// Only list the status and moderation queue.
    ///
    /// Does not wait for input, does not modify anything.
    #[clap(long, short)]
    list: bool,

    /// Be silent if there is no pending comments.
    #[clap(long, short)]
    silent: bool,
}

impl Args {
    pub fn run(self) -> Result<()> {
        let mut db = self.db.get_db()?;
        let (public, spam, pending) = c::comments
            .group_by((c::is_public, c::is_spam))
            .select(((c::is_public, c::is_spam), count_star()))
            .load::<((bool, bool), i64)>(&mut db)
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

        let wrap_opt = textwrap::Options::with_termwidth()
            .initial_indent(" > ")
            .subsequent_indent(" > ");

        for comment in mod_queue(&mut db)? {
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
            for line in wrap(&comment.content, &wrap_opt) {
                println!("{}", line);
            }

            if !self.list {
                match prompt(
                    "How about this comment?",
                    &["ok", "spam", "quit"],
                )? {
                    "ok" => {
                        println!("Should allow this");
                        do_moderate(comment.id(), false, &mut db)?;
                    }
                    "spam" => {
                        println!("Should disallow this");
                        do_moderate(comment.id(), true, &mut db)?;
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

pub fn mod_queue(db: &mut PgConnection) -> Result<Vec<PostComment>> {
    let year = year_of_date(p::posted_at);
    c::comments
        .inner_join(p::posts.on(p::id.eq(c::post_id)))
        .select((
            (c::id, c::posted_at, c::raw_md, c::name, c::email, c::url),
            (p::id, year, p::slug, p::lang, p::title),
        ))
        .filter(c::is_public.eq(false))
        .filter(c::is_spam.eq(false))
        .order_by(c::posted_at.desc())
        .limit(50)
        .load(db)
        .map_err(Into::into)
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

fn do_moderate(
    comment: i32,
    spam: bool,
    db: &mut PgConnection,
) -> Result<()> {
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
