//! 消息: direct threads, group chats, their messages and their rosters.
//!
//! Every path here is conversation-scoped but none puts the conversation id in the
//! path. The id travels in the query for reads and deletes and in the body for
//! writes, so one flat set of paths covers both thread kinds — a direct thread and
//! a group differ only in what the service will let the caller do to them.
//! `/direct` and `/groups` are the two ways a thread comes into being: opening a
//! direct thread is idempotent and returns the existing one, creating a group is not.

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};

use super::{middleware::RequestMetadata, validation::ProtoQuery, AppResult, AppState};
use synctv_api_common::impls::EndpointRateLimitCategory;
use synctv_proto::client::{
    AddConversationMembersRequest, AddConversationMembersResponse, CreateGroupConversationRequest,
    CreateGroupConversationResponse, DeleteConversationMessageRequest,
    DeleteConversationMessageResponse, DeleteConversationRequest, DeleteConversationResponse,
    GetConversationRequest, GetConversationResponse, LeaveConversationRequest,
    LeaveConversationResponse, ListConversationMembersRequest, ListConversationMembersResponse,
    ListConversationMessagesRequest, ListConversationMessagesResponse, ListConversationsRequest,
    ListConversationsResponse, MarkConversationReadRequest, MarkConversationReadResponse,
    OpenDirectConversationRequest, OpenDirectConversationResponse, RemoveConversationMemberRequest,
    RemoveConversationMemberResponse, SendConversationMessageRequest,
    SendConversationMessageResponse, SetConversationMutedRequest, SetConversationMutedResponse,
    SetConversationTitleRequest, SetConversationTitleResponse,
};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/user/conversations",
            get(list_conversations).delete(delete_conversation),
        )
        .route(
            "/api/user/conversations/direct",
            post(open_direct_conversation),
        )
        .route(
            "/api/user/conversations/groups",
            post(create_group_conversation),
        )
        .route("/api/user/conversations/detail", get(get_conversation))
        .route(
            "/api/user/conversations/messages",
            get(list_conversation_messages)
                .post(send_conversation_message)
                .delete(delete_conversation_message),
        )
        .route("/api/user/conversations/read", post(mark_conversation_read))
        .route(
            "/api/user/conversations/muted",
            post(set_conversation_muted),
        )
        .route(
            "/api/user/conversations/members",
            get(list_conversation_members)
                .post(add_conversation_members)
                .delete(remove_conversation_member),
        )
        .route("/api/user/conversations/leave", post(leave_conversation))
        .route(
            "/api/user/conversations/title",
            axum::routing::patch(set_conversation_title),
        )
}

