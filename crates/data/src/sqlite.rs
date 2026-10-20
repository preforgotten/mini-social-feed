use async_trait::async_trait;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::Row;

use mini_social_feed_domain::{
    Cursor, Direction, EntityId, MediaRef, Page, PageParams, Post, PostId, PostSearchQuery,
    Relation, User, UserId, UserSearchQuery,
};

use crate::error::DataError;
use crate::ports::{GraphStore, MediaStore, PostStore, UserStore};

/// SQLite-backed store with auto-migration on connect.
#[derive(Clone)]
pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    pub async fn connect(path: &str) -> Result<Self, DataError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(path)
            .await
            .map_err(|e| DataError::Internal(e.to_string()))?;

        let store = Self { pool };
        store.migrate().await?;
        store.enable_wal().await?;
        Ok(store)
    }

    async fn migrate(&self) -> Result<(), DataError> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY,
                username TEXT UNIQUE NOT NULL,
                password_hash TEXT NOT NULL DEFAULT '',
                api_key_hash TEXT,
                bio TEXT,
                nostr_pubkey TEXT UNIQUE,
                email TEXT,
                created_at INTEGER NOT NULL
            )",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DataError::Internal(e.to_string()))?;

        // Add columns for databases created before these fields existed.
        // Errors (e.g. column already exists) are ignored.
        let _ = sqlx::query("ALTER TABLE users ADD COLUMN password_hash TEXT NOT NULL DEFAULT ''")
            .execute(&self.pool)
            .await;
        let _ = sqlx::query("ALTER TABLE users ADD COLUMN api_key_hash TEXT")
            .execute(&self.pool)
            .await;
        let _ = sqlx::query("ALTER TABLE users ADD COLUMN avatar_media_id TEXT")
            .execute(&self.pool)
            .await;
        let _ = sqlx::query("ALTER TABLE users ADD COLUMN rate_limit_tier TEXT")
            .execute(&self.pool)
            .await;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS posts (
                id TEXT PRIMARY KEY,
                author_id TEXT NOT NULL REFERENCES users(id),
                content TEXT NOT NULL,
                parent_id TEXT REFERENCES posts(id),
                media_ids TEXT NOT NULL DEFAULT '[]',
                created_at INTEGER NOT NULL,
                nostr_event_id TEXT,
                nostr_sig TEXT
            )",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DataError::Internal(e.to_string()))?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_posts_author ON posts(author_id)")
            .execute(&self.pool)
            .await
            .map_err(|e| DataError::Internal(e.to_string()))?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_posts_parent ON posts(parent_id)")
            .execute(&self.pool)
            .await
            .map_err(|e| DataError::Internal(e.to_string()))?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_posts_created ON posts(created_at)")
            .execute(&self.pool)
            .await
            .map_err(|e| DataError::Internal(e.to_string()))?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS relations (
                from_type TEXT NOT NULL,
                from_id TEXT NOT NULL,
                to_type TEXT NOT NULL,
                to_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                PRIMARY KEY (from_type, from_id, to_type, to_id, kind)
            )",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DataError::Internal(e.to_string()))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_relations_from ON relations(from_type, from_id, kind)",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DataError::Internal(e.to_string()))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_relations_to ON relations(to_type, to_id, kind)",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DataError::Internal(e.to_string()))?;

        // Add media_ids column (ignore error if already exists)
        let _ = sqlx::query("ALTER TABLE posts ADD COLUMN media_ids TEXT NOT NULL DEFAULT '[]'")
            .execute(&self.pool)
            .await;

        // ── media table ──
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS media (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL REFERENCES users(id),
                content_type TEXT NOT NULL,
                size_bytes INTEGER NOT NULL,
                deleted INTEGER NOT NULL DEFAULT 0,
                backend TEXT NOT NULL DEFAULT 'local'
            )",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DataError::Internal(e.to_string()))?;

        // Add backend column for databases created before this field existed.
        let _ = sqlx::query("ALTER TABLE media ADD COLUMN backend TEXT NOT NULL DEFAULT 'local'")
            .execute(&self.pool)
            .await;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_media_user ON media(user_id)")
            .execute(&self.pool)
            .await
            .map_err(|e| DataError::Internal(e.to_string()))?;

        Ok(())
    }

    /// Enable WAL mode for better concurrent read/write performance.
    async fn enable_wal(&self) -> Result<(), DataError> {
        sqlx::query("PRAGMA journal_mode=WAL")
            .execute(&self.pool)
            .await
            .map_err(|e| DataError::Internal(e.to_string()))?;
        Ok(())
    }
}

