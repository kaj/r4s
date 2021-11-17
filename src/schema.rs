table! {
    assets (id) {
        id -> Int4,
        updated_at -> Timestamptz,
        year -> Int2,
        name -> Varchar,
        mime -> Varchar,
        content -> Bytea,
    }
}

table! {
    post_tags (id) {
        id -> Int4,
        post_id -> Int4,
        tag_id -> Int4,
    }
}

table! {
    posts (id) {
        id -> Int4,
        posted_at -> Timestamptz,
        updated_at -> Timestamptz,
        slug -> Varchar,
        title -> Varchar,
        lang -> Varchar,
        content -> Text,
        teaser -> Text,
        orig_md -> Text,
    }
}

table! {
    tags (id) {
        id -> Int4,
        slug -> Varchar,
        name -> Varchar,
    }
}

joinable!(post_tags -> posts (post_id));
joinable!(post_tags -> tags (tag_id));

allow_tables_to_appear_in_same_query!(assets, post_tags, posts, tags,);
