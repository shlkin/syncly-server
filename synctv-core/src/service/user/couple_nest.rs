//! 爱的小窝: the rules on top of the nest's storage.
//!
//! Two things live here rather than in the repository. Authorisation, because
//! it is the same question everywhere — "is the caller in this space" — and
//! answering it once is what keeps the thirty queries below free of viewer
//! predicates. And the pet, because its numbers are a simulation: hunger and
//! mood are stored as of an instant and decay from there, so "what is the pet
//! like now" is computed, never read.

use chrono::{DateTime, Duration, NaiveDate, Utc};

use crate::{
    models::{
        CoupleAiKind, CoupleAiWork, CoupleAlbum, CoupleAnniversary, CoupleAnniversaryUpdate,
        CoupleMedia, CoupleMediaScope, CoupleMemory, CoupleMemoryUpdate, CoupleNestHome,
        CouplePetAction, CouplePetCheckinDay, CouplePetUpdate, CouplePetView, CoupleTodo,
        CoupleTodoUpdate, NewCoupleAiWork, NewCoupleAnniversary, NewCoupleMedia, NewCoupleMemory,
        NewCoupleTodo, PageParams, UserId,
    },
    Error, Result,
};

use super::UserService;

// Bounds, all matching the column widths so an over-long value is refused with
// a sentence rather than as a database error.
const MEMORY_TITLE_MAX: usize = 120;
const MEMORY_CONTENT_MAX: usize = 8000;
const LOCATION_MAX: usize = 200;
const ALBUM_NAME_MAX: usize = 80;
const CAPTION_MAX: usize = 200;
const ANNIVERSARY_TITLE_MAX: usize = 80;
const NOTE_MAX: usize = 200;
const TODO_TITLE_MAX: usize = 120;
const TODO_NOTE_MAX: usize = 300;
const TODO_CATEGORY_MAX: usize = 40;
const PET_NAME_MAX: usize = 40;
const AI_TITLE_MAX: usize = 120;
const AI_CONTENT_MAX: usize = 20000;

/// Home shows a strip, not the whole library.
const HOME_MEDIA_LIMIT: i64 = 10;
const HOME_TODO_LIMIT: i64 = 5;
/// A year of squares is as much history as the streak panel can show.
const STREAK_HISTORY_DAYS: i64 = 366;

// ------------------------------------------------------------ pet rules ----

/// Hunger lost per hour left alone. A day of neglect costs 24 of 100, so a pet
/// visited every couple of days stays comfortable and one abandoned for a week
/// is visibly unhappy.
const HUNGER_DECAY_PER_HOUR: f64 = 1.0;
/// Mood falls at half that: loneliness is slower than hunger.
const MOOD_DECAY_PER_HOUR: f64 = 0.5;

/// Cumulative experience needed to reach stage 2, 3 and 4.
const STAGE_THRESHOLDS: [i32; 3] = [100, 300, 700];

impl CouplePetAction {
    const fn effect(self) -> (i16, i16, i32) {
        // (hunger, mood, experience)
        match self {
            Self::Feed => (30, 5, 2),
            Self::Play => (5, 25, 3),
            Self::Pet => (0, 10, 1),
        }
    }
}

fn decay(value: i16, per_hour: f64, since: DateTime<Utc>, now: DateTime<Utc>) -> i16 {
    let hours = (now - since).num_seconds().max(0) as f64 / 3600.0;
    let lost = (hours * per_hour).floor();
    // The cast is safe: `value` is 0..=100 and `lost` is non-negative, so the
    // difference is bounded by i16 either way.
    let remaining = f64::from(value) - lost;
    remaining.clamp(0.0, 100.0) as i16
}

/// Which stage the pet has earned, from its total experience.
fn stage_for(experience: i32) -> i16 {
    let mut stage = 1;
    for threshold in STAGE_THRESHOLDS {
        if experience >= threshold {
            stage += 1;
        }
    }
    stage
}

/// Progress within the current stage, and what the next one costs.
fn stage_progress(experience: i32) -> (i32, i32) {
    let stage = stage_for(experience);
    let floor = if stage >= 2 {
        STAGE_THRESHOLDS[stage as usize - 2]
    } else {
        0
    };
    let ceiling = STAGE_THRESHOLDS.get(stage as usize - 1).copied();
    match ceiling {
        Some(ceiling) => (experience - floor, ceiling - floor),
        // Fully grown: the bar is full rather than empty.
        None => (1, 1),
    }
}