fn user_from_row(r: &sqlx::sqlite::SqliteRow) -> User {
    User {
        id: UserId(r.get("id")),
        username: r.get("username"),
        password_hash: r.get("password_hash"),
        api_key_hash: r.get("api_key_hash"),
        bio: r.get("bio"),
        nostr_pubkey: r.get("nostr_pubkey"),
        email: r.get("email"),
        avatar_media_id: r.get("avatar_media_id"),
        rate_limit_tier: r.get("rate_limit_tier"),
        created_at: r.get("created_at"),
    }
}

// ---------------------------------------------------------------------------
// UserStore impl
// ---------------------------------------------------------------------------

#[async_trait]
impl UserStore for SqliteStore {
    async fn create(&self, user: User) -> Result<User, DataError> {
        // User arrives with password_hash pre-hashed — store as-is.
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, api_key_hash, bio, nostr_pubkey, email, avatar_media_id, rate_limit_tier, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&user.id.0)
        .bind(&user.username)
        .bind(&user.password_hash)
        .bind(&user.api_key_hash)
        .bind(&user.bio)
        .bind(&user.nostr_pubkey)
        .bind(&user.email)
        .bind(&user.avatar_media_id)
        .bind(&user.rate_limit_tier)
        .bind(user.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(ref dbe) if dbe.is_unique_violation() => {
                DataError::Duplicate("username or nostr pubkey already taken".into())
            }
            _ => DataError::Internal(e.to_string()),
        })?;

        Ok(user)
    }

    async fn get_by_id(&self, id: &UserId) -> Result<Option<User>, DataError> {
        let row = sqlx::query(
            "SELECT id, username, password_hash, api_key_hash, bio, nostr_pubkey, email, avatar_media_id, rate_limit_tier, created_at
             FROM users WHERE id = ?",
        )
        .bind(&id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DataError::Internal(e.to_string()))?;

        Ok(row.as_ref().map(user_from_row))
    }

    async fn get_by_username(&self, name: &str) -> Result<Option<User>, DataError> {
        let row = sqlx::query(
            "SELECT id, username, password_hash, api_key_hash, bio, nostr_pubkey, email, avatar_media_id, rate_limit_tier, created_at
             FROM users WHERE username = ?",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DataError::Internal(e.to_string()))?;

        Ok(row.as_ref().map(user_from_row))
    }

    async fn get_by_api_key_hash(&self, hash: &str) -> Result<Option<UserId>, DataError> {
        let row = sqlx::query("SELECT id FROM users WHERE api_key_hash = ?")
            .bind(hash)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DataError::Internal(e.to_string()))?;

        Ok(row.map(|r: sqlx::sqlite::SqliteRow| UserId(r.get("id"))))
    }

    async fn set_api_key_hash(&self, user_id: &UserId, hash: String) -> Result<(), DataError> {
        sqlx::query("UPDATE users SET api_key_hash = ? WHERE id = ?")
            .bind(&hash)
            .bind(&user_id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| DataError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn set_nostr_pubkey(&self, user_id: &UserId, pubkey: String) -> Result<(), DataError> {
        sqlx::query("UPDATE users SET nostr_pubkey = ? WHERE id = ?")
            .bind(&pubkey)
            .bind(&user_id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| match e {
                sqlx::Error::Database(ref dbe) if dbe.is_unique_violation() => {
                    DataError::Duplicate("nostr pubkey already linked to another user".into())
                }
                _ => DataError::Internal(e.to_string()),
            })?;

        Ok(())
    }

    async fn set_avatar(
        &self,
        user_id: &UserId,
        media_id: Option<String>,
    ) -> Result<(), DataError> {
        sqlx::query("UPDATE users SET avatar_media_id = ? WHERE id = ?")
            .bind(&media_id)
            .bind(&user_id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| DataError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn update_profile(
        &self,
        user_id: &UserId,
        bio: Option<String>,
        email: Option<String>,
    ) -> Result<User, DataError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DataError::Internal(e.to_string()))?;

        if let Some(ref b) = bio {
            sqlx::query("UPDATE users SET bio = ? WHERE id = ?")
                .bind(b)
                .bind(&user_id.0)
                .execute(&mut *tx)
                .await
                .map_err(|e| DataError::Internal(e.to_string()))?;
        }
        if let Some(ref e) = email {
            sqlx::query("UPDATE users SET email = ? WHERE id = ?")
                .bind(e)
                .bind(&user_id.0)
                .execute(&mut *tx)
                .await
                .map_err(|e2| DataError::Internal(e2.to_string()))?;
        }

        tx.commit()
            .await
            .map_err(|e| DataError::Internal(e.to_string()))?;

        self.get_by_id(user_id)
            .await?
            .ok_or_else(|| DataError::NotFound("user not found".into()))
    }

    async fn search(&self, query: &UserSearchQuery) -> Result<Vec<User>, DataError> {
        if let Some(ref username) = query.username {
            let row = sqlx::query(
                "SELECT id, username, password_hash, api_key_hash, bio, nostr_pubkey, email, avatar_media_id, rate_limit_tier, created_at
                 FROM users WHERE username = ?",
            )
            .bind(username)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DataError::Internal(e.to_string()))?;

            if let Some(r) = row {
                return Ok(vec![user_from_row(&r)]);
            }
        }
        Ok(vec![])
    }

    async fn get_space_used(&self, user_id: &UserId) -> Result<u64, DataError> {
        let row = sqlx::query(
            "SELECT COALESCE(SUM(size_bytes), 0) AS total FROM media WHERE user_id = ? AND deleted = 0",
        )
        .bind(&user_id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DataError::Internal(e.to_string()))?;

        let total: i64 = row.get("total");
        Ok(total as u64)
    }
}

