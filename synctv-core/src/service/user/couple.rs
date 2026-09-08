//! 情侣空间: inviting a partner, binding, and the space the two of them share.
//!
//! The database guarantees at most one *active* space per account through
//! `couple_space_members`, so this file is about the rest: who may invite whom,
//! which side of a pending invite the caller is on, and turning a stored row into
//! the view the space screen renders — the partner's account, the day counter and
//! the two headline numbers.

use chrono::{NaiveDate, Utc};

use crate::{
    models::{
        CoupleFavorite, CoupleSpace, CoupleSpaceStatus, CoupleSpaceView, NewCoupleFavorite,
        PageParams, SharedWatchEntry, UserId, COUPLE_FAVORITE_NOTE_MAX_CHARS,
        COUPLE_INVITE_MESSAGE_MAX_CHARS, COUPLE_SPACE_TITLE_MAX_CHARS,
    },
    Error, Result,
};

use super::watch_history::{
    validate_entry_cover_url, validate_entry_source_key, validate_entry_title,
};
use super::UserService;

impl UserService {
    /// The space that concerns this account — the active one, or else the invite
    /// waiting on an answer. `None` means the 情侣空间 card renders its empty state.
    pub async fn couple_space_for_user(&self, user_id: &UserId) -> Result<Option<CoupleSpaceView>> {
        let Some(space) = self.repository.couple_space_for_user(user_id).await? else {
            return Ok(None);
        };
        self.couple_space_view(space, user_id).await.map(Some)
    }

    /// Invites `partner_user_id` to bind.
    ///
    /// If they have already invited the caller, that invite is accepted instead:
    /// two people inviting each other is agreement, not a pair of invitations
    /// each waiting on the other.
    pub async fn invite_couple_partner(
        &self,
        initiator_user_id: &UserId,
        partner_user_id: &UserId,
        title: &str,
        invite_message: &str,
    ) -> Result<CoupleSpaceView> {
        if initiator_user_id == partner_user_id {
            return Err(Error::InvalidInput("Cannot invite yourself".to_string()));
        }
        let title = validate_space_title(title)?;
        let invite_message = validate_invite_message(invite_message)?;
        self.ensure_friendable(initiator_user_id, partner_user_id)
            .await?;

        self.ensure_not_bound(initiator_user_id, "You are already in a couple space")
            .await?;
        self.ensure_not_bound(partner_user_id, "This user is already in a couple space")
            .await?;

        if let Some(theirs) = self
            .pending_invite_from(partner_user_id, initiator_user_id)
            .await?
        {
            return self
                .accept_couple_invite(theirs, initiator_user_id, None)
                .await;
        }

        let space_id = self
            .repository
            .create_couple_invite(initiator_user_id, partner_user_id, &title, &invite_message)
            .await?;

        // No id means the partial unique index refused a second open invite in
        // this direction, so the existing one is the answer.
        let space = match space_id {
            Some(space_id) => self.require_space(space_id).await?,
            None => self
                .repository
                .couple_space_for_user(initiator_user_id)
                .await?
                .filter(|space| space.status == CoupleSpaceStatus::Pending)
                .ok_or_else(|| {
                    Error::Conflict("An invite to this user already exists".to_string())
                })?,
        };

        self.couple_space_view(space, initiator_user_id).await
    }

    /// Accepts an invite the caller received, binding both accounts.
    ///
    /// `anniversary_date` is optional; without it the space counts from today.
    pub async fn accept_couple_invite(
        &self,
        space_id: i64,
        partner_user_id: &UserId,
        anniversary_date: Option<NaiveDate>,
    ) -> Result<CoupleSpaceView> {
        let space = self
            .repository
            .accept_couple_invite(space_id, partner_user_id, anniversary_date)
            .await?
            .ok_or_else(invite_not_found)?;

        self.couple_space_view(space, partner_user_id).await
    }

    /// Settles a pending invite: declining one the caller received, or
    /// withdrawing one they sent. Both end the same way, so both land here.
    pub async fn dismiss_couple_invite(&self, space_id: i64, actor_user_id: &UserId) -> Result<()> {
        if !self
            .repository
            .settle_pending_couple_invite(space_id, actor_user_id)
            .await?
        {
            return Err(invite_not_found());
        }
        Ok(())
    }

