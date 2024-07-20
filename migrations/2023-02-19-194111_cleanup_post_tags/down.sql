alter table post_tags drop constraint post_tags_pkey;
alter table post_tags add column id serial primary key;
