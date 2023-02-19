alter table post_tags drop column id;
alter table post_tags add primary key (post_id, tag_id);