    /// Unbinds an active space, freeing both accounts to bind again.
    ///
    /// The space and its shared list stay readable; what goes is the membership.
    pub async fn unbind_couple_space(&self, space_id: i64, actor_user_id: &UserId) -> Result<()> {
        if !self
            .repository
            .end_couple_space(space_id, actor_user_id)
            .await?
        {
            return Err(space_not_found());
        }
        Ok(())
    }

    /// Edits the title or the anniversary, from either side.
    pub async fn update_couple_space(
        &self,
        space_id: i64,
        actor_user_id: &UserId,
        title: Option<&str>,
        anniversary_date: Option<NaiveDate>,
    ) -> Result<CoupleSpaceView> {
        let title = title.map(validate_space_title).transpose()?;

        let space = self
            .repository
            .update_couple_space(space_id, actor_user_id, title.as_deref(), anniversary_date)
            .await?
            .ok_or_else(space_not_found)?;

        self.couple_space_view(space, actor_user_id).await
    }

    /// Adds to the shared list. Adding something already on it refreshes the
    /// entry, so the button is a plain toggle for either member.
    pub async fn add_couple_favorite(
        &self,
        space_id: i64,
        actor_user_id: &UserId,
        favorite: &NewCoupleFavorite,
    ) -> Result<CoupleFavorite> {
        self.require_active_space(space_id, actor_user_id).await?;

        let normalized = NewCoupleFavorite {
            room_id: favorite.room_id,
            media_id: favorite.media_id,
            title: validate_entry_title(&favorite.title)?,
            cover_url: validate_entry_cover_url(&favorite.cover_url)?,
            source_key: validate_entry_source_key(&favorite.source_key)?,
            note: validate_favorite_note(&favorite.note)?,
        };

        self.repository
            .add_couple_favorite(space_id, actor_user_id, &normalized)
            .await
    }

    /// Removes a shared entry by `source_key`.
    pub async fn remove_couple_favorite(
        &self,
        space_id: i64,
        actor_user_id: &UserId,
        source_key: &str,
    ) -> Result<()> {
        self.require_active_space(space_id, actor_user_id).await?;
        let source_key = validate_entry_source_key(source_key)?;

        if !self
            .repository
            .remove_couple_favorite(space_id, &source_key)
            .await?
        {
            return Err(Error::NotFound("Favorite not found".to_string()));
        }
        Ok(())
    }

    /// The shared list, newest first.
    pub async fn list_couple_favorites(
        &self,
        space_id: i64,
        viewer_user_id: &UserId,
        pagination: PageParams,
    ) -> Result<(Vec<CoupleFavorite>, i64)> {
        self.require_space_member(space_id, viewer_user_id).await?;

        self.repository
            .list_couple_favorites(space_id, pagination)
            .await
    }

    /// 共同观影记录: titles both members have watched, newest activity first.
    pub async fn list_couple_shared_watch_entries(
        &self,
        space_id: i64,
        viewer_user_id: &UserId,
        pagination: PageParams,
    ) -> Result<(Vec<SharedWatchEntry>, i64)> {
        let space = self.require_space_member(space_id, viewer_user_id).await?;
        let partner_user_id = partner_of(&space, viewer_user_id)?;

        self.repository
            .list_shared_watch_entries(viewer_user_id, &partner_user_id, pagination)
            .await
    }

    /// Assembles what the space screen renders from a stored row.
    async fn couple_space_view(
        &self,
        space: CoupleSpace,
        viewer_user_id: &UserId,
    ) -> Result<CoupleSpaceView> {
        let partner_user_id = partner_of(&space, viewer_user_id)?;
        let partner = self
            .repository
            .get_by_id(&partner_user_id)
            .await?
            .ok_or_else(|| Error::NotFound("User not found".to_string()))?;

        // A pending or ended space has no live counters to show, and only a bound
        // one has a day count.
        let (favorite_count, shared_watch_count) = if space.status == CoupleSpaceStatus::Active {
            self.repository
                .couple_space_counts(space.id, viewer_user_id, &partner_user_id)
                .await?
        } else {
            (0, 0)
        };
        let days_together = if space.status == CoupleSpaceStatus::Active {
            space.anniversary_date.map(days_since)
        } else {
            None
        };

        Ok(CoupleSpaceView {
            viewer_is_initiator: space.initiator_user_id == *viewer_user_id,
            partner,
            days_together,
            favorite_count,
            shared_watch_count,
            space,
        })
    }