// ---------------------------------------------------------------------------
// PostStore impl
// ---------------------------------------------------------------------------

#[async_trait]
impl PostStore for SqliteStore {
    async fn create_post(&self, post: Post) -> Result<Post, DataError> {
        let media_ids_json = serde_json::to_string(&post.media_ids).unwrap_or_default();
        sqlx::query(
            "INSERT INTO posts (id, author_id, content, parent_id, media_ids, created_at, nostr_event_id, nostr_sig)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&post.id.0)
        .bind(&post.author_id.0)
        .bind(&post.content)
        .bind(post.parent_id.as_ref().map(|p| &p.0))
        .bind(&media_ids_json)
        .bind(post.created_at)
        .bind(&post.nostr_event_id)
        .bind(&post.nostr_sig)
        .execute(&self.pool)
        .await
        .map_err(|e| DataError::Internal(e.to_string()))?;

        Ok(post)
    }

    async fn get_post(&self, id: &PostId) -> Result<Option<Post>, DataError> {
        let row = sqlx::query(
            "SELECT id, author_id, content, parent_id, media_ids, created_at, nostr_event_id, nostr_sig
             FROM posts WHERE id = ?",
        )
        .bind(&id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DataError::Internal(e.to_string()))?;

        Ok(row.map(|r| {
            let media_str: String = r.get("media_ids");
            let media_ids: Vec<String> = serde_json::from_str(&media_str).unwrap_or_default();
            Post {
                id: PostId(r.get("id")),
                author_id: UserId(r.get("author_id")),
                content: r.get("content"),
                parent_id: r.get::<Option<String>, _>("parent_id").map(PostId),
                media_ids,
                created_at: r.get("created_at"),
                nostr_event_id: r.get("nostr_event_id"),
                nostr_sig: r.get("nostr_sig"),
            }
        }))
    }

    async fn get_user_posts(
        &self,
        user_id: &UserId,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<Post>, usize), DataError> {
        let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM posts WHERE author_id = ?")
            .bind(&user_id.0)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DataError::Internal(e.to_string()))?;

        let rows = sqlx::query(
            "SELECT id, author_id, content, parent_id, media_ids, created_at, nostr_event_id, nostr_sig
             FROM posts WHERE author_id = ?
             ORDER BY created_at DESC
             LIMIT ? OFFSET ?",
        )
        .bind(&user_id.0)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DataError::Internal(e.to_string()))?;

