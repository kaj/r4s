table! {
    posts (id) {
        id -> Int4,
        posted_at -> Timestamptz,
        updated_at -> Timestamptz,
        slug -> Varchar,
        title -> Varchar,
        lang -> Varchar,
        content -> Text,
        orig_md -> Text,
    }
}