    /// A space the caller belongs to, whatever its status, for read paths.
    pub(super) async fn require_space_member(
        &self,
        space_id: i64,
        user_id: &UserId,
    ) -> Result<CoupleSpace> {
        let space = self.require_space(space_id).await?;
        if space.initiator_user_id != *user_id && space.partner_user_id != *user_id {
            // Membership is what makes the space visible at all, so a stranger's
            // id must not distinguish "not yours" from "does not exist".
            return Err(space_not_found());
        }
        Ok(space)
    }

    /// A space the caller belongs to and that is still bound, for write paths.
    pub(super) async fn require_active_space(
        &self,
        space_id: i64,
        user_id: &UserId,
    ) -> Result<CoupleSpace> {
        let space = self.require_space_member(space_id, user_id).await?;
        if space.status != CoupleSpaceStatus::Active {
            return Err(Error::Conflict(
                "This couple space is not active".to_string(),
            ));
        }
        Ok(space)
    }

    async fn require_space(&self, space_id: i64) -> Result<CoupleSpace> {
        self.repository
            .couple_space(space_id)
            .await?
            .ok_or_else(space_not_found)
    }

    /// Refuses to start something that would need a second active membership.
    ///
    /// The primary key on `couple_space_members` is the real guarantee; this only
    /// turns it into a message that says which account is the problem.
    async fn ensure_not_bound(&self, user_id: &UserId, message: &str) -> Result<()> {
        if self.repository.has_active_couple_space(user_id).await? {
            return Err(Error::Conflict(message.to_string()));
        }
        Ok(())
    }

    /// Id of the open invite from `initiator_user_id` to `partner_user_id`.
    async fn pending_invite_from(
        &self,
        initiator_user_id: &UserId,
        partner_user_id: &UserId,
    ) -> Result<Option<i64>> {
        Ok(self
            .repository
            .couple_space_for_user(partner_user_id)
            .await?
            .filter(|space| {
                space.status == CoupleSpaceStatus::Pending
                    && space.initiator_user_id == *initiator_user_id
                    && space.partner_user_id == *partner_user_id
            })
            .map(|space| space.id))
    }
}

/// The other account, whichever column it sits in.
fn partner_of(space: &CoupleSpace, viewer_user_id: &UserId) -> Result<UserId> {
    if space.initiator_user_id == *viewer_user_id {
        Ok(space.partner_user_id)
    } else if space.partner_user_id == *viewer_user_id {
        Ok(space.initiator_user_id)
    } else {
        Err(space_not_found())
    }
}

/// Whole days from `anniversary` to today, counting both ends, so the day a
/// couple binds reads "1 天" rather than "0 天". A future date reads zero.
fn days_since(anniversary: NaiveDate) -> i64 {
    Utc::now()
        .date_naive()
        .signed_duration_since(anniversary)
        .num_days()
        .saturating_add(1)
        .max(0)
}

fn space_not_found() -> Error {
    Error::NotFound("Couple space not found".to_string())
}

fn invite_not_found() -> Error {
    Error::NotFound("Couple invite not found".to_string())
}

fn validate_space_title(title: &str) -> Result<String> {
    let trimmed = title.trim();
    if trimmed.chars().count() > COUPLE_SPACE_TITLE_MAX_CHARS {
        return Err(Error::InvalidInput(format!(
            "title must be at most {COUPLE_SPACE_TITLE_MAX_CHARS} characters"
        )));
    }
    Ok(trimmed.to_string())
}

fn validate_invite_message(invite_message: &str) -> Result<String> {
    let trimmed = invite_message.trim();
    if trimmed.chars().count() > COUPLE_INVITE_MESSAGE_MAX_CHARS {
        return Err(Error::InvalidInput(format!(
            "invite_message must be at most {COUPLE_INVITE_MESSAGE_MAX_CHARS} characters"
        )));
    }
    Ok(trimmed.to_string())
}

fn validate_favorite_note(note: &str) -> Result<String> {
    let trimmed = note.trim();
    if trimmed.chars().count() > COUPLE_FAVORITE_NOTE_MAX_CHARS {
        return Err(Error::InvalidInput(format!(
            "note must be at most {COUPLE_FAVORITE_NOTE_MAX_CHARS} characters"
        )));
    }
    Ok(trimmed.to_string())
}