/// The longest run of consecutive days ending today, or ending yesterday when
/// nobody has been in yet today.
///
/// Ending yesterday still counts: a streak should not look broken at breakfast
/// merely because the day is young.
fn streak_from(days: &[CouplePetCheckinDay], today: NaiveDate) -> i64 {
    let mut cursor = match days.first() {
        Some(first) if first.on_date == today => today,
        Some(first) if first.on_date == today - Duration::days(1) => today - Duration::days(1),
        _ => return 0,
    };
    let mut streak = 0;
    for day in days {
        if day.on_date == cursor {
            streak += 1;
            cursor -= Duration::days(1);
        } else if day.on_date < cursor {
            break;
        }
    }
    streak
}

// ---------------------------------------------------------- validation -----

fn trimmed(value: &str, max: usize, what: &str) -> Result<String> {
    let value = value.trim();
    if value.chars().count() > max {
        return Err(Error::InvalidInput(format!(
            "{what} may not exceed {max} characters"
        )));
    }
    Ok(value.to_string())
}

fn required(value: &str, max: usize, what: &str) -> Result<String> {
    let value = trimmed(value, max, what)?;
    if value.is_empty() {
        return Err(Error::InvalidInput(format!("{what} is required")));
    }
    Ok(value)
}

impl UserService {
    // ------------------------------------------------------- 时光轴 ------

    pub async fn list_couple_memories(
        &self,
        space_id: i64,
        viewer: &UserId,
        pagination: PageParams,
    ) -> Result<(Vec<CoupleMemory>, Vec<CoupleMedia>, i64)> {
        self.require_space_member(space_id, viewer).await?;
        let (memories, total) = self
            .repository
            .list_couple_memories(space_id, pagination.limit_i64()?, pagination.offset_i64()?)
            .await?;
        // One extra query for the whole page rather than one per entry.
        let ids: Vec<i64> = memories.iter().map(|memory| memory.id).collect();
        let media = self.repository.couple_media_for_memories(&ids).await?;
        Ok((memories, media, total))
    }

    pub async fn create_couple_memory(
        &self,
        space_id: i64,
        author: &UserId,
        happened_on: NaiveDate,
        title: &str,
        content: &str,
        location: &str,
    ) -> Result<CoupleMemory> {
        self.require_active_space(space_id, author).await?;
        let memory = NewCoupleMemory {
            happened_on,
            title: trimmed(title, MEMORY_TITLE_MAX, "Title")?,
            content: trimmed(content, MEMORY_CONTENT_MAX, "Content")?,
            location: trimmed(location, LOCATION_MAX, "Location")?,
        };
        if memory.title.is_empty() && memory.content.is_empty() {
            return Err(Error::InvalidInput(
                "A memory needs a title or something written in it".to_string(),
            ));
        }
        self.repository
            .create_couple_memory(space_id, author, &memory)
            .await
    }

    pub async fn update_couple_memory(
        &self,
        memory_id: i64,
        actor: &UserId,
        update: CoupleMemoryUpdate,
    ) -> Result<CoupleMemory> {
        let memory = self.require_memory(memory_id, actor).await?;
        self.require_active_space(memory.space_id, actor).await?;
        let update = CoupleMemoryUpdate {
            title: update
                .title
                .map(|value| trimmed(&value, MEMORY_TITLE_MAX, "Title"))
                .transpose()?,
            content: update
                .content
                .map(|value| trimmed(&value, MEMORY_CONTENT_MAX, "Content"))
                .transpose()?,
            location: update
                .location
                .map(|value| trimmed(&value, LOCATION_MAX, "Location"))
                .transpose()?,
            ..update
        };
        self.repository
            .update_couple_memory(memory_id, &update)
            .await?
            .ok_or_else(memory_not_found)
    }

    pub async fn delete_couple_memory(&self, memory_id: i64, actor: &UserId) -> Result<()> {
        let memory = self.require_memory(memory_id, actor).await?;
        self.require_active_space(memory.space_id, actor).await?;
        if self.repository.delete_couple_memory(memory_id).await? {
            return Ok(());
        }
        Err(memory_not_found())
    }