        let posts: Vec<Post> = rows
            .iter()
            .map(|r| {
                let media_str: String = r.get("media_ids");
                let media_ids: Vec<String> = serde_json::from_str(&media_str).unwrap_or_default();
                Post {
                    id: PostId(r.get("id")),
                    author_id: UserId(r.get("author_id")),
                    content: r.get("content"),
                    parent_id: r.get::<Option<String>, _>("parent_id").map(PostId),
                    media_ids,
                    created_at: r.get("created_at"),
                    nostr_event_id: r.get("nostr_event_id"),
                    nostr_sig: r.get("nostr_sig"),
                }
            })
            .collect();

        Ok((posts, total.0 as usize))
    }

    async fn get_posts(
        &self,
        params: &PageParams,
        author_id: Option<&UserId>,
    ) -> Result<Page<Post>, DataError> {
        let page_size = params.page_size() as i64;

        // Build WHERE clause
        let mut conditions: Vec<String> = vec!["1=1".into()];
        if let Some(aid) = author_id {
            conditions.push(format!("author_id = '{}'", aid.0.replace('\'', "''")));
        }
        if let Some(since) = params.since {
            conditions.push(format!("created_at >= {since}"));
        }
        if let Some(until) = params.until {
            conditions.push(format!("created_at <= {until}"));
        }

        let where_base = conditions.join(" AND ");

        // Total count
        let count_sql = format!("SELECT COUNT(*) FROM posts WHERE {where_base}");
        let total: (i64,) = sqlx::query_as(&count_sql)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DataError::Internal(e.to_string()))?;

        let posts = if let Some(ref cursor) = params.before {
            // Items older than cursor (load next page)
            if let Some((cts, cid)) = cursor.to_time_id() {
                let sql = format!(
                    "SELECT id, author_id, content, parent_id, media_ids, created_at, nostr_event_id, nostr_sig \
                     FROM posts WHERE {where_base} \
                     AND (created_at < ? OR (created_at = ? AND id < ?)) \
                     ORDER BY created_at DESC, id DESC LIMIT ?"
                );
                sqlx::query(&sql)
                    .bind(cts)
                    .bind(cts)
                    .bind(&cid)
                    .bind(page_size)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(|e| DataError::Internal(e.to_string()))?
            } else {
                vec![]
            }
        } else if let Some(ref cursor) = params.after {
            // Items newer than cursor (load previous page, then reverse)
            if let Some((cts, cid)) = cursor.to_time_id() {
                let sql = format!(
                    "SELECT id, author_id, content, parent_id, media_ids, created_at, nostr_event_id, nostr_sig \
                     FROM posts WHERE {where_base} \
                     AND (created_at > ? OR (created_at = ? AND id > ?)) \
                     ORDER BY created_at ASC, id ASC LIMIT ?"
                );
                let mut rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(&sql)
                    .bind(cts)
                    .bind(cts)
                    .bind(&cid)
                    .bind(page_size)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(|e| DataError::Internal(e.to_string()))?;
                rows.reverse(); // back to newest-first
                rows
            } else {
                vec![]
            }
        } else {
            // First page — no cursor
            let sql = format!(
                "SELECT id, author_id, content, parent_id, media_ids, created_at, nostr_event_id, nostr_sig \
                 FROM posts WHERE {where_base} \
                 ORDER BY created_at DESC, id DESC LIMIT ?"
            );
            sqlx::query(&sql)
                .bind(page_size)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| DataError::Internal(e.to_string()))?
        };

        let items: Vec<Post> = posts
            .iter()
            .map(|r| {
                let media_str: String = r.get("media_ids");
                let media_ids: Vec<String> = serde_json::from_str(&media_str).unwrap_or_default();
                Post {
                    id: PostId(r.get("id")),
                    author_id: UserId(r.get("author_id")),
                    content: r.get("content"),
                    parent_id: r.get::<Option<String>, _>("parent_id").map(PostId),
                    media_ids,
                    created_at: r.get("created_at"),
                    nostr_event_id: r.get("nostr_event_id"),
                    nostr_sig: r.get("nostr_sig"),
                }
            })
            .collect();

        let first_cursor = items
            .first()
            .map(|p| Cursor::from_time_id(p.created_at, &p.id.0));
        let last_cursor = items
            .last()
            .map(|p| Cursor::from_time_id(p.created_at, &p.id.0));

        Ok(Page {
            items,
            total: total.0 as usize,
            page_size: page_size as usize,
            first: first_cursor,
            last: last_cursor,
        })
    }

    async fn get_thread_posts(&self, root_id: &PostId) -> Result<Vec<Post>, DataError> {
        let mut result = Vec::new();
        let mut ids = vec![root_id.0.clone()];

        while !ids.is_empty() {
            let placeholders: Vec<String> = ids
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", i + 1))
                .collect();
            let query = format!(
                "SELECT id, author_id, content, parent_id, media_ids, created_at, nostr_event_id, nostr_sig
                 FROM posts WHERE id IN ({}) ORDER BY created_at ASC",
                placeholders.join(",")
            );

            let mut q = sqlx::query(&query);
            for id in &ids {
                q = q.bind(id);
            }
            let rows = q
                .fetch_all(&self.pool)
                .await
                .map_err(|e| DataError::Internal(e.to_string()))?;

            let mut next_ids = Vec::new();
            for row in &rows {
                let pid: String = row.get("id");
                let parent: Option<String> = row.get("parent_id");
                let media_str: String = row.get("media_ids");
                let media_ids: Vec<String> = serde_json::from_str(&media_str).unwrap_or_default();
                result.push(Post {
                    id: PostId(pid.clone()),
                    author_id: UserId(row.get("author_id")),
                    content: row.get("content"),
                    parent_id: parent.map(PostId),
                    media_ids,
                    created_at: row.get("created_at"),
                    nostr_event_id: row.get("nostr_event_id"),
                    nostr_sig: row.get("nostr_sig"),
                });
                next_ids.push(pid);
            }

            // Find children of the batch we just fetched
            if next_ids.is_empty() {
                break;
            }
            let child_placeholders: Vec<String> = next_ids
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", i + 1))
                .collect();
            let child_query = format!(
                "SELECT id FROM posts WHERE parent_id IN ({})",
                child_placeholders.join(",")
            );
            let mut cq = sqlx::query(&child_query);
            for id in &next_ids {
                cq = cq.bind(id);
            }
            ids = cq
                .fetch_all(&self.pool)
                .await
                .map_err(|e| DataError::Internal(e.to_string()))?
                .iter()
                .map(|r: &sqlx::sqlite::SqliteRow| r.get("id"))
                .collect();
        }

        Ok(result)
    }

    async fn search_posts(&self, query: &PostSearchQuery) -> Result<Vec<Post>, DataError> {
        let limit = query.limit.unwrap_or(20).min(100);
        let offset = query.offset.unwrap_or(0);

        let mut conditions: Vec<String> = Vec::new();
        let mut binds: Vec<String> = Vec::new();

        if let Some(ref c) = query.content_contains {
            conditions.push("p.content LIKE '%' || ? || '%'".into());
            binds.push(c.clone());
        }
        if let Some(ref aid) = query.author_id {
            conditions.push("p.author_id = ?".into());
            binds.push(aid.clone());
        }
        if let Some(ref aun) = query.author_username {
            conditions.push("u.username = ?".into());
            binds.push(aun.clone());
        }
        if let Some(ref pid) = query.parent_id {
            conditions.push("p.parent_id = ?".into());
            binds.push(pid.clone());
        }

        let where_clause = if conditions.is_empty() {
            "1=1".to_string()
        } else {
            conditions.join(" AND ")
        };

        let sql = if query.author_username.is_some() {
            format!(
                "SELECT p.id, p.author_id, p.content, p.parent_id, p.media_ids, p.created_at, p.nostr_event_id, p.nostr_sig
                 FROM posts p JOIN users u ON p.author_id = u.id
                 WHERE {} ORDER BY p.created_at DESC LIMIT ? OFFSET ?",
                where_clause
            )
        } else {
            format!(
                "SELECT id, author_id, content, parent_id, media_ids, created_at, nostr_event_id, nostr_sig
                 FROM posts p WHERE {} ORDER BY created_at DESC LIMIT ? OFFSET ?",
                where_clause
            )
        };

        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = q.bind(b);
        }
        q = q.bind(limit as i64).bind(offset as i64);

        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DataError::Internal(e.to_string()))?;

        let posts = rows
            .iter()
            .map(|r| {
                let media_str: String = r.get("media_ids");
                let media_ids: Vec<String> = serde_json::from_str(&media_str).unwrap_or_default();
                Post {
                    id: PostId(r.get("id")),
                    author_id: UserId(r.get("author_id")),
                    content: r.get("content"),
                    parent_id: r.get::<Option<String>, _>("parent_id").map(PostId),
                    media_ids,
                    created_at: r.get("created_at"),
                    nostr_event_id: r.get("nostr_event_id"),
                    nostr_sig: r.get("nostr_sig"),
                }
            })
            .collect();

        Ok(posts)
    }
}

