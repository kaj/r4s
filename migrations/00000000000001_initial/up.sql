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

create unique index idx_post_year_slug_l on posts (slug, year_of_date(posted_at), lang);
select diesel_manage_updated_at('posts');

create table metapages (
  id serial primary key,
  updated_at timestamp with time zone not null default now(),
  slug varchar not null,
  title varchar not null,
  lang varchar(2) not null, -- TODO enum?
  content text not null,
  orig_md text not null
);

create unique index idx_page_slug_l on metapages (slug, lang);
select diesel_manage_updated_at('metapages');

create table tags (
  id serial primary key,
  slug varchar not null,
  name varchar not null
);
create unique index idx_tags_tag on tags (name);
create unique index idx_tags_slug ON tags (slug);

create table post_tags (
  id serial primary key,
  post_id integer not null references posts (id) on delete cascade,
  tag_id integer not null references tags (id)
);
create unique index idx_post_tags_rel on post_tags (post_id, tag_id);

create function has_lang(yearp smallint, slugp varchar, langp varchar(2))
  returns bool
  language sql immutable strict parallel safe
  as $func$
  select count(*) > 0 from posts p where year_of_date(posted_at) = yearp and p.slug = slugp and p.lang = langp
  $func$;

create table assets (
  id serial primary key,
  updated_at timestamp with time zone not null default now(),
  year smallint not null,
  name varchar not null,
  mime varchar not null,
  content bytea not null
);

create unique index idx_asset_path on assets (year, name);
select diesel_manage_updated_at('assets');

create table comments (
  id serial primary key,
  post_id integer not null references posts (id) on delete cascade,
  posted_at timestamp with time zone not null default now(),
  content text not null,
  name varchar not null,
  email varchar not null,
  url varchar,
  from_host inet not null,
  raw_md text not null,
  is_public boolean not null,
  is_spam boolean not null default false
);