    async fn require_memory(&self, memory_id: i64, viewer: &UserId) -> Result<CoupleMemory> {
        let memory = self
            .repository
            .couple_memory(memory_id)
            .await?
            .ok_or_else(memory_not_found)?;
        // Membership decides visibility, so a non-member is told it does not
        // exist rather than that it is not theirs.
        self.require_space_member(memory.space_id, viewer)
            .await
            .map_err(|_| memory_not_found())?;
        Ok(memory)
    }

    // --------------------------------------------------------- 相册 ------

    pub async fn list_couple_albums(
        &self,
        space_id: i64,
        viewer: &UserId,
    ) -> Result<Vec<CoupleAlbum>> {
        self.require_space_member(space_id, viewer).await?;
        self.repository.list_couple_albums(space_id).await
    }

    pub async fn create_couple_album(
        &self,
        space_id: i64,
        actor: &UserId,
        name: &str,
    ) -> Result<CoupleAlbum> {
        self.require_active_space(space_id, actor).await?;
        let name = required(name, ALBUM_NAME_MAX, "Album name")?;
        self.repository
            .create_couple_album(space_id, actor, &name)
            .await
    }

    pub async fn rename_couple_album(
        &self,
        album_id: i64,
        actor: &UserId,
        name: &str,
    ) -> Result<()> {
        let space_id = self.require_album_space(album_id, actor).await?;
        self.require_active_space(space_id, actor).await?;
        let name = required(name, ALBUM_NAME_MAX, "Album name")?;
        if self.repository.rename_couple_album(album_id, &name).await? {
            return Ok(());
        }
        Err(album_not_found())
    }

    /// Removes the folder. The photos in it survive as unfiled: the pictures
    /// are the point, the folder is not, and a delete that silently took a
    /// hundred photos with it would be unforgivable here.
    pub async fn delete_couple_album(&self, album_id: i64, actor: &UserId) -> Result<()> {
        let space_id = self.require_album_space(album_id, actor).await?;
        self.require_active_space(space_id, actor).await?;
        if self.repository.delete_couple_album(album_id).await? {
            return Ok(());
        }
        Err(album_not_found())
    }

    async fn require_album_space(&self, album_id: i64, viewer: &UserId) -> Result<i64> {
        let space_id = self
            .repository
            .couple_album_space(album_id)
            .await?
            .ok_or_else(album_not_found)?;
        self.require_space_member(space_id, viewer)
            .await
            .map_err(|_| album_not_found())?;
        Ok(space_id)
    }

    // --------------------------------------------------------- 媒体 ------

    pub async fn list_couple_media(
        &self,
        space_id: i64,
        viewer: &UserId,
        scope: CoupleMediaScope,
        pagination: PageParams,
    ) -> Result<(Vec<CoupleMedia>, i64)> {
        self.require_space_member(space_id, viewer).await?;
        self.repository
            .list_couple_media(
                space_id,
                scope,
                pagination.limit_i64()?,
                pagination.offset_i64()?,
            )
            .await
    }

    /// Files an already-uploaded object into the nest.
    ///
    /// The upload itself goes through the file-storage endpoints, exactly as an
    /// avatar or a chat attachment does; this only records what the object is
    /// and where it belongs.
    pub async fn add_couple_media(
        &self,
        space_id: i64,
        uploader: &UserId,
        mut media: NewCoupleMedia,
    ) -> Result<CoupleMedia> {
        self.require_active_space(space_id, uploader).await?;
        media.caption = trimmed(&media.caption, CAPTION_MAX, "Caption")?;
        if media.object_key.trim().is_empty() {
            return Err(Error::InvalidInput("An object key is required".to_string()));
        }
        // Both are optional, but a wrong one would file a photo into another
        // couple's album, so each is checked against this space.
        if let Some(album_id) = media.album_id {
            let owner = self
                .repository
                .couple_album_space(album_id)
                .await?
                .ok_or_else(album_not_found)?;
            if owner != space_id {
                return Err(album_not_found());
            }
        }
        if let Some(memory_id) = media.memory_id {
            let memory = self
                .repository
                .couple_memory(memory_id)
                .await?
                .ok_or_else(memory_not_found)?;
            if memory.space_id != space_id {
                return Err(memory_not_found());
            }
        }
        self.repository
            .create_couple_media(space_id, uploader, &media)
            .await
    }