// ---------------------------------------------------------------------------
// EntityId helpers for SQL serialization
// ---------------------------------------------------------------------------

fn entity_parts(e: &EntityId) -> (&'static str, &str) {
    match e {
        EntityId::User(uid) => ("user", &uid.0),
        EntityId::Post(pid) => ("post", &pid.0),
    }
}

fn entity_from_parts(ty: &str, id: &str) -> EntityId {
    match ty {
        "user" => EntityId::User(UserId(id.to_string())),
        "post" => EntityId::Post(PostId(id.to_string())),
        _ => EntityId::Post(PostId(id.to_string())),
    }
}

fn relation_kind_str(kind: &Relation) -> &'static str {
    match kind {
        Relation::Reply => "reply",
        Relation::Subscribe => "subscribe",
        Relation::ConnectRequest => "connect_request",
        Relation::Connected => "connected",
        Relation::Block => "block",
        Relation::Mention => "mention",
    }
}

#[allow(dead_code)]
fn relation_kind_from_str(s: &str) -> Relation {
    match s {
        "reply" => Relation::Reply,
        "subscribe" => Relation::Subscribe,
        "connect_request" => Relation::ConnectRequest,
        "connected" => Relation::Connected,
        "block" => Relation::Block,
        "mention" => Relation::Mention,
        _ => Relation::Reply,
    }
}

// ---------------------------------------------------------------------------
// GraphStore impl
// ---------------------------------------------------------------------------

#[async_trait]
impl GraphStore for SqliteStore {
    async fn relate(
        &self,
        from: &EntityId,
        to: &EntityId,
        kind: Relation,
    ) -> Result<(), DataError> {
        let (ft, fi) = entity_parts(from);
        let (tt, ti) = entity_parts(to);
        let k = relation_kind_str(&kind);

        sqlx::query(
            "INSERT OR IGNORE INTO relations (from_type, from_id, to_type, to_id, kind)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(ft)
        .bind(fi)
        .bind(tt)
        .bind(ti)
        .bind(k)
        .execute(&self.pool)
        .await
        .map_err(|e| DataError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn unrelate(
        &self,
        from: &EntityId,
        to: &EntityId,
        kind: Relation,
    ) -> Result<(), DataError> {
        let (ft, fi) = entity_parts(from);
        let (tt, ti) = entity_parts(to);
        let k = relation_kind_str(&kind);

        sqlx::query(
            "DELETE FROM relations WHERE from_type = ? AND from_id = ? AND to_type = ? AND to_id = ? AND kind = ?",
        )
        .bind(ft)
        .bind(fi)
        .bind(tt)
        .bind(ti)
        .bind(k)
        .execute(&self.pool)
        .await
        .map_err(|e| DataError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn related(
        &self,
        entity: &EntityId,
        kind: Relation,
        dir: Direction,
    ) -> Result<Vec<EntityId>, DataError> {
        let k = relation_kind_str(&kind);

        let rows = match dir {
            Direction::Outbound => {
                let (ft, fi) = entity_parts(entity);
                sqlx::query(
                    "SELECT to_type, to_id FROM relations WHERE from_type = ? AND from_id = ? AND kind = ?",
                )
                .bind(ft)
                .bind(fi)
                .bind(k)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| DataError::Internal(e.to_string()))?
            }
            Direction::Inbound => {
                let (tt, ti) = entity_parts(entity);
                sqlx::query(
                    "SELECT from_type, from_id FROM relations WHERE to_type = ? AND to_id = ? AND kind = ?",
                )
                .bind(tt)
                .bind(ti)
                .bind(k)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| DataError::Internal(e.to_string()))?
            }
        };

        Ok(rows
            .iter()
            .map(|r| {
                let (ty_col, id_col) = match dir {
                    Direction::Outbound => ("to_type", "to_id"),
                    Direction::Inbound => ("from_type", "from_id"),
                };
                let ty: String = r.get(ty_col);
                let id: String = r.get(id_col);
                entity_from_parts(&ty, &id)
            })
            .collect())
    }

    async fn check_relation(
        &self,
        a: &EntityId,
        b: &EntityId,
        kind: Relation,
    ) -> Result<bool, DataError> {
        let (ft, fi) = entity_parts(a);
        let (tt, ti) = entity_parts(b);
        let k = relation_kind_str(&kind);

        let row = sqlx::query(
            "SELECT 1 FROM relations WHERE from_type = ? AND from_id = ? AND to_type = ? AND to_id = ? AND kind = ? LIMIT 1",
        )
        .bind(ft)
        .bind(fi)
        .bind(tt)
        .bind(ti)
        .bind(k)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DataError::Internal(e.to_string()))?;

        Ok(row.is_some())
    }
}

#[async_trait]
impl MediaStore for SqliteStore {
    async fn upload(
        &self,
        user_id: &UserId,
        data: Vec<u8>,
        _filename: &str,
        content_type: &str,
    ) -> Result<MediaRef, DataError> {
        let id = uuid::Uuid::new_v4().to_string();
        let url = format!("/api/media/{}", id);

        sqlx::query(
            "INSERT INTO media (id, user_id, content_type, size_bytes, deleted, backend) VALUES (?, ?, ?, ?, 0, 'local')",
        )
        .bind(&id)
        .bind(&user_id.0)
        .bind(content_type)
        .bind(data.len() as i64)
        .execute(&self.pool)
        .await
        .map_err(|e| DataError::Internal(e.to_string()))?;

        Ok(MediaRef {
            id,
            url,
            backend: "local".into(),
            content_type: content_type.to_string(),
            size_bytes: data.len() as u64,
            deleted: false,
        })
    }

    async fn delete(&self, user_id: &UserId, id: &str) -> Result<(), DataError> {
        let result = sqlx::query(
            "UPDATE media SET deleted = 1 WHERE id = ? AND user_id = ? AND deleted = 0",
        )
        .bind(id)
        .bind(&user_id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| DataError::Internal(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(DataError::NotFound("media not found or not owner".into()));
        }
        Ok(())
    }

    async fn get_url(&self, id: &str) -> Result<Option<String>, DataError> {
        let row = sqlx::query("SELECT deleted FROM media WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DataError::Internal(e.to_string()))?;

        match row {
            Some(r) => {
                let deleted: i64 = r.get("deleted");
                if deleted == 0 {
                    Ok(Some(format!("/api/media/{}", id)))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    async fn resolve_batch(&self, ids: &[String]) -> Result<Vec<MediaRef>, DataError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let placeholders: Vec<String> = ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect();
        let query_str = format!(
            "SELECT id, content_type, size_bytes, deleted, backend FROM media WHERE id IN ({})",
            placeholders.join(",")
        );

        let mut q = sqlx::query(&query_str);
        for id in ids {
            q = q.bind(id);
        }

        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DataError::Internal(e.to_string()))?;

        let mut refs = Vec::new();
        for row in &rows {
            let deleted: i64 = row.get("deleted");
            let id: String = row.get("id");
            refs.push(MediaRef {
                id: id.clone(),
                url: if deleted == 0 {
                    format!("/api/media/{}", id)
                } else {
                    String::new()
                },
                backend: row.get("backend"),
                content_type: row.get("content_type"),
                size_bytes: row.get::<i64, _>("size_bytes") as u64,
                deleted: deleted != 0,
            });
        }
        Ok(refs)
    }
}
