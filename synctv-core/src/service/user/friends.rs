//! Friend requests and the friendship graph.
//!
//! The repository stores one edge per pair and rejects a duplicate pending
//! request by index, so what lives here is what makes friending a *social*
//! action: nobody friends themselves, nobody friends an account that is gone or
//! that has blocked them, and two people who ask each other are made friends
//! rather than left holding each other's invitation.

use crate::{
    models::{
        AcceptedFriendRequest, Friend, FriendRequest, FriendRequestDirection, FriendRequestOutcome,
        PageParams, SocialUnreadSummary, UserId, FRIEND_REQUEST_MESSAGE_MAX_CHARS,
    },
    Error, Result,
};

use super::UserService;

impl UserService {
    /// Asks `addressee_user_id` to be friends.
    ///
    /// Asking again while a request is open returns that request instead of an
    /// error, and asking someone who has already asked you accepts theirs.
    pub async fn send_friend_request(
        &self,
        requester_user_id: &UserId,
        addressee_user_id: &UserId,
        message: &str,
    ) -> Result<FriendRequestOutcome> {
        if requester_user_id == addressee_user_id {
            return Err(Error::InvalidInput(
                "Cannot send a friend request to yourself".to_string(),
            ));
        }
        let message = validate_request_message(message)?;
        self.ensure_friendable(requester_user_id, addressee_user_id)
            .await?;

        if self
            .repository
            .are_friends(requester_user_id, addressee_user_id)
            .await?
        {
            return Err(Error::Conflict(
                "You are already friends with this user".to_string(),
            ));
        }

        // Both sides wanting it is consent enough: accept what they sent rather
        // than opening a second request that each would have to answer.
        if let Some(their_request_id) = self
            .repository
            .pending_friend_request_id(addressee_user_id, requester_user_id)
            .await?
        {
            if let Some(accepted) = self
                .repository
                .accept_friend_request(their_request_id, requester_user_id)
                .await?
            {
                return Ok(FriendRequestOutcome::Accepted(accepted));
            }
        }

        if let Some(request_id) = self
            .repository
            .create_friend_request(requester_user_id, addressee_user_id, &message)
            .await?
        {
            return Ok(FriendRequestOutcome::Pending { request_id });
        }

        // The partial unique index refused the insert, so a request in this
        // direction is already open. Reporting it is the answer the caller needs.
        let request_id = self
            .repository
            .pending_friend_request_id(requester_user_id, addressee_user_id)
            .await?
            .ok_or_else(|| {
                Error::Conflict("A friend request to this user already exists".to_string())
            })?;

        Ok(FriendRequestOutcome::Pending { request_id })
    }

    /// Accepts a request the caller received.
    pub async fn accept_friend_request(
        &self,
        request_id: i64,
        addressee_user_id: &UserId,
    ) -> Result<AcceptedFriendRequest> {
        self.repository
            .accept_friend_request(request_id, addressee_user_id)
            .await?
            .ok_or_else(|| Error::NotFound("Friend request not found".to_string()))
    }

    /// Turns down a request the caller received.
    pub async fn decline_friend_request(
        &self,
        request_id: i64,
        addressee_user_id: &UserId,
    ) -> Result<()> {
        if !self
            .repository
            .decline_friend_request(request_id, addressee_user_id)
            .await?
        {
            return Err(Error::NotFound("Friend request not found".to_string()));
        }
        Ok(())
    }

    /// Withdraws a request the caller sent.
    pub async fn cancel_friend_request(
        &self,
        request_id: i64,
        requester_user_id: &UserId,
    ) -> Result<()> {
        if !self
            .repository
            .cancel_friend_request(request_id, requester_user_id)
            .await?
        {
            return Err(Error::NotFound("Friend request not found".to_string()));
        }
        Ok(())
    }

    /// Open requests in one direction, newest first.
    pub async fn list_friend_requests(
        &self,
        user_id: &UserId,
        direction: FriendRequestDirection,
        pagination: PageParams,
    ) -> Result<(Vec<FriendRequest>, i64)> {
        self.repository
            .list_friend_requests(user_id, direction, pagination)
            .await
    }

    /// The caller's friends, newest first, optionally filtered by username.
    pub async fn list_friends(
        &self,
        user_id: &UserId,
        pagination: PageParams,
        search: Option<&str>,
    ) -> Result<(Vec<Friend>, i64)> {
        self.repository
            .list_friends(user_id, pagination, search)
            .await
    }

    pub async fn are_friends(&self, first: &UserId, second: &UserId) -> Result<bool> {
        self.repository.are_friends(first, second).await
    }

    /// Drops the friendship from both sides.
    ///
    /// The direct thread survives: unfriending should not destroy a history both
    /// accounts wrote, and it stays unwritable while they are not friends.
    pub async fn remove_friend(&self, user_id: &UserId, friend_user_id: &UserId) -> Result<bool> {
        self.repository
            .remove_friendship(user_id, friend_user_id)
            .await
    }

    /// The badge numbers for the 消息 tab.
    pub async fn social_unread_summary(&self, user_id: &UserId) -> Result<SocialUnreadSummary> {
        self.repository.social_unread_summary(user_id).await
    }

    /// Rejects the pairs that must never become an edge: a missing account, and
    /// either direction of a block.
    ///
    /// A block held by the *other* side is reported without naming it — the
    /// message must not tell an account that it has been blocked. The caller's own
    /// block is theirs to see, so that one says what to do about it.
    pub(super) async fn ensure_friendable(
        &self,
        actor_user_id: &UserId,
        other_user_id: &UserId,
    ) -> Result<()> {
        if self.repository.get_by_id(other_user_id).await?.is_none() {
            return Err(Error::NotFound("User not found".to_string()));
        }
        if self
            .repository
            .is_blocking(other_user_id, actor_user_id)
            .await?
        {
            return Err(Error::Authorization("Cannot reach this user".to_string()));
        }
        if self
            .repository
            .is_blocking(actor_user_id, other_user_id)
            .await?
        {
            return Err(Error::InvalidInput("Unblock this user first".to_string()));
        }
        Ok(())
    }
}

/// The note attached to a request. Empty is fine — most requests carry none.
fn validate_request_message(message: &str) -> Result<String> {
    let trimmed = message.trim();
    if trimmed.chars().count() > FRIEND_REQUEST_MESSAGE_MAX_CHARS {
        return Err(Error::InvalidInput(format!(
            "message must be at most {FRIEND_REQUEST_MESSAGE_MAX_CHARS} characters"
        )));
    }
    Ok(trimmed.to_string())
}