    pub async fn move_couple_media(
        &self,
        media_id: i64,
        actor: &UserId,
        album_id: Option<i64>,
    ) -> Result<()> {
        let media = self.require_media(media_id, actor).await?;
        self.require_active_space(media.space_id, actor).await?;
        if let Some(album_id) = album_id {
            let owner = self
                .repository
                .couple_album_space(album_id)
                .await?
                .ok_or_else(album_not_found)?;
            if owner != media.space_id {
                return Err(album_not_found());
            }
        }
        if self
            .repository
            .set_couple_media_album(media_id, album_id)
            .await?
        {
            return Ok(());
        }
        Err(media_not_found())
    }

    pub async fn delete_couple_media(&self, media_id: i64, actor: &UserId) -> Result<CoupleMedia> {
        let media = self.require_media(media_id, actor).await?;
        self.require_active_space(media.space_id, actor).await?;
        if self.repository.delete_couple_media(media_id).await? {
            // Returned so the caller can release the blob it was holding.
            return Ok(media);
        }
        Err(media_not_found())
    }

    /// Picks the home cover, or clears it when `media_id` is `None`.
    pub async fn set_couple_space_cover(
        &self,
        space_id: i64,
        actor: &UserId,
        media_id: Option<i64>,
    ) -> Result<()> {
        self.require_active_space(space_id, actor).await?;
        if let Some(media_id) = media_id {
            let media = self
                .repository
                .couple_media_by_id(media_id)
                .await?
                .ok_or_else(media_not_found)?;
            if media.space_id != space_id {
                return Err(media_not_found());
            }
        }
        self.repository
            .set_couple_space_cover(space_id, media_id)
            .await?;
        Ok(())
    }

    /// One item, for the route that streams its bytes back. Membership-checked
    /// like everything else here, so a link to a photo is useless to anyone
    /// outside the space it belongs to.
    pub async fn couple_media(&self, media_id: i64, viewer: &UserId) -> Result<CoupleMedia> {
        self.require_media(media_id, viewer).await
    }

    async fn require_media(&self, media_id: i64, viewer: &UserId) -> Result<CoupleMedia> {
        let media = self
            .repository
            .couple_media_by_id(media_id)
            .await?
            .ok_or_else(media_not_found)?;
        self.require_space_member(media.space_id, viewer)
            .await
            .map_err(|_| media_not_found())?;
        Ok(media)
    }

    // ------------------------------------------------------- 纪念日 ------

    pub async fn list_couple_anniversaries(
        &self,
        space_id: i64,
        viewer: &UserId,
    ) -> Result<Vec<CoupleAnniversary>> {
        self.require_space_member(space_id, viewer).await?;
        self.repository.list_couple_anniversaries(space_id).await
    }

    pub async fn create_couple_anniversary(
        &self,
        space_id: i64,
        actor: &UserId,
        title: &str,
        happens_on: NaiveDate,
        repeat_yearly: bool,
        note: &str,
    ) -> Result<CoupleAnniversary> {
        self.require_active_space(space_id, actor).await?;
        let anniversary = NewCoupleAnniversary {
            title: required(title, ANNIVERSARY_TITLE_MAX, "Title")?,
            happens_on,
            repeat_yearly,
            note: trimmed(note, NOTE_MAX, "Note")?,
        };
        self.repository
            .create_couple_anniversary(space_id, actor, &anniversary)
            .await
    }

    pub async fn update_couple_anniversary(
        &self,
        anniversary_id: i64,
        actor: &UserId,
        update: CoupleAnniversaryUpdate,
    ) -> Result<CoupleAnniversary> {
        let space_id = self
            .require_anniversary_space(anniversary_id, actor)
            .await?;
        self.require_active_space(space_id, actor).await?;
        let update = CoupleAnniversaryUpdate {
            title: update
                .title
                .map(|value| required(&value, ANNIVERSARY_TITLE_MAX, "Title"))
                .transpose()?,
            note: update
                .note
                .map(|value| trimmed(&value, NOTE_MAX, "Note"))
                .transpose()?,
            ..update
        };
        self.repository
            .update_couple_anniversary(anniversary_id, &update)
            .await?
            .ok_or_else(anniversary_not_found)
    }

