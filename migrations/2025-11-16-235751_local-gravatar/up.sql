-- Your SQL goes here
create table avatars (
  id serial primary key,
  updated_at timestamp with time zone not null default now(),
  email varchar not null,
  -- A 96 bit random number in url-safe base64.
  -- Yes, it's allways 16 characters, but that just complicates things.
  slug varchar not null,
  mime varchar not null,
  content bytea not null
);

create unique index idx_avatar_email on avatars (email);
create unique index idx_avatar_slug on avatars (slug);

