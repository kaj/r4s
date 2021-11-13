-- Create the sql schema for blog posts

create function year_of_date(arg timestamp with time zone)
  returns smallint
  language sql immutable strict parallel safe
  as $func$ select cast(date_part('year', arg at time zone 'UTC') as smallint); $func$;

create table posts (
  id serial primary key,
  posted_at timestamp with time zone not null default now(),
  updated_at timestamp with time zone not null default now(),
  slug varchar not null,
  title varchar not null,
  lang varchar(2) not null, -- TODO enum?
  content text not null,
  teaser text not null, -- The same start of content, may be all if short.
  orig_md text not null
);

create unique index idx_post_year_slug on posts (slug, year_of_date(posted_at), lang);
select diesel_manage_updated_at('posts');

create function recent_posts(langarg varchar, limitarg smallint)
  returns table (id integer, year smallint, slug varchar, lang varchar, title varchar,
                 posted_at timestamp with time zone, updated_at timestamp with time zone,
                 content varchar)
  language sql immutable strict parallel safe
  as $func$
    select id, year_of_date(posted_at), slug, lang, title, posted_at, updated_at, content
    from (select *, bool_or(lang=langarg) over (partition by year_of_date(posted_at), slug) as langq
          from posts) as t
  where lang=langarg or not langq
  order by updated_at desc
  limit limitarg;
  $func$;

create table tags (
  id serial primary key,
  slug varchar not null,
  name varchar not null
);
create unique index idx_tags_tag on tags (name);
create unique index idx_tags_slug ON tags (slug);

create table post_tags (
  id serial primary key,
  post_id integer not null references posts (id),
  tag_id integer not null references tags (id)
);
create unique index idx_post_tags_rel on post_tags (post_id, tag_id);