    pub async fn delete_couple_anniversary(
        &self,
        anniversary_id: i64,
        actor: &UserId,
    ) -> Result<()> {
        let space_id = self
            .require_anniversary_space(anniversary_id, actor)
            .await?;
        self.require_active_space(space_id, actor).await?;
        if self
            .repository
            .delete_couple_anniversary(anniversary_id)
            .await?
        {
            return Ok(());
        }
        Err(anniversary_not_found())
    }

    async fn require_anniversary_space(&self, anniversary_id: i64, viewer: &UserId) -> Result<i64> {
        let space_id = self
            .repository
            .couple_anniversary_space(anniversary_id)
            .await?
            .ok_or_else(anniversary_not_found)?;
        self.require_space_member(space_id, viewer)
            .await
            .map_err(|_| anniversary_not_found())?;
        Ok(space_id)
    }

    // --------------------------------------------------------- 清单 ------

    pub async fn list_couple_todos(
        &self,
        space_id: i64,
        viewer: &UserId,
    ) -> Result<Vec<CoupleTodo>> {
        self.require_space_member(space_id, viewer).await?;
        self.repository.list_couple_todos(space_id).await
    }

    pub async fn create_couple_todo(
        &self,
        space_id: i64,
        actor: &UserId,
        title: &str,
        note: &str,
        category: &str,
        priority: i16,
        plan_date: Option<NaiveDate>,
    ) -> Result<CoupleTodo> {
        self.require_active_space(space_id, actor).await?;
        let todo = NewCoupleTodo {
            title: required(title, TODO_TITLE_MAX, "Title")?,
            note: trimmed(note, TODO_NOTE_MAX, "Note")?,
            category: trimmed(category, TODO_CATEGORY_MAX, "Category")?,
            priority: priority.clamp(0, 2),
            plan_date,
        };
        self.repository
            .create_couple_todo(space_id, actor, &todo)
            .await
    }

    pub async fn update_couple_todo(
        &self,
        todo_id: i64,
        actor: &UserId,
        update: CoupleTodoUpdate,
    ) -> Result<CoupleTodo> {
        let space_id = self.require_todo_space(todo_id, actor).await?;
        self.require_active_space(space_id, actor).await?;
        let update = CoupleTodoUpdate {
            title: update
                .title
                .map(|value| required(&value, TODO_TITLE_MAX, "Title"))
                .transpose()?,
            note: update
                .note
                .map(|value| trimmed(&value, TODO_NOTE_MAX, "Note"))
                .transpose()?,
            category: update
                .category
                .map(|value| trimmed(&value, TODO_CATEGORY_MAX, "Category"))
                .transpose()?,
            priority: update.priority.map(|value| value.clamp(0, 2)),
            ..update
        };
        self.repository
            .update_couple_todo(todo_id, &update, actor)
            .await?
            .ok_or_else(todo_not_found)
    }

    pub async fn delete_couple_todo(&self, todo_id: i64, actor: &UserId) -> Result<()> {
        let space_id = self.require_todo_space(todo_id, actor).await?;
        self.require_active_space(space_id, actor).await?;
        if self.repository.delete_couple_todo(todo_id).await? {
            return Ok(());
        }
        Err(todo_not_found())
    }

    async fn require_todo_space(&self, todo_id: i64, viewer: &UserId) -> Result<i64> {
        let space_id = self
            .repository
            .couple_todo_space(todo_id)
            .await?
            .ok_or_else(todo_not_found)?;
        self.require_space_member(space_id, viewer)
            .await
            .map_err(|_| todo_not_found())?;
        Ok(space_id)
    }

    // ------------------------------------------------------ 宠物小屋 ------

    /// The pet as it is now: stored values decayed to this moment, plus the
    /// streak its check-ins imply. `None` until the pair adopts one.
    pub async fn couple_pet(
        &self,
        space_id: i64,
        viewer: &UserId,
    ) -> Result<Option<CouplePetView>> {
        self.require_space_member(space_id, viewer).await?;
        self.pet_view(space_id).await
    }

