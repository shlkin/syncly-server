//! Friend requests and the friendship list behind the 消息 tab.
//!
//! Listings load their avatars in one batch: a page of requests or friends is a
//! page of accounts, and resolving each avatar in turn would make the inbox as
//! slow as its longest lookup.

use synctv_core::models::{
    AcceptedFriendRequest, Friend, FriendRequest, FriendRequestDirection, FriendRequestOutcome,
    FriendRequestStatus, UserId, DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE,
};

use super::convert::total_to_proto;
use super::ClientApiImpl;
use crate::impls::ApiError;

impl ClientApiImpl {
    /// Asks an account to be friends.
    ///
    /// Asking someone who has already asked the caller settles both at once, so
    /// the response says which of the two happened.
    pub async fn send_friend_request(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::SendFriendRequestRequest,
    ) -> Result<synctv_proto::client::SendFriendRequestResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let addressee_user_id =
            crate::impls::proto_validated_user_id(req.user_id, &self.public_id_codec)?;
        let outcome = self
            .user_service
            .send_friend_request(user_id, &addressee_user_id, &req.message)
            .await
            .map_err(ApiError::from)?;

        match outcome {
            FriendRequestOutcome::Pending { request_id } => {
                Ok(synctv_proto::client::SendFriendRequestResponse {
                    accepted: false,
                    request_id: self
                        .public_id_codec
                        .encode_friend_request_id(request_id)
                        .map_err(ApiError::Internal)?,
                    friend: None,
                    conversation_id: String::new(),
                })
            }
            FriendRequestOutcome::Accepted(accepted) => {
                let (friend, conversation_id) = self.accepted_friend_to_proto(accepted).await?;
                Ok(synctv_proto::client::SendFriendRequestResponse {
                    accepted: true,
                    request_id: String::new(),
                    friend: Some(friend),
                    conversation_id,
                })
            }
        }
    }

    /// Accepts a request the caller received. The direct thread is opened with it,
    /// so the client can navigate straight into a conversation.
    pub async fn accept_friend_request(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::AcceptFriendRequestRequest,
    ) -> Result<synctv_proto::client::AcceptFriendRequestResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let request_id = self.decode_friend_request_id(&req.request_id)?;
        let accepted = self
            .user_service
            .accept_friend_request(request_id, user_id)
            .await
            .map_err(ApiError::from)?;
        let (friend, conversation_id) = self.accepted_friend_to_proto(accepted).await?;

        Ok(synctv_proto::client::AcceptFriendRequestResponse {
            friend: Some(friend),
            conversation_id,
        })
    }

    pub async fn decline_friend_request(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::DeclineFriendRequestRequest,
    ) -> Result<synctv_proto::client::DeclineFriendRequestResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let request_id = self.decode_friend_request_id(&req.request_id)?;
        self.user_service
            .decline_friend_request(request_id, user_id)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::DeclineFriendRequestResponse { success: true })
    }

    /// Withdraws a request the caller sent.
    pub async fn cancel_friend_request(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::CancelFriendRequestRequest,
    ) -> Result<synctv_proto::client::CancelFriendRequestResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let request_id = self.decode_friend_request_id(&req.request_id)?;
        self.user_service
            .cancel_friend_request(request_id, user_id)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::CancelFriendRequestResponse { success: true })
    }

    /// Open requests in one direction. An unset direction reads the inbox, which
    /// is the side the tab badge counts.
    pub async fn list_friend_requests(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::ListFriendRequestsRequest,
    ) -> Result<synctv_proto::client::ListFriendRequestsResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let direction = friend_request_direction_from_proto(req.direction);
        let pagination = crate::impls::proto_page_params(
            req.page,
            req.page_size,
            DEFAULT_PAGE_SIZE,
            MAX_PAGE_SIZE,
        );
        let (page, total) = self
            .user_service
            .list_friend_requests(user_id, direction, pagination)
            .await
            .map_err(ApiError::from)?;

        let counterparts = page
            .iter()
            .map(|request| request.counterpart.clone())
            .collect::<Vec<_>>();
        let counterparts = self
            .batch_user_public_views_with_loaded_avatars(&counterparts)
            .await?;
        let requests = page
            .into_iter()
            .zip(counterparts)
            .map(|(request, counterpart)| self.friend_request_to_proto(request, counterpart))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(synctv_proto::client::ListFriendRequestsResponse {
            requests,
            total: total_to_proto(total, "friend request")?,
        })
    }

    pub async fn list_friends(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::ListFriendsRequest,
    ) -> Result<synctv_proto::client::ListFriendsResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let pagination = crate::impls::proto_page_params(
            req.page,
            req.page_size,
            DEFAULT_PAGE_SIZE,
            MAX_PAGE_SIZE,
        );
        let search = (!req.search.is_empty()).then_some(req.search);
        let (page, total) = self
            .user_service
            .list_friends(user_id, pagination, search.as_deref())
            .await
            .map_err(ApiError::from)?;
        let friends = self.friends_to_proto(page).await?;

        Ok(synctv_proto::client::ListFriendsResponse {
            friends,
            total: total_to_proto(total, "friend")?,
        })
    }

    /// Drops the friendship from both sides. `success` is false when the two were
    /// not friends, which is also what unfriending twice reports.
    pub async fn remove_friend(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::RemoveFriendRequest,
    ) -> Result<synctv_proto::client::RemoveFriendResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let friend_user_id =
            crate::impls::proto_validated_user_id(req.user_id, &self.public_id_codec)?;
        let success = self
            .user_service
            .remove_friend(user_id, &friend_user_id)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::RemoveFriendResponse { success })
    }

    /// The badge numbers for the 消息 tab.
    pub async fn get_social_unread_summary(
        &self,
        user_id: &UserId,
    ) -> Result<synctv_proto::client::GetSocialUnreadSummaryResponse, ApiError> {
        let summary = self
            .user_service
            .social_unread_summary(user_id)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::GetSocialUnreadSummaryResponse {
            unread_message_count: summary.unread_message_count,
            unread_conversation_count: summary.unread_conversation_count,
            pending_friend_request_count: summary.pending_friend_request_count,
        })
    }

    fn decode_friend_request_id(&self, value: &str) -> Result<i64, ApiError> {
        self.public_id_codec
            .decode_friend_request_id(value)
            .map_err(ApiError::InvalidInput)
    }

    /// One batch of avatars for a page of friends.
    pub(super) async fn friends_to_proto(
        &self,
        page: Vec<Friend>,
    ) -> Result<Vec<synctv_proto::client::Friend>, ApiError> {
        let users = page
            .iter()
            .map(|friend| friend.user.clone())
            .collect::<Vec<_>>();
        let users = self
            .batch_user_public_views_with_loaded_avatars(&users)
            .await?;

        Ok(page
            .into_iter()
            .zip(users)
            .map(|(friend, user)| synctv_proto::client::Friend {
                user: Some(user),
                friended_at: friend.friended_at.timestamp(),
            })
            .collect())
    }

    /// The friendship and the thread that accepting opened.
    async fn accepted_friend_to_proto(
        &self,
        accepted: AcceptedFriendRequest,
    ) -> Result<(synctv_proto::client::Friend, String), ApiError> {
        let user = self
            .user_public_view_with_loaded_avatar(&accepted.friend.user)
            .await?;
        let conversation_id = self
            .public_id_codec
            .encode_conversation_id(accepted.conversation_id)
            .map_err(ApiError::Internal)?;

        Ok((
            synctv_proto::client::Friend {
                user: Some(user),
                friended_at: accepted.friend.friended_at.timestamp(),
            },
            conversation_id,
        ))
    }

    fn friend_request_to_proto(
        &self,
        request: FriendRequest,
        counterpart: synctv_proto::client::UserPublicView,
    ) -> Result<synctv_proto::client::FriendRequest, ApiError> {
        Ok(synctv_proto::client::FriendRequest {
            id: self
                .public_id_codec
                .encode_friend_request_id(request.id)
                .map_err(ApiError::Internal)?,
            counterpart: Some(counterpart),
            direction: friend_request_direction_to_proto(request.direction),
            message: request.message,
            status: friend_request_status_to_proto(request.status),
            created_at: request.created_at.timestamp(),
            responded_at: request
                .responded_at
                .map(|at| at.timestamp())
                .unwrap_or_default(),
        })
    }
}

