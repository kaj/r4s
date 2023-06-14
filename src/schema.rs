// @generated automatically by Diesel CLI.

diesel::table! {
    assets (id) {
        id -> Int4,
        updated_at -> Timestamptz,
        year -> Int2,
        name -> Varchar,
        mime -> Varchar,
        content -> Bytea,
    }
}

diesel::table! {
    comments (id) {
        id -> Int4,
        post_id -> Int4,
        posted_at -> Timestamptz,
        content -> Text,
        name -> Varchar,
        email -> Varchar,
        url -> Nullable<Varchar>,
        from_host -> Inet,
        raw_md -> Text,
        is_public -> Bool,
        is_spam -> Bool,
    }
}

diesel::table! {
    metapages (id) {
        id -> Int4,
        updated_at -> Timestamptz,
        slug -> Varchar,
        title -> Varchar,
        #[max_length = 2]
        lang -> Varchar,
        content -> Text,
        orig_md -> Text,
    }
}

diesel::table! {
    post_tags (post_id, tag_id) {
        post_id -> Int4,
        tag_id -> Int4,
    }
}

diesel::table! {
    posts (id) {
        id -> Int4,
        posted_at -> Timestamptz,
        updated_at -> Timestamptz,
        slug -> Varchar,
        title -> Varchar,
        #[max_length = 2]
        lang -> Varchar,
        content -> Text,
        teaser -> Text,
        front_image -> Nullable<Varchar>,
        description -> Varchar,
        use_leaflet -> Bool,
        orig_md -> Text,
    }
}

diesel::table! {
    tags (id) {
        id -> Int4,
        slug -> Varchar,
        name -> Varchar,
    }
}

diesel::joinable!(comments -> posts (post_id));
diesel::joinable!(post_tags -> posts (post_id));
diesel::joinable!(post_tags -> tags (tag_id));

diesel::allow_tables_to_appear_in_same_query!(
    assets, comments, metapages, post_tags, posts, tags,
);