    pub async fn adopt_couple_pet(
        &self,
        space_id: i64,
        actor: &UserId,
        name: &str,
        species: i16,
    ) -> Result<CouplePetView> {
        self.require_active_space(space_id, actor).await?;
        let name = trimmed(name, PET_NAME_MAX, "Pet name")?;
        self.repository
            .adopt_couple_pet(space_id, &name, species.clamp(1, 4))
            .await?;
        self.pet_view(space_id)
            .await?
            .ok_or_else(|| Error::Internal("the pet vanished after adoption".to_string()))
    }

    /// Feeds, plays with or strokes the pet, and records that this member was
    /// in today.
    ///
    /// The effect is applied to the *decayed* values, not the stored ones —
    /// feeding a pet that has been alone for a week must not restore it from
    /// the number it had a week ago.
    pub async fn interact_with_couple_pet(
        &self,
        space_id: i64,
        actor: &UserId,
        action: CouplePetAction,
    ) -> Result<CouplePetView> {
        self.require_active_space(space_id, actor).await?;
        let pet = self
            .repository
            .couple_pet(space_id)
            .await?
            .ok_or_else(pet_not_found)?;

        let now = Utc::now();
        let (hunger_gain, mood_gain, experience_gain) = action.effect();
        let hunger =
            (decay(pet.hunger, HUNGER_DECAY_PER_HOUR, pet.settled_at, now) + hunger_gain).min(100);
        let mood = (decay(pet.mood, MOOD_DECAY_PER_HOUR, pet.settled_at, now) + mood_gain).min(100);
        let experience = pet.experience.saturating_add(experience_gain);

        self.repository
            .settle_couple_pet(space_id, hunger, mood, experience, stage_for(experience))
            .await?;
        self.repository
            .record_couple_pet_checkin(space_id, actor, now.date_naive())
            .await?;

        self.pet_view(space_id)
            .await?
            .ok_or_else(|| Error::Internal("the pet vanished mid-interaction".to_string()))
    }

    pub async fn update_couple_pet(
        &self,
        space_id: i64,
        actor: &UserId,
        update: CouplePetUpdate,
    ) -> Result<CouplePetView> {
        self.require_active_space(space_id, actor).await?;
        let update = CouplePetUpdate {
            name: update
                .name
                .map(|value| trimmed(&value, PET_NAME_MAX, "Pet name"))
                .transpose()?,
            species: update.species.map(|value| value.clamp(1, 4)),
            ..update
        };
        self.repository
            .update_couple_pet(space_id, &update)
            .await?
            .ok_or_else(pet_not_found)?;
        self.pet_view(space_id)
            .await?
            .ok_or_else(|| Error::Internal("the pet vanished mid-update".to_string()))
    }

    /// The streak panel: one entry per day either member was in.
    pub async fn couple_pet_history(
        &self,
        space_id: i64,
        viewer: &UserId,
    ) -> Result<Vec<CouplePetCheckinDay>> {
        self.require_space_member(space_id, viewer).await?;
        self.repository
            .couple_pet_checkin_days(space_id, STREAK_HISTORY_DAYS)
            .await
    }

    async fn pet_view(&self, space_id: i64) -> Result<Option<CouplePetView>> {
        let Some(pet) = self.repository.couple_pet(space_id).await? else {
            return Ok(None);
        };
        let now = Utc::now();
        let today = now.date_naive();
        let days = self
            .repository
            .couple_pet_checkin_days(space_id, STREAK_HISTORY_DAYS)
            .await?;
        let (into_stage, for_next) = stage_progress(pet.experience);
        Ok(Some(CouplePetView {
            hunger: decay(pet.hunger, HUNGER_DECAY_PER_HOUR, pet.settled_at, now),
            mood: decay(pet.mood, MOOD_DECAY_PER_HOUR, pet.settled_at, now),
            streak_days: streak_from(&days, today),
            both_checked_in_today: days
                .first()
                .is_some_and(|day| day.on_date == today && day.members >= 2),
            experience_into_stage: into_stage,
            experience_for_next_stage: for_next,
            pet,
        }))
    }

    // ------------------------------------------------------- 心动 AI ------

    pub async fn list_couple_ai_works(
        &self,
        space_id: i64,
        viewer: &UserId,
        kind: Option<CoupleAiKind>,
        pagination: PageParams,
    ) -> Result<(Vec<CoupleAiWork>, i64)> {
        self.require_space_member(space_id, viewer).await?;
        self.repository
            .list_couple_ai_works(
                space_id,
                kind,
                pagination.limit_i64()?,
                pagination.offset_i64()?,
            )
            .await
    }

