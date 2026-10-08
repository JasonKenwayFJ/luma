use sqlx::PgPool;

pub async fn init_db(pool: &PgPool) {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS users (
            id            TEXT PRIMARY KEY,
            email         TEXT NOT NULL UNIQUE,
            username      TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
        )",
    )
        .execute(pool)
        .await
        .expect("cannot create users table");
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS fcm_tokens (
        user_id    TEXT PRIMARY KEY,
        token      TEXT NOT NULL,
        updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
    )",
    )
        .execute(pool)
        .await
        .expect("cannot create fcm_tokens table");
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS push_tokens (
            token      TEXT PRIMARY KEY,
            user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            platform   TEXT NOT NULL CHECK (platform IN ('android', 'windows')),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )",
    )
        .execute(pool)
        .await
        .expect("cannot create push_tokens table");
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_push_tokens_user_platform ON push_tokens (user_id, platform)")
        .execute(pool)
        .await
        .expect("cannot create push token index");
    sqlx::query(
        "INSERT INTO push_tokens (user_id, token, platform, updated_at)
         SELECT user_id, token, 'android', updated_at FROM fcm_tokens WHERE btrim(token) <> ''
         ON CONFLICT (token) DO UPDATE SET user_id = EXCLUDED.user_id, updated_at = EXCLUDED.updated_at",
    )
        .execute(pool)
        .await
        .expect("cannot migrate fcm tokens");
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS messages (
            id          TEXT PRIMARY KEY,
            room_id     TEXT NOT NULL,
            user_id     TEXT NOT NULL,
            author_name TEXT NOT NULL,
            avatar_url  TEXT,
            text        TEXT NOT NULL,
            attachments JSONB,
            sent_at     TIMESTAMPTZ NOT NULL,
            created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
        )",
    )
        .execute(pool)
        .await
        .expect("cannot create messages table");

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_messages_room_sent ON messages (room_id, sent_at)")
        .execute(pool)
        .await
        .expect("cannot create index");
}