//! 爱的小窝 storage: the timeline, albums, media, anniversaries, the list, the
//! pet and the AI pieces.
//!
//! Every query is keyed on `couple_space_id` and none of them takes a viewer:
//! a space has exactly two members and both may read and write all of it, so
//! the membership check belongs in the service, once, rather than being spread
//! across thirty `WHERE` clauses that each have to remember it.

use chrono::{DateTime, NaiveDate, Utc};

use super::user::UserRepository;
use crate::{
    models::{
        CoupleAiKind, CoupleAiWork, CoupleAlbum, CoupleAnniversary, CoupleAnniversaryUpdate,
        CoupleMedia, CoupleMediaKind, CoupleMediaScope, CoupleMemory, CoupleMemoryUpdate,
        CouplePet, CouplePetCheckinDay, CouplePetUpdate, CoupleTodo, CoupleTodoUpdate,
        NewCoupleAiWork, NewCoupleAnniversary, NewCoupleMedia, NewCoupleMemory, NewCoupleTodo,
        UserId,
    },
    Error, Result,
};

// ---------------------------------------------------------------- rows ------

struct MemoryRow {
    id: i64,
    couple_space_id: i64,
    author_user_id: Option<UserId>,
    happened_on: NaiveDate,
    title: String,
    content: String,
    location: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<MemoryRow> for CoupleMemory {
    fn from(row: MemoryRow) -> Self {
        Self {
            id: row.id,
            space_id: row.couple_space_id,
            author_user_id: row.author_user_id,
            happened_on: row.happened_on,
            title: row.title,
            content: row.content,
            location: row.location,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

struct MediaRow {
    id: i64,
    couple_space_id: i64,
    album_id: Option<i64>,
    memory_id: Option<i64>,
    uploader_user_id: Option<UserId>,
    media_kind: i16,
    storage_backend: String,
    object_key: String,
    mime_type: String,
    size_bytes: Option<i64>,
    width: Option<i32>,
    height: Option<i32>,
    taken_on: Option<NaiveDate>,
    caption: String,
    created_at: DateTime<Utc>,
}

impl TryFrom<MediaRow> for CoupleMedia {
    type Error = Error;

    fn try_from(row: MediaRow) -> Result<Self> {
        Ok(Self {
            id: row.id,
            space_id: row.couple_space_id,
            album_id: row.album_id,
            memory_id: row.memory_id,
            uploader_user_id: row.uploader_user_id,
            // The CHECK constraint keeps this in range, so a value outside it
            // means the row was written by hand.
            kind: CoupleMediaKind::from_i16(row.media_kind).ok_or_else(|| {
                Error::Internal(format!(
                    "stored couple media kind is invalid: {}",
                    row.media_kind
                ))
            })?,
            storage_backend: row.storage_backend,
            object_key: row.object_key,
            mime_type: row.mime_type,
            size_bytes: row.size_bytes,
            width: row.width,
            height: row.height,
            taken_on: row.taken_on,
            caption: row.caption,
            created_at: row.created_at,
        })
    }
}

struct AnniversaryRow {
    id: i64,
    couple_space_id: i64,
    created_by_user_id: Option<UserId>,
    title: String,
    happens_on: NaiveDate,
    repeat_yearly: bool,
    note: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<AnniversaryRow> for CoupleAnniversary {
    fn from(row: AnniversaryRow) -> Self {
        Self {
            id: row.id,
            space_id: row.couple_space_id,
            created_by_user_id: row.created_by_user_id,
            title: row.title,
            happens_on: row.happens_on,
            repeat_yearly: row.repeat_yearly,
            note: row.note,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

struct TodoRow {
    id: i64,
    couple_space_id: i64,
    created_by_user_id: Option<UserId>,
    title: String,
    note: String,
    category: String,
    priority: i16,
    plan_date: Option<NaiveDate>,
    done_by_user_id: Option<UserId>,
    done_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<TodoRow> for CoupleTodo {
    fn from(row: TodoRow) -> Self {
        Self {
            id: row.id,
            space_id: row.couple_space_id,
            created_by_user_id: row.created_by_user_id,
            title: row.title,
            note: row.note,
            category: row.category,
            priority: row.priority,
            plan_date: row.plan_date,
            done_by_user_id: row.done_by_user_id,
            done_at: row.done_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

struct AiWorkRow {
    id: i64,
    couple_space_id: i64,
    author_user_id: Option<UserId>,
    kind: i16,
    title: String,
    prompt: serde_json::Value,
    content: String,
    created_at: DateTime<Utc>,
}

impl TryFrom<AiWorkRow> for CoupleAiWork {
    type Error = Error;

    fn try_from(row: AiWorkRow) -> Result<Self> {
        Ok(Self {
            id: row.id,
            space_id: row.couple_space_id,
            author_user_id: row.author_user_id,
            kind: CoupleAiKind::from_i16(row.kind).ok_or_else(|| {
                Error::Internal(format!("stored couple AI kind is invalid: {}", row.kind))
            })?,
            title: row.title,
            prompt: row.prompt,
            content: row.content,
            created_at: row.created_at,
        })
    }
}

// ------------------------------------------------------------- 时光轴 ------

impl UserRepository {
    /// The timeline, newest remembered day first.
    pub async fn list_couple_memories(
        &self,
        space_id: i64,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<CoupleMemory>, i64)> {
        let total = sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "total!" FROM couple_memories WHERE couple_space_id = $1"#,
            space_id,
        )
        .fetch_one(self.pool())
        .await?;

        let rows = sqlx::query_as!(
            MemoryRow,
            r#"
            SELECT id,
                   couple_space_id,
                   author_user_id AS "author_user_id: UserId",
                   happened_on,
                   title,
                   content,
                   location,
                   created_at,
                   updated_at
            FROM couple_memories
            WHERE couple_space_id = $1
            ORDER BY happened_on DESC, id DESC
            LIMIT $2 OFFSET $3
            "#,
            space_id,
            limit,
            offset,
        )
        .fetch_all(self.pool())
        .await?;

        Ok((rows.into_iter().map(CoupleMemory::from).collect(), total))
    }

    pub async fn couple_memory(&self, memory_id: i64) -> Result<Option<CoupleMemory>> {
        let row = sqlx::query_as!(
            MemoryRow,
            r#"
            SELECT id,
                   couple_space_id,
                   author_user_id AS "author_user_id: UserId",
                   happened_on, title, content, location, created_at, updated_at
            FROM couple_memories
            WHERE id = $1
            "#,
            memory_id,
        )
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(CoupleMemory::from))
    }

    pub async fn create_couple_memory(
        &self,
        space_id: i64,
        author: &UserId,
        memory: &NewCoupleMemory,
    ) -> Result<CoupleMemory> {
        let row = sqlx::query_as!(
            MemoryRow,
            r#"
            INSERT INTO couple_memories
                (couple_space_id, author_user_id, happened_on, title, content, location)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id,
                      couple_space_id,
                      author_user_id AS "author_user_id: UserId",
                      happened_on, title, content, location, created_at, updated_at
            "#,
            space_id,
            author as &UserId,
            memory.happened_on,
            memory.title,
            memory.content,
            memory.location,
        )
        .fetch_one(self.pool())
        .await?;
        Ok(row.into())
    }

    pub async fn update_couple_memory(
        &self,
        memory_id: i64,
        update: &CoupleMemoryUpdate,
    ) -> Result<Option<CoupleMemory>> {
        let row = sqlx::query_as!(
            MemoryRow,
            r#"
            UPDATE couple_memories
            SET happened_on = COALESCE($2, happened_on),
                title = COALESCE($3, title),
                content = COALESCE($4, content),
                location = COALESCE($5, location),
                updated_at = CURRENT_TIMESTAMP
            WHERE id = $1
            RETURNING id,
                      couple_space_id,
                      author_user_id AS "author_user_id: UserId",
                      happened_on, title, content, location, created_at, updated_at
            "#,
            memory_id,
            update.happened_on,
            update.title.as_deref(),
            update.content.as_deref(),
            update.location.as_deref(),
        )
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(CoupleMemory::from))
    }

    /// Removes the entry. Its media rows cascade; the blobs are released by the
    /// caller, which owns the file-reference side.
    pub async fn delete_couple_memory(&self, memory_id: i64) -> Result<bool> {
        let result = sqlx::query!("DELETE FROM couple_memories WHERE id = $1", memory_id)
            .execute(self.pool())
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn count_couple_memories(&self, space_id: i64) -> Result<i64> {
        Ok(sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "total!" FROM couple_memories WHERE couple_space_id = $1"#,
            space_id,
        )
        .fetch_one(self.pool())
        .await?)
    }
}

// --------------------------------------------------------------- 相册 ------

impl UserRepository {
    /// Albums with their size and newest item, for the shelf view.
    pub async fn list_couple_albums(&self, space_id: i64) -> Result<Vec<CoupleAlbum>> {
        struct AlbumRow {
            id: i64,
            couple_space_id: i64,
            created_by_user_id: Option<UserId>,
            name: String,
            created_at: DateTime<Utc>,
            media_count: i64,
        }

        let rows = sqlx::query_as!(
            AlbumRow,
            r#"
            SELECT a.id,
                   a.couple_space_id,
                   a.created_by_user_id AS "created_by_user_id: UserId",
                   a.name,
                   a.created_at,
                   COUNT(m.id) AS "media_count!"
            FROM couple_albums a
            LEFT JOIN couple_media m ON m.album_id = a.id
            WHERE a.couple_space_id = $1
            GROUP BY a.id
            ORDER BY a.created_at DESC, a.id DESC
            "#,
            space_id,
        )
        .fetch_all(self.pool())
        .await?;

        let mut albums = Vec::with_capacity(rows.len());
        for row in rows {
            // One extra query per album rather than a lateral join: the shelf
            // holds a handful of albums, and the join that returns a whole
            // media row per group is far harder to read than this.
            let cover = self.newest_couple_media_in_album(row.id).await?;
            albums.push(CoupleAlbum {
                id: row.id,
                space_id: row.couple_space_id,
                created_by_user_id: row.created_by_user_id,
                name: row.name,
                created_at: row.created_at,
                media_count: row.media_count,
                cover,
            });
        }
        Ok(albums)
    }

    async fn newest_couple_media_in_album(&self, album_id: i64) -> Result<Option<CoupleMedia>> {
        let row = sqlx::query_as!(
            MediaRow,
            r#"
            SELECT id, couple_space_id, album_id, memory_id,
                   uploader_user_id AS "uploader_user_id: UserId",
                   media_kind, storage_backend, object_key, mime_type,
                   size_bytes, width, height, taken_on, caption, created_at
            FROM couple_media
            WHERE album_id = $1
            ORDER BY created_at DESC, id DESC
            LIMIT 1
            "#,
            album_id,
        )
        .fetch_optional(self.pool())
        .await?;
        row.map(CoupleMedia::try_from).transpose()
    }

    pub async fn create_couple_album(
        &self,
        space_id: i64,
        created_by: &UserId,
        name: &str,
    ) -> Result<CoupleAlbum> {
        let row = sqlx::query!(
            r#"
            INSERT INTO couple_albums (couple_space_id, created_by_user_id, name)
            VALUES ($1, $2, $3)
            RETURNING id, created_at
            "#,
            space_id,
            created_by as &UserId,
            name,
        )
        .fetch_one(self.pool())
        .await?;

        Ok(CoupleAlbum {
            id: row.id,
            space_id,
            created_by_user_id: Some(created_by.clone()),
            name: name.to_string(),
            created_at: row.created_at,
            media_count: 0,
            cover: None,
        })
    }

    pub async fn rename_couple_album(&self, album_id: i64, name: &str) -> Result<bool> {
        let result = sqlx::query!(
            "UPDATE couple_albums SET name = $2 WHERE id = $1",
            album_id,
            name,
        )
        .execute(self.pool())
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Removes the album. Its media survive as unfiled rather than being
    /// deleted — the photos are the point, the folder is not.
    pub async fn delete_couple_album(&self, album_id: i64) -> Result<bool> {
        let result = sqlx::query!("DELETE FROM couple_albums WHERE id = $1", album_id)
            .execute(self.pool())
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn couple_album_space(&self, album_id: i64) -> Result<Option<i64>> {
        Ok(sqlx::query_scalar!(
            "SELECT couple_space_id FROM couple_albums WHERE id = $1",
            album_id
        )
        .fetch_optional(self.pool())
        .await?)
    }
}

// --------------------------------------------------------------- 媒体 ------

impl UserRepository {
    pub async fn list_couple_media(
        &self,
        space_id: i64,
        scope: CoupleMediaScope,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<CoupleMedia>, i64)> {
        // The four scopes differ only in one predicate, and sqlx's compile-time
        // checking wants each shape spelled out, so the filter is passed as two
        // nullable parameters plus a flag rather than as string concatenation.
        let (album_id, memory_id, unfiled) = match scope {
            CoupleMediaScope::All => (None, None, false),
            CoupleMediaScope::Album(id) => (Some(id), None, false),
            CoupleMediaScope::Memory(id) => (None, Some(id), false),
            CoupleMediaScope::Unfiled => (None, None, true),
        };

        let total = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "total!"
            FROM couple_media
            WHERE couple_space_id = $1
              AND ($2::bigint IS NULL OR album_id = $2)
              AND ($3::bigint IS NULL OR memory_id = $3)
              AND (NOT $4::boolean OR album_id IS NULL)
            "#,
            space_id,
            album_id,
            memory_id,
            unfiled,
        )
        .fetch_one(self.pool())
        .await?;

        let rows = sqlx::query_as!(
            MediaRow,
            r#"
            SELECT id, couple_space_id, album_id, memory_id,
                   uploader_user_id AS "uploader_user_id: UserId",
                   media_kind, storage_backend, object_key, mime_type,
                   size_bytes, width, height, taken_on, caption, created_at
            FROM couple_media
            WHERE couple_space_id = $1
              AND ($2::bigint IS NULL OR album_id = $2)
              AND ($3::bigint IS NULL OR memory_id = $3)
              AND (NOT $4::boolean OR album_id IS NULL)
            ORDER BY created_at DESC, id DESC
            LIMIT $5 OFFSET $6
            "#,
            space_id,
            album_id,
            memory_id,
            unfiled,
            limit,
            offset,
        )
        .fetch_all(self.pool())
        .await?;

        let media = rows
            .into_iter()
            .map(CoupleMedia::try_from)
            .collect::<Result<Vec<_>>>()?;
        Ok((media, total))
    }

    /// Media attached to each of several memories, so a timeline page does not
    /// issue one query per entry.
    pub async fn couple_media_for_memories(&self, memory_ids: &[i64]) -> Result<Vec<CoupleMedia>> {
        if memory_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query_as!(
            MediaRow,
            r#"
            SELECT id, couple_space_id, album_id, memory_id,
                   uploader_user_id AS "uploader_user_id: UserId",
                   media_kind, storage_backend, object_key, mime_type,
                   size_bytes, width, height, taken_on, caption, created_at
            FROM couple_media
            WHERE memory_id = ANY($1)
            ORDER BY created_at ASC, id ASC
            "#,
            memory_ids,
        )
        .fetch_all(self.pool())
        .await?;
        rows.into_iter().map(CoupleMedia::try_from).collect()
    }

    pub async fn create_couple_media(
        &self,
        space_id: i64,
        uploader: &UserId,
        media: &NewCoupleMedia,
    ) -> Result<CoupleMedia> {
        let row = sqlx::query_as!(
            MediaRow,
            r#"
            INSERT INTO couple_media
                (couple_space_id, album_id, memory_id, uploader_user_id, media_kind,
                 storage_backend, object_key, mime_type, size_bytes, width, height,
                 taken_on, caption)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            RETURNING id, couple_space_id, album_id, memory_id,
                      uploader_user_id AS "uploader_user_id: UserId",
                      media_kind, storage_backend, object_key, mime_type,
                      size_bytes, width, height, taken_on, caption, created_at
            "#,
            space_id,
            media.album_id,
            media.memory_id,
            uploader as &UserId,
            media.kind.as_i16(),
            media.storage_backend,
            media.object_key,
            media.mime_type,
            media.size_bytes,
            media.width,
            media.height,
            media.taken_on,
            media.caption,
        )
        .fetch_one(self.pool())
        .await?;
        CoupleMedia::try_from(row)
    }

    pub async fn couple_media_by_id(&self, media_id: i64) -> Result<Option<CoupleMedia>> {
        let row = sqlx::query_as!(
            MediaRow,
            r#"
            SELECT id, couple_space_id, album_id, memory_id,
                   uploader_user_id AS "uploader_user_id: UserId",
                   media_kind, storage_backend, object_key, mime_type,
                   size_bytes, width, height, taken_on, caption, created_at
            FROM couple_media
            WHERE id = $1
            "#,
            media_id,
        )
        .fetch_optional(self.pool())
        .await?;
        row.map(CoupleMedia::try_from).transpose()
    }

    /// Moves media between albums. `album_id` of `None` unfiles it.
    pub async fn set_couple_media_album(
        &self,
        media_id: i64,
        album_id: Option<i64>,
    ) -> Result<bool> {
        let result = sqlx::query!(
            "UPDATE couple_media SET album_id = $2 WHERE id = $1",
            media_id,
            album_id,
        )
        .execute(self.pool())
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_couple_media(&self, media_id: i64) -> Result<bool> {
        let result = sqlx::query!("DELETE FROM couple_media WHERE id = $1", media_id)
            .execute(self.pool())
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn count_couple_media(&self, space_id: i64) -> Result<i64> {
        Ok(sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "total!" FROM couple_media WHERE couple_space_id = $1"#,
            space_id,
        )
        .fetch_one(self.pool())
        .await?)
    }

    /// The home cover, if one was chosen and still exists.
    pub async fn couple_space_cover(&self, space_id: i64) -> Result<Option<CoupleMedia>> {
        let row = sqlx::query_as!(
            MediaRow,
            r#"
            SELECT m.id, m.couple_space_id, m.album_id, m.memory_id,
                   m.uploader_user_id AS "uploader_user_id: UserId",
                   m.media_kind, m.storage_backend, m.object_key, m.mime_type,
                   m.size_bytes, m.width, m.height, m.taken_on, m.caption, m.created_at
            FROM couple_spaces s
            JOIN couple_media m ON m.id = s.cover_media_id
            WHERE s.id = $1
            "#,
            space_id,
        )
        .fetch_optional(self.pool())
        .await?;
        row.map(CoupleMedia::try_from).transpose()
    }

    pub async fn set_couple_space_cover(
        &self,
        space_id: i64,
        media_id: Option<i64>,
    ) -> Result<bool> {
        let result = sqlx::query!(
            "UPDATE couple_spaces SET cover_media_id = $2 WHERE id = $1",
            space_id,
            media_id,
        )
        .execute(self.pool())
        .await?;
        Ok(result.rows_affected() > 0)
    }
}

// ------------------------------------------------------------- 纪念日 ------

impl UserRepository {
    pub async fn list_couple_anniversaries(&self, space_id: i64) -> Result<Vec<CoupleAnniversary>> {
        let rows = sqlx::query_as!(
            AnniversaryRow,
            r#"
            SELECT id,
                   couple_space_id,
                   created_by_user_id AS "created_by_user_id: UserId",
                   title, happens_on, repeat_yearly, note, created_at, updated_at
            FROM couple_anniversaries
            WHERE couple_space_id = $1
            ORDER BY happens_on ASC, id ASC
            "#,
            space_id,
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(CoupleAnniversary::from).collect())
    }

    pub async fn create_couple_anniversary(
        &self,
        space_id: i64,
        created_by: &UserId,
        anniversary: &NewCoupleAnniversary,
    ) -> Result<CoupleAnniversary> {
        let row = sqlx::query_as!(
            AnniversaryRow,
            r#"
            INSERT INTO couple_anniversaries
                (couple_space_id, created_by_user_id, title, happens_on, repeat_yearly, note)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id,
                      couple_space_id,
                      created_by_user_id AS "created_by_user_id: UserId",
                      title, happens_on, repeat_yearly, note, created_at, updated_at
            "#,
            space_id,
            created_by as &UserId,
            anniversary.title,
            anniversary.happens_on,
            anniversary.repeat_yearly,
            anniversary.note,
        )
        .fetch_one(self.pool())
        .await?;
        Ok(row.into())
    }

    pub async fn update_couple_anniversary(
        &self,
        anniversary_id: i64,
        update: &CoupleAnniversaryUpdate,
    ) -> Result<Option<CoupleAnniversary>> {
        let row = sqlx::query_as!(
            AnniversaryRow,
            r#"
            UPDATE couple_anniversaries
            SET title = COALESCE($2, title),
                happens_on = COALESCE($3, happens_on),
                repeat_yearly = COALESCE($4, repeat_yearly),
                note = COALESCE($5, note),
                updated_at = CURRENT_TIMESTAMP
            WHERE id = $1
            RETURNING id,
                      couple_space_id,
                      created_by_user_id AS "created_by_user_id: UserId",
                      title, happens_on, repeat_yearly, note, created_at, updated_at
            "#,
            anniversary_id,
            update.title.as_deref(),
            update.happens_on,
            update.repeat_yearly,
            update.note.as_deref(),
        )
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(CoupleAnniversary::from))
    }

    pub async fn delete_couple_anniversary(&self, anniversary_id: i64) -> Result<bool> {
        let result = sqlx::query!(
            "DELETE FROM couple_anniversaries WHERE id = $1",
            anniversary_id,
        )
        .execute(self.pool())
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn couple_anniversary_space(&self, anniversary_id: i64) -> Result<Option<i64>> {
        Ok(sqlx::query_scalar!(
            "SELECT couple_space_id FROM couple_anniversaries WHERE id = $1",
            anniversary_id,
        )
        .fetch_optional(self.pool())
        .await?)
    }
}

// --------------------------------------------------------------- 清单 ------

impl UserRepository {
    /// Outstanding first, then by planned date, then by age.
    pub async fn list_couple_todos(&self, space_id: i64) -> Result<Vec<CoupleTodo>> {
        let rows = sqlx::query_as!(
            TodoRow,
            r#"
            SELECT id,
                   couple_space_id,
                   created_by_user_id AS "created_by_user_id: UserId",
                   title, note, category, priority, plan_date,
                   done_by_user_id AS "done_by_user_id: UserId",
                   done_at, created_at, updated_at
            FROM couple_todos
            WHERE couple_space_id = $1
            ORDER BY (done_at IS NOT NULL), priority DESC, plan_date ASC NULLS LAST, id ASC
            "#,
            space_id,
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(CoupleTodo::from).collect())
    }

    pub async fn create_couple_todo(
        &self,
        space_id: i64,
        created_by: &UserId,
        todo: &NewCoupleTodo,
    ) -> Result<CoupleTodo> {
        let row = sqlx::query_as!(
            TodoRow,
            r#"
            INSERT INTO couple_todos
                (couple_space_id, created_by_user_id, title, note, category, priority, plan_date)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id,
                      couple_space_id,
                      created_by_user_id AS "created_by_user_id: UserId",
                      title, note, category, priority, plan_date,
                      done_by_user_id AS "done_by_user_id: UserId",
                      done_at, created_at, updated_at
            "#,
            space_id,
            created_by as &UserId,
            todo.title,
            todo.note,
            todo.category,
            todo.priority,
            todo.plan_date,
        )
        .fetch_one(self.pool())
        .await?;
        Ok(row.into())
    }

    /// `clear_plan_date` is the flag an `Option<Option<_>>` collapses to: the
    /// COALESCE below cannot tell "leave it" from "clear it" on its own.
    pub async fn update_couple_todo(
        &self,
        todo_id: i64,
        update: &CoupleTodoUpdate,
        actor: &UserId,
    ) -> Result<Option<CoupleTodo>> {
        let clear_plan_date = matches!(update.plan_date, Some(None));
        let plan_date = update.plan_date.flatten();
        // Three states: tick it in the actor's name, clear it, or leave it.
        let (set_done, clear_done) = match update.done {
            Some(true) => (true, false),
            Some(false) => (false, true),
            None => (false, false),
        };

        let row = sqlx::query_as!(
            TodoRow,
            r#"
            UPDATE couple_todos
            SET title = COALESCE($2, title),
                note = COALESCE($3, note),
                category = COALESCE($4, category),
                priority = COALESCE($5, priority),
                plan_date = CASE WHEN $6 THEN NULL ELSE COALESCE($7, plan_date) END,
                done_by_user_id = CASE
                    WHEN $8 THEN $10
                    WHEN $9 THEN NULL
                    ELSE done_by_user_id
                END,
                done_at = CASE
                    WHEN $8 THEN CURRENT_TIMESTAMP
                    WHEN $9 THEN NULL
                    ELSE done_at
                END,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = $1
            RETURNING id,
                      couple_space_id,
                      created_by_user_id AS "created_by_user_id: UserId",
                      title, note, category, priority, plan_date,
                      done_by_user_id AS "done_by_user_id: UserId",
                      done_at, created_at, updated_at
            "#,
            todo_id,
            update.title.as_deref(),
            update.note.as_deref(),
            update.category.as_deref(),
            update.priority,
            clear_plan_date,
            plan_date,
            set_done,
            clear_done,
            actor as &UserId,
        )
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(CoupleTodo::from))
    }

    pub async fn delete_couple_todo(&self, todo_id: i64) -> Result<bool> {
        let result = sqlx::query!("DELETE FROM couple_todos WHERE id = $1", todo_id)
            .execute(self.pool())
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn couple_todo_space(&self, todo_id: i64) -> Result<Option<i64>> {
        Ok(sqlx::query_scalar!(
            "SELECT couple_space_id FROM couple_todos WHERE id = $1",
            todo_id
        )
        .fetch_optional(self.pool())
        .await?)
    }

    /// Outstanding and completed counts, for the home progress bar.
    pub async fn couple_todo_counts(&self, space_id: i64) -> Result<(i64, i64)> {
        let row = sqlx::query!(
            r#"
            SELECT COUNT(*) FILTER (WHERE done_at IS NULL) AS "open!",
                   COUNT(*) FILTER (WHERE done_at IS NOT NULL) AS "done!"
            FROM couple_todos
            WHERE couple_space_id = $1
            "#,
            space_id,
        )
        .fetch_one(self.pool())
        .await?;
        Ok((row.open, row.done))
    }
}

// ------------------------------------------------------------ 宠物小屋 ------

impl UserRepository {
    pub async fn couple_pet(&self, space_id: i64) -> Result<Option<CouplePet>> {
        let row = sqlx::query_as!(
            CouplePet,
            r#"
            SELECT couple_space_id AS space_id,
                   name, species, skin, accessory, stage, experience,
                   hunger, mood, settled_at, created_at, updated_at
            FROM couple_pets
            WHERE couple_space_id = $1
            "#,
            space_id,
        )
        .fetch_optional(self.pool())
        .await?;
        Ok(row)
    }

    /// Adopts the space's pet, or returns the one already there.
    ///
    /// `ON CONFLICT DO UPDATE` rather than `DO NOTHING` so the row comes back
    /// either way: two members opening 宠物小屋 at once must not race into one
    /// of them getting nothing.
    pub async fn adopt_couple_pet(
        &self,
        space_id: i64,
        name: &str,
        species: i16,
    ) -> Result<CouplePet> {
        let row = sqlx::query_as!(
            CouplePet,
            r#"
            INSERT INTO couple_pets (couple_space_id, name, species)
            VALUES ($1, $2, $3)
            ON CONFLICT (couple_space_id) DO UPDATE
                SET updated_at = couple_pets.updated_at
            RETURNING couple_space_id AS space_id,
                      name, species, skin, accessory, stage, experience,
                      hunger, mood, settled_at, created_at, updated_at
            "#,
            space_id,
            name,
            species,
        )
        .fetch_one(self.pool())
        .await?;
        Ok(row)
    }

    /// Writes the post-interaction state. Hunger and mood are stored together
    /// with the instant they were true, which is what lets the reader decay
    /// them without a background job.
    pub async fn settle_couple_pet(
        &self,
        space_id: i64,
        hunger: i16,
        mood: i16,
        experience: i32,
        stage: i16,
    ) -> Result<Option<CouplePet>> {
        let row = sqlx::query_as!(
            CouplePet,
            r#"
            UPDATE couple_pets
            SET hunger = $2,
                mood = $3,
                experience = $4,
                stage = $5,
                settled_at = CURRENT_TIMESTAMP,
                updated_at = CURRENT_TIMESTAMP
            WHERE couple_space_id = $1
            RETURNING couple_space_id AS space_id,
                      name, species, skin, accessory, stage, experience,
                      hunger, mood, settled_at, created_at, updated_at
            "#,
            space_id,
            hunger,
            mood,
            experience,
            stage,
        )
        .fetch_optional(self.pool())
        .await?;
        Ok(row)
    }

    pub async fn update_couple_pet(
        &self,
        space_id: i64,
        update: &CouplePetUpdate,
    ) -> Result<Option<CouplePet>> {
        let row = sqlx::query_as!(
            CouplePet,
            r#"
            UPDATE couple_pets
            SET name = COALESCE($2, name),
                species = COALESCE($3, species),
                skin = COALESCE($4, skin),
                accessory = COALESCE($5, accessory),
                updated_at = CURRENT_TIMESTAMP
            WHERE couple_space_id = $1
            RETURNING couple_space_id AS space_id,
                      name, species, skin, accessory, stage, experience,
                      hunger, mood, settled_at, created_at, updated_at
            "#,
            space_id,
            update.name.as_deref(),
            update.species,
            update.skin,
            update.accessory,
        )
        .fetch_optional(self.pool())
        .await?;
        Ok(row)
    }

    /// Records that this member was in today. Idempotent per day; the counter
    /// is what distinguishes "said hello" from "played all evening".
    pub async fn record_couple_pet_checkin(
        &self,
        space_id: i64,
        user_id: &UserId,
        on_date: NaiveDate,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO couple_pet_checkins (couple_space_id, user_id, on_date)
            VALUES ($1, $2, $3)
            ON CONFLICT (couple_space_id, user_id, on_date) DO UPDATE
                SET interactions = couple_pet_checkins.interactions + 1
            "#,
            space_id,
            user_id as &UserId,
            on_date,
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// One row per day either member was in, newest first.
    pub async fn couple_pet_checkin_days(
        &self,
        space_id: i64,
        limit: i64,
    ) -> Result<Vec<CouplePetCheckinDay>> {
        let rows = sqlx::query_as!(
            CouplePetCheckinDay,
            r#"
            SELECT on_date,
                   COUNT(DISTINCT user_id) AS "members!",
                   SUM(interactions)::bigint AS "interactions!"
            FROM couple_pet_checkins
            WHERE couple_space_id = $1
            GROUP BY on_date
            ORDER BY on_date DESC
            LIMIT $2
            "#,
            space_id,
            limit,
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }
}

// ------------------------------------------------------------- 心动 AI ------

impl UserRepository {
    pub async fn list_couple_ai_works(
        &self,
        space_id: i64,
        kind: Option<CoupleAiKind>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<CoupleAiWork>, i64)> {
        let kind = kind.map(CoupleAiKind::as_i16);

        let total = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "total!"
            FROM couple_ai_works
            WHERE couple_space_id = $1 AND ($2::smallint IS NULL OR kind = $2)
            "#,
            space_id,
            kind,
        )
        .fetch_one(self.pool())
        .await?;

        let rows = sqlx::query_as!(
            AiWorkRow,
            r#"
            SELECT id,
                   couple_space_id,
                   author_user_id AS "author_user_id: UserId",
                   kind, title, prompt, content, created_at
            FROM couple_ai_works
            WHERE couple_space_id = $1 AND ($2::smallint IS NULL OR kind = $2)
            ORDER BY created_at DESC, id DESC
            LIMIT $3 OFFSET $4
            "#,
            space_id,
            kind,
            limit,
            offset,
        )
        .fetch_all(self.pool())
        .await?;

        let works = rows
            .into_iter()
            .map(CoupleAiWork::try_from)
            .collect::<Result<Vec<_>>>()?;
        Ok((works, total))
    }

    pub async fn create_couple_ai_work(
        &self,
        space_id: i64,
        author: &UserId,
        work: &NewCoupleAiWork,
    ) -> Result<CoupleAiWork> {
        let row = sqlx::query_as!(
            AiWorkRow,
            r#"
            INSERT INTO couple_ai_works
                (couple_space_id, author_user_id, kind, title, prompt, content)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id,
                      couple_space_id,
                      author_user_id AS "author_user_id: UserId",
                      kind, title, prompt, content, created_at
            "#,
            space_id,
            author as &UserId,
            work.kind.as_i16(),
            work.title,
            work.prompt,
            work.content,
        )
        .fetch_one(self.pool())
        .await?;
        CoupleAiWork::try_from(row)
    }

    pub async fn delete_couple_ai_work(&self, work_id: i64) -> Result<bool> {
        let result = sqlx::query!("DELETE FROM couple_ai_works WHERE id = $1", work_id)
            .execute(self.pool())
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn couple_ai_work_space(&self, work_id: i64) -> Result<Option<i64>> {
        Ok(sqlx::query_scalar!(
            "SELECT couple_space_id FROM couple_ai_works WHERE id = $1",
            work_id,
        )
        .fetch_optional(self.pool())
        .await?)
    }
}