    /// Stores a generated piece. Generation itself is the caller's job — the
    /// model lives outside this crate, and a piece typed by hand is just as
    /// valid a thing to keep.
    pub async fn save_couple_ai_work(
        &self,
        space_id: i64,
        author: &UserId,
        kind: CoupleAiKind,
        title: &str,
        prompt: serde_json::Value,
        content: &str,
    ) -> Result<CoupleAiWork> {
        self.require_active_space(space_id, author).await?;
        let work = NewCoupleAiWork {
            kind,
            title: trimmed(title, AI_TITLE_MAX, "Title")?,
            prompt: if prompt.is_object() {
                prompt
            } else {
                serde_json::Value::Object(serde_json::Map::new())
            },
            content: required(content, AI_CONTENT_MAX, "Content")?,
        };
        self.repository
            .create_couple_ai_work(space_id, author, &work)
            .await
    }

    pub async fn delete_couple_ai_work(&self, work_id: i64, actor: &UserId) -> Result<()> {
        let space_id = self
            .repository
            .couple_ai_work_space(work_id)
            .await?
            .ok_or_else(ai_work_not_found)?;
        self.require_space_member(space_id, actor)
            .await
            .map_err(|_| ai_work_not_found())?;
        self.require_active_space(space_id, actor).await?;
        if self.repository.delete_couple_ai_work(work_id).await? {
            return Ok(());
        }
        Err(ai_work_not_found())
    }

    // --------------------------------------------------------- 首页 ------

    /// Everything the nest's home screen shows, in one read.
    pub async fn couple_nest_home(&self, space_id: i64, viewer: &UserId) -> Result<CoupleNestHome> {
        self.require_space_member(space_id, viewer).await?;
        let today = Utc::now().date_naive();

        let anniversaries = self.repository.list_couple_anniversaries(space_id).await?;
        // Soonest upcoming, each date resolved through its own repeat rule; a
        // one-off already past sorts to the end rather than being dropped, so a
        // pair with only past dates still sees something.
        let next = anniversaries
            .into_iter()
            .map(|anniversary| {
                let on = anniversary.next_occurrence(today);
                (anniversary, on)
            })
            .min_by_key(|(_, on)| (*on < today, *on));

        let (media, _) = self
            .repository
            .list_couple_media(space_id, CoupleMediaScope::All, HOME_MEDIA_LIMIT, 0)
            .await?;
        let todos = self.repository.list_couple_todos(space_id).await?;
        let (open_todo_count, done_todo_count) =
            self.repository.couple_todo_counts(space_id).await?;

        Ok(CoupleNestHome {
            cover: self.repository.couple_space_cover(space_id).await?,
            next_anniversary: next.as_ref().map(|(anniversary, _)| anniversary.clone()),
            next_anniversary_on: next.map(|(_, on)| on),
            recent_media: media,
            open_todos: todos
                .into_iter()
                .filter(|todo| todo.done_at.is_none())
                .take(HOME_TODO_LIMIT as usize)
                .collect(),
            open_todo_count,
            done_todo_count,
            memory_count: self.repository.count_couple_memories(space_id).await?,
            media_count: self.repository.count_couple_media(space_id).await?,
            pet: self.pet_view(space_id).await?,
        })
    }
}

fn memory_not_found() -> Error {
    Error::NotFound("Memory not found".to_string())
}

fn album_not_found() -> Error {
    Error::NotFound("Album not found".to_string())
}

fn media_not_found() -> Error {
    Error::NotFound("Media not found".to_string())
}

fn anniversary_not_found() -> Error {
    Error::NotFound("Anniversary not found".to_string())
}

fn todo_not_found() -> Error {
    Error::NotFound("List item not found".to_string())
}

fn pet_not_found() -> Error {
    Error::NotFound("No pet has been adopted yet".to_string())
}

