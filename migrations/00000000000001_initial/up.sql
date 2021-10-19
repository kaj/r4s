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
  orig_md text not null
);

create unique index idx_post_year_slug on posts (slug, year_of_date(posted_at));
select diesel_manage_updated_at('posts');
