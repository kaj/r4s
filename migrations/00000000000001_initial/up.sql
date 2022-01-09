-- Create the sql schema for blog posts

-- Like diesel_manage_updated_at, but only update if orig_md has changed.
CREATE OR REPLACE FUNCTION r4s_manage_updated_at(_tbl regclass) RETURNS VOID AS $$
BEGIN
    EXECUTE format('CREATE TRIGGER set_updated_at BEFORE UPDATE ON %s
                    FOR EACH ROW EXECUTE PROCEDURE r4s_set_updated_at()', _tbl);
END;
$$ LANGUAGE plpgsql;

-- Like diesel_set_updated_at, but only update if orig_md has changed.
CREATE OR REPLACE FUNCTION r4s_set_updated_at() RETURNS trigger AS $$
BEGIN
    IF (
        NEW.orig_md IS DISTINCT FROM OLD.orig_md AND
        NEW.updated_at IS NOT DISTINCT FROM OLD.updated_at
    ) THEN
        NEW.updated_at := current_timestamp;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

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
  content text not null, -- The prerendered html content of the post.
  teaser text not null, -- The same start of content, may be == content if short.
  front_image varchar, -- image url.
  description varchar not null, -- Short plaintext teaser.
  use_leaflet boolean not null, -- true if leaflet maps are used.
  orig_md text not null
);

create unique index idx_post_year_slug_l on posts (slug, year_of_date(posted_at), lang);
select r4s_manage_updated_at('posts');

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
select r4s_manage_updated_at('metapages');

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