#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/user/conversations",
        tag = "Messaging",
        params(ListConversationsRequest),
        responses((status = 200, description = "Conversation list page", body = ListConversationsResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn list_conversations(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    ProtoQuery(req): ProtoQuery<ListConversationsRequest>,
) -> AppResult<Json<ListConversationsResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Read,
            |auth| async move { client_api.list_conversations(&auth.user_id(), req).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

/// Removes the thread for the caller. The service decides whether that hides one
/// side's copy or ends a group the caller owns.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        delete,
        path = "/api/user/conversations",
        tag = "Messaging",
        params(DeleteConversationRequest),
        responses((status = 200, description = "Conversation deleted", body = DeleteConversationResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn delete_conversation(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    ProtoQuery(req): ProtoQuery<DeleteConversationRequest>,
) -> AppResult<Json<DeleteConversationResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move { client_api.delete_conversation(&auth.user_id(), req).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

/// Idempotent: tapping 发消息 twice on the same profile reopens one thread
/// rather than forking a second one.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/user/conversations/direct",
        tag = "Messaging",
        request_body = OpenDirectConversationRequest,
        responses((status = 200, description = "Direct thread opened", body = OpenDirectConversationResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn open_direct_conversation(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    Json(req): Json<OpenDirectConversationRequest>,
) -> AppResult<Json<OpenDirectConversationResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move {
                client_api
                    .open_direct_conversation(&auth.user_id(), req)
                    .await
            },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/user/conversations/groups",
        tag = "Messaging",
        request_body = CreateGroupConversationRequest,
        responses((status = 200, description = "Group created", body = CreateGroupConversationResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn create_group_conversation(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    Json(req): Json<CreateGroupConversationRequest>,
) -> AppResult<Json<CreateGroupConversationResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move {
                client_api
                    .create_group_conversation(&auth.user_id(), req)
                    .await
            },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

/// `/detail` rather than `/{conversationId}`: a literal segment cannot be
/// shadowed by the sibling action paths below it.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/user/conversations/detail",
        tag = "Messaging",
        params(GetConversationRequest),
        responses((status = 200, description = "Conversation summary", body = GetConversationResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn get_conversation(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    ProtoQuery(req): ProtoQuery<GetConversationRequest>,
) -> AppResult<Json<GetConversationResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Read,
            |auth| async move { client_api.get_conversation(&auth.user_id(), req).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

/// Cursor-paged backwards from newest, which is the direction a chat scrolls.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/user/conversations/messages",
        tag = "Messaging",
        params(ListConversationMessagesRequest),
        responses((status = 200, description = "Message page", body = ListConversationMessagesResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn list_conversation_messages(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    ProtoQuery(req): ProtoQuery<ListConversationMessagesRequest>,
) -> AppResult<Json<ListConversationMessagesResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Read,
            |auth| async move {
                client_api
                    .list_conversation_messages(&auth.user_id(), req)
                    .await
            },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/user/conversations/messages",
        tag = "Messaging",
        request_body = SendConversationMessageRequest,
        responses((status = 200, description = "Message sent", body = SendConversationMessageResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn send_conversation_message(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    Json(req): Json<SendConversationMessageRequest>,
) -> AppResult<Json<SendConversationMessageResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move {
                client_api
                    .send_conversation_message(&auth.user_id(), req)
                    .await
            },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        delete,
        path = "/api/user/conversations/messages",
        tag = "Messaging",
        params(DeleteConversationMessageRequest),
        responses((status = 200, description = "Message deleted", body = DeleteConversationMessageResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn delete_conversation_message(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    ProtoQuery(req): ProtoQuery<DeleteConversationMessageRequest>,
) -> AppResult<Json<DeleteConversationMessageResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move {
                client_api
                    .delete_conversation_message(&auth.user_id(), req)
                    .await
            },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

/// Moves the caller's read cursor. The response echoes the cursor it settled on,
/// which is the empty string for a thread that has no messages yet.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/user/conversations/read",
        tag = "Messaging",
        request_body = MarkConversationReadRequest,
        responses((status = 200, description = "Read cursor moved", body = MarkConversationReadResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn mark_conversation_read(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    Json(req): Json<MarkConversationReadRequest>,
) -> AppResult<Json<MarkConversationReadResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move {
                client_api
                    .mark_conversation_read(&auth.user_id(), req)
                    .await
            },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

/// Per-member mute, so muting a group does not touch anyone else's setting.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/user/conversations/muted",
        tag = "Messaging",
        request_body = SetConversationMutedRequest,
        responses((status = 200, description = "Mute state updated", body = SetConversationMutedResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn set_conversation_muted(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    Json(req): Json<SetConversationMutedRequest>,
) -> AppResult<Json<SetConversationMutedResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move {
                client_api
                    .set_conversation_muted(&auth.user_id(), req)
                    .await
            },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/user/conversations/members",
        tag = "Messaging",
        params(ListConversationMembersRequest),
        responses((status = 200, description = "Member page", body = ListConversationMembersResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn list_conversation_members(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    ProtoQuery(req): ProtoQuery<ListConversationMembersRequest>,
) -> AppResult<Json<ListConversationMembersResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Read,
            |auth| async move {
                client_api
                    .list_conversation_members(&auth.user_id(), req)
                    .await
            },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

/// Plural: the invite sheet adds a whole selection in one write, and the response
/// reports how many were new so already-present picks are not an error.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/user/conversations/members",
        tag = "Messaging",
        request_body = AddConversationMembersRequest,
        responses((status = 200, description = "Members added", body = AddConversationMembersResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn add_conversation_members(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    Json(req): Json<AddConversationMembersRequest>,
) -> AppResult<Json<AddConversationMembersResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move {
                client_api
                    .add_conversation_members(&auth.user_id(), req)
                    .await
            },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

/// Kicks one member; leaving is the separate endpoint below, since the two differ
/// in who may do it and in what the group is told afterwards.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        delete,
        path = "/api/user/conversations/members",
        tag = "Messaging",
        params(RemoveConversationMemberRequest),
        responses((status = 200, description = "Member removed", body = RemoveConversationMemberResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn remove_conversation_member(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    ProtoQuery(req): ProtoQuery<RemoveConversationMemberRequest>,
) -> AppResult<Json<RemoveConversationMemberResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move {
                client_api
                    .remove_conversation_member(&auth.user_id(), req)
                    .await
            },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/user/conversations/leave",
        tag = "Messaging",
        request_body = LeaveConversationRequest,
        responses((status = 200, description = "Left the conversation", body = LeaveConversationResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn leave_conversation(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    Json(req): Json<LeaveConversationRequest>,
) -> AppResult<Json<LeaveConversationResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move { client_api.leave_conversation(&auth.user_id(), req).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

/// Group rename. Direct threads take their name from the counterpart, so the
/// service rejects this for them rather than storing a title nobody would see.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        patch,
        path = "/api/user/conversations/title",
        tag = "Messaging",
        request_body = SetConversationTitleRequest,
        responses((status = 200, description = "Title updated", body = SetConversationTitleResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn set_conversation_title(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    Json(req): Json<SetConversationTitleRequest>,
) -> AppResult<Json<SetConversationTitleResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move {
                client_api
                    .set_conversation_title(&auth.user_id(), req)
                    .await
            },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}