/// An unset direction means the inbox: that is the list the tab opens on, and
/// the one whose count the badge shows.
fn friend_request_direction_from_proto(value: i32) -> FriendRequestDirection {
    match synctv_proto::client::FriendRequestDirection::try_from(value) {
        Ok(synctv_proto::client::FriendRequestDirection::Outgoing) => {
            FriendRequestDirection::Outgoing
        }
        _ => FriendRequestDirection::Incoming,
    }
}

fn friend_request_direction_to_proto(direction: FriendRequestDirection) -> i32 {
    match direction {
        FriendRequestDirection::Incoming => {
            synctv_proto::client::FriendRequestDirection::Incoming as i32
        }
        FriendRequestDirection::Outgoing => {
            synctv_proto::client::FriendRequestDirection::Outgoing as i32
        }
    }
}

fn friend_request_status_to_proto(status: FriendRequestStatus) -> i32 {
    match status {
        FriendRequestStatus::Pending => synctv_proto::client::FriendRequestStatus::Pending as i32,
        FriendRequestStatus::Accepted => synctv_proto::client::FriendRequestStatus::Accepted as i32,
        FriendRequestStatus::Declined => synctv_proto::client::FriendRequestStatus::Declined as i32,
        FriendRequestStatus::Cancelled => {
            synctv_proto::client::FriendRequestStatus::Cancelled as i32
        }
    }
}