fn ai_work_not_found() -> Error {
    Error::NotFound("Saved piece not found".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(days_ago: i64, members: i64) -> CouplePetCheckinDay {
        CouplePetCheckinDay {
            on_date: NaiveDate::from_ymd_opt(2026, 9, 8).unwrap() - Duration::days(days_ago),
            members,
            interactions: members,
        }
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 8).unwrap()
    }

    #[test]
    fn a_run_ending_today_counts_every_day_in_it() {
        let days = vec![day(0, 2), day(1, 1), day(2, 2)];
        assert_eq!(streak_from(&days, today()), 3);
    }

    #[test]
    fn a_run_ending_yesterday_still_counts() {
        // The day is young; a streak should not look broken at breakfast.
        let days = vec![day(1, 1), day(2, 1)];
        assert_eq!(streak_from(&days, today()), 2);
    }

    #[test]
    fn a_gap_ends_the_run() {
        let days = vec![day(0, 1), day(1, 1), day(3, 1), day(4, 1)];
        assert_eq!(streak_from(&days, today()), 2);
    }

    #[test]
    fn a_run_that_stopped_days_ago_is_not_a_streak() {
        let days = vec![day(4, 2), day(5, 2)];
        assert_eq!(streak_from(&days, today()), 0);
    }

    #[test]
    fn hunger_falls_a_point_an_hour_and_stops_at_empty() {
        let now = Utc::now();
        assert_eq!(
            decay(60, HUNGER_DECAY_PER_HOUR, now - Duration::hours(10), now),
            50
        );
        assert_eq!(
            decay(60, HUNGER_DECAY_PER_HOUR, now - Duration::hours(500), now),
            0
        );
    }

    #[test]
    fn mood_falls_at_half_the_rate_of_hunger() {
        let now = Utc::now();
        assert_eq!(
            decay(60, MOOD_DECAY_PER_HOUR, now - Duration::hours(10), now),
            55
        );
    }

    #[test]
    fn a_clock_that_went_backwards_does_not_feed_the_pet() {
        // settled_at in the future would otherwise read as negative decay.
        let now = Utc::now();
        assert_eq!(
            decay(40, HUNGER_DECAY_PER_HOUR, now + Duration::hours(5), now),
            40
        );
    }

    #[test]
    fn stages_follow_the_thresholds() {
        assert_eq!(stage_for(0), 1);
        assert_eq!(stage_for(99), 1);
        assert_eq!(stage_for(100), 2);
        assert_eq!(stage_for(300), 3);
        assert_eq!(stage_for(700), 4);
        assert_eq!(stage_for(10_000), 4);
    }

    #[test]
    fn progress_is_measured_within_the_current_stage() {
        assert_eq!(stage_progress(0), (0, 100));
        assert_eq!(stage_progress(150), (50, 200));
        // Fully grown reads as a full bar rather than an empty one.
        assert_eq!(stage_progress(900), (1, 1));
    }

    #[test]
    fn a_yearly_date_already_past_this_year_lands_on_next_year() {
        let anniversary = CoupleAnniversary {
            id: 1,
            space_id: 1,
            created_by_user_id: None,
            title: "生日".to_string(),
            happens_on: NaiveDate::from_ymd_opt(2020, 3, 4).unwrap(),
            repeat_yearly: true,
            note: String::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        assert_eq!(
            anniversary.next_occurrence(today()),
            NaiveDate::from_ymd_opt(2027, 3, 4).unwrap()
        );
    }

    #[test]
    fn a_one_off_keeps_its_own_date_even_once_it_has_passed() {
        let anniversary = CoupleAnniversary {
            id: 1,
            space_id: 1,
            created_by_user_id: None,
            title: "搬家".to_string(),
            happens_on: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
            repeat_yearly: false,
            note: String::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        assert_eq!(
            anniversary.next_occurrence(today()),
            NaiveDate::from_ymd_opt(2024, 1, 2).unwrap()
        );
    }

    #[test]
    fn the_twenty_ninth_of_february_falls_back_rather_than_skipping_three_years() {
        let anniversary = CoupleAnniversary {
            id: 1,
            space_id: 1,
            created_by_user_id: None,
            title: "闰日".to_string(),
            happens_on: NaiveDate::from_ymd_opt(2024, 2, 29).unwrap(),
            repeat_yearly: true,
            note: String::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        // 2027 is not a leap year, so the countdown points at the 28th.
        assert_eq!(
            anniversary.next_occurrence(today()),
            NaiveDate::from_ymd_opt(2027, 2, 28).unwrap()
        );
    }
}
