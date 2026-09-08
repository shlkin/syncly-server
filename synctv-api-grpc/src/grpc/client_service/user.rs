use futures::StreamExt;
use tonic::{Request, Response, Status};

use super::{map_api_error, ClientServiceImpl};
use synctv_api_common::impls::EndpointRateLimitCategory;
use synctv_proto::client::{
    user_service_server::UserService, BlockUserRequest, BlockUserResponse, CloseAccountRequest,
    CloseAccountResponse, CompleteUserAvatarUploadSessionRequest,
    CompleteUserAvatarUploadSessionResponse, ConfirmEmailBindRequest, CreateRoomRequest,
    CreateUserAvatarUploadSessionRequest, CreateUserAvatarUploadSessionResponse,
    DeletePasskeyRequest, DeletePasskeyResponse, DeleteTotpRequest, DeleteTotpResponse,
    DiscoverRoomsRequest, DiscoverRoomsResponse, FavoriteRoomRequest, FavoriteRoomResponse,
    FinishOpaquePasswordUpdateRequest, FinishPasskeyBindRequest, FinishRoomPasswordLoginRequest,
    FinishSensitiveOperationVerificationRequest, FinishTotpSetupRequest, FollowUserRequest,
    FollowUserResponse, GetProfileRequest, GetRoomDiscoveryRequest, GetRoomRequest,
    GetRoomResponse, GetUserAvatarObjectRequest, GetUserPreferencesRequest,
    GetUserPreferencesResponse, GetUserProfileRequest, GetUserProfileResponse, JoinRoomRequest,
    JoinRoomResponse, ListBlockedUsersRequest, ListBlockedUsersResponse, ListFavoriteRoomsRequest,
    ListFavoriteRoomsResponse, ListFollowersRequest, ListFollowersResponse, ListFollowingRequest,
    ListFollowingResponse, ListMyRoomsRequest, ListMyRoomsResponse, ListPasskeysRequest,
    ListPasskeysResponse, LogoutRequest, LogoutResponse, PasskeyCredential,
    RegenerateTotpRecoveryCodesRequest, RequestSensitiveOperationEmailCodeRequest,
    RequestSensitiveOperationEmailCodeResponse, Room, RoomDiscoveryItem,
    SensitiveOperationVerificationOutcome, SetTwoFactorEnabledRequest, SetUsernameRequest,
    StartEmailBindRequest, StartEmailBindResponse, StartOpaquePasswordUpdateRequest,
    StartOpaquePasswordUpdateResponse, StartPasskeyBindRequest, StartPasskeyBindResponse,
    StartRoomPasswordLoginRequest, StartRoomPasswordLoginResponse,
    StartSensitiveOperationPasskeyRequest, StartSensitiveOperationPasskeyResponse,
    StartSensitiveOperationVerificationRequest, StartTotpSetupRequest, StartTotpSetupResponse,
    TotpRecoveryCodesResponse, UnbindEmailRequest, UnblockUserRequest, UnblockUserResponse,
    UnfavoriteRoomRequest, UnfavoriteRoomResponse, UnfollowUserRequest, UnfollowUserResponse,
    UpdateUserAvatarRequest, UpdateUserPreferencesRequest, UpdateUserPreferencesResponse,
    UpdateUserSignatureRequest, UpdateUserSignatureResponse, UploadUserAvatarObjectRequest,
    UploadUserAvatarObjectResponse, User, UserAvatarObjectResponse,
};
// Profile page background: its own upload transport, because the files are
// larger and may be animated.
use synctv_proto::client::{
    ClearUserProfileBackgroundRequest, CompleteUserProfileBackgroundUploadSessionRequest,
    CompleteUserProfileBackgroundUploadSessionResponse,
    CreateUserProfileBackgroundUploadSessionRequest,
    CreateUserProfileBackgroundUploadSessionResponse, GetUserProfileBackgroundObjectRequest,
    UpdateUserProfileBackgroundRequest, UpdateUserProfileBackgroundResponse,
    UploadUserProfileBackgroundObjectRequest, UploadUserProfileBackgroundObjectResponse,
    UserProfileBackgroundObjectResponse,
};
use synctv_proto::client::{
    GetPrivacySettingsRequest, ListUserGiftShowcaseRequest, ListUserGiftShowcaseResponse,
    PrivacySettings, SearchUsersRequest, SearchUsersResponse, UpdatePrivacySettingsRequest,
};
// 我的 tab additions: history, starred media, check-in, 情侣空间 and 消息. Grouped by
// feature rather than merged above, so the list stays readable.
use synctv_proto::client::{
    AcceptCoupleInviteRequest, AcceptCoupleInviteResponse, AddCoupleFavoriteRequest,
    AddCoupleFavoriteResponse, DismissCoupleInviteRequest, DismissCoupleInviteResponse,
    GetCoupleSpaceRequest, GetCoupleSpaceResponse, InviteCouplePartnerRequest,
    InviteCouplePartnerResponse, ListCoupleFavoritesRequest, ListCoupleFavoritesResponse,
    ListCoupleSharedWatchEntriesRequest, ListCoupleSharedWatchEntriesResponse,
    RemoveCoupleFavoriteRequest, RemoveCoupleFavoriteResponse, UnbindCoupleSpaceRequest,
    UnbindCoupleSpaceResponse, UpdateCoupleSpaceRequest, UpdateCoupleSpaceResponse,
};
use synctv_proto::client::{
    AcceptFriendRequestRequest, AcceptFriendRequestResponse, CancelFriendRequestRequest,
    CancelFriendRequestResponse, DeclineFriendRequestRequest, DeclineFriendRequestResponse,
    GetSocialUnreadSummaryRequest, GetSocialUnreadSummaryResponse, ListFriendRequestsRequest,
    ListFriendRequestsResponse, ListFriendsRequest, ListFriendsResponse, RemoveFriendRequest,
    RemoveFriendResponse, SendFriendRequestRequest, SendFriendRequestResponse,
};
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
use synctv_proto::client::{
    AddCoupleMediaRequest, AddCoupleMediaResponse, AdoptCouplePetRequest, AdoptCouplePetResponse,
    CreateCoupleAlbumRequest, CreateCoupleAlbumResponse, CreateCoupleAnniversaryRequest,
    CreateCoupleAnniversaryResponse, CreateCoupleMemoryRequest, CreateCoupleMemoryResponse,
    CreateCoupleTodoRequest, CreateCoupleTodoResponse, DeleteCoupleAiWorkRequest,
    DeleteCoupleAiWorkResponse, DeleteCoupleAlbumRequest, DeleteCoupleAlbumResponse,
    DeleteCoupleAnniversaryRequest, DeleteCoupleAnniversaryResponse, DeleteCoupleMediaRequest,
    DeleteCoupleMediaResponse, DeleteCoupleMemoryRequest, DeleteCoupleMemoryResponse,
    DeleteCoupleTodoRequest, DeleteCoupleTodoResponse, GetCoupleNestHomeRequest,
    GetCoupleNestHomeResponse, GetCouplePetHistoryRequest, GetCouplePetHistoryResponse,
    GetCouplePetRequest, GetCouplePetResponse, InteractWithCouplePetRequest,
    InteractWithCouplePetResponse, ListCoupleAiWorksRequest, ListCoupleAiWorksResponse,
    ListCoupleAlbumsRequest, ListCoupleAlbumsResponse, ListCoupleAnniversariesRequest,
    ListCoupleAnniversariesResponse, ListCoupleMediaRequest, ListCoupleMediaResponse,
    ListCoupleMemoriesRequest, ListCoupleMemoriesResponse, ListCoupleTodosRequest,
    ListCoupleTodosResponse, MoveCoupleMediaRequest, MoveCoupleMediaResponse,
    RenameCoupleAlbumRequest, RenameCoupleAlbumResponse, SaveCoupleAiWorkRequest,
    SaveCoupleAiWorkResponse, SetCoupleSpaceCoverRequest, SetCoupleSpaceCoverResponse,
    UpdateCoupleAnniversaryRequest, UpdateCoupleAnniversaryResponse, UpdateCoupleMemoryRequest,
    UpdateCoupleMemoryResponse, UpdateCouplePetRequest, UpdateCouplePetResponse,
    UpdateCoupleTodoRequest, UpdateCoupleTodoResponse,
};
use synctv_proto::client::{
    AddMediaFavoriteRequest, AddMediaFavoriteResponse, ClearWatchHistoryRequest,
    ClearWatchHistoryResponse, DeleteWatchHistoryEntryRequest, DeleteWatchHistoryEntryResponse,
    GetMediaFavoriteStatesRequest, GetMediaFavoriteStatesResponse, ListMediaFavoritesRequest,
    ListMediaFavoritesResponse, ListWatchHistoryRequest, ListWatchHistoryResponse,
    RecordWatchHistoryRequest, RecordWatchHistoryResponse, RemoveMediaFavoriteRequest,
    RemoveMediaFavoriteResponse,
};
use synctv_proto::client::{
    ClaimCheckInRequest, ClaimCheckInResponse, GetCheckInStatusRequest, GetCheckInStatusResponse,
    ListPointTransactionsRequest, ListPointTransactionsResponse,
};
use synctv_proto::client::{
    GetGiftStatsRequest, GetGiftStatsResponse, ListGiftRecordsRequest, ListGiftRecordsResponse,
    ListGiftsRequest, ListGiftsResponse, ListRoomGiftsRequest, ListRoomGiftsResponse,
    ListVodSourcesRequest, ListVodSourcesResponse, SendGiftRequest, SendGiftResponse,
};

type UserAvatarObjectStream = super::GrpcStatusStream<UserAvatarObjectResponse>;
type UserProfileBackgroundObjectStream =
    super::GrpcStatusStream<UserProfileBackgroundObjectResponse>;

#[tonic::async_trait]
// Tonic generated service traits require `Result<Response<_>, tonic::Status>`.
#[allow(clippy::result_large_err)]
impl UserService for ClientServiceImpl {
    type GetUserAvatarObjectStream = UserAvatarObjectStream;
    type GetUserProfileBackgroundObjectStream = UserProfileBackgroundObjectStream;

    async fn logout(
        &self,
        request: Request<LogoutRequest>,
    ) -> Result<Response<LogoutResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let authorization = metadata.authorization.clone();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |_| async move {
                    let auth_value = authorization.ok_or_else(|| {
                        synctv_api_common::impls::ApiError::Authentication(
                            synctv_common::messages::AUTHENTICATION_REQUIRED.to_string(),
                        )
                    })?;
                    let token =
                        synctv_core::service::JwtValidator::extract_bearer_token(&auth_value)
                            .map_err(|_| {
                                synctv_api_common::impls::ApiError::Authentication(
                                    synctv_common::messages::INVALID_OR_EXPIRED_TOKEN.to_string(),
                                )
                            })?;
                    client_api.logout(&token).await?;
                    Ok::<(), synctv_api_common::impls::ApiError>(())
                },
            )
            .await
            .map_err(map_api_error)?;

        Ok(Response::new(LogoutResponse {
            success: true,
            message: String::new(),
        }))
    }

    async fn get_profile(
        &self,
        request: Request<GetProfileRequest>,
    ) -> Result<Response<User>, Status> {
        let metadata = self.request_metadata(&request)?;
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api.get_profile(&authenticated.user_id()).await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn set_username(
        &self,
        request: Request<SetUsernameRequest>,
    ) -> Result<Response<User>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api.set_username(&authenticated.user_id(), req).await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn create_user_avatar_upload_session(
        &self,
        request: Request<CreateUserAvatarUploadSessionRequest>,
    ) -> Result<Response<CreateUserAvatarUploadSessionResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .create_user_avatar_upload_session(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn upload_user_avatar_object(
        &self,
        request: Request<UploadUserAvatarObjectRequest>,
    ) -> Result<Response<UploadUserAvatarObjectResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let response = self
            .client_api
            .execute_public_endpoint(&metadata, EndpointRateLimitCategory::Write, move || {
                let client_api = self.client_api.clone();
                async move { client_api.upload_user_avatar_object(req).await }
            })
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn complete_user_avatar_upload_session(
        &self,
        request: Request<CompleteUserAvatarUploadSessionRequest>,
    ) -> Result<Response<CompleteUserAvatarUploadSessionResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let response = self
            .client_api
            .execute_public_endpoint(&metadata, EndpointRateLimitCategory::Write, move || {
                let client_api = self.client_api.clone();
                async move { client_api.complete_user_avatar_upload_session(req).await }
            })
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn get_user_avatar_object(
        &self,
        request: Request<GetUserAvatarObjectRequest>,
    ) -> Result<Response<Self::GetUserAvatarObjectStream>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let download = self
            .client_api
            .execute_public_endpoint(&metadata, EndpointRateLimitCategory::Read, move || {
                let client_api = self.client_api.clone();
                async move { client_api.get_user_avatar_object(req).await }
            })
            .await
            .map_err(map_api_error)?;
        let stream = synctv_api_common::impls::client::file_download::avatar_chunk_stream(download)
            .map(|result| result.map_err(map_api_error));
        Ok(Response::new(Box::pin(stream)))
    }

    async fn create_user_profile_background_upload_session(
        &self,
        request: Request<CreateUserProfileBackgroundUploadSessionRequest>,
    ) -> Result<Response<CreateUserProfileBackgroundUploadSessionResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .create_user_profile_background_upload_session(
                            &authenticated.user_id(),
                            req,
                        )
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn upload_user_profile_background_object(
        &self,
        request: Request<UploadUserProfileBackgroundObjectRequest>,
    ) -> Result<Response<UploadUserProfileBackgroundObjectResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let response = self
            .client_api
            .execute_public_endpoint(&metadata, EndpointRateLimitCategory::Write, move || {
                let client_api = self.client_api.clone();
                async move { client_api.upload_user_profile_background_object(req).await }
            })
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn complete_user_profile_background_upload_session(
        &self,
        request: Request<CompleteUserProfileBackgroundUploadSessionRequest>,
    ) -> Result<Response<CompleteUserProfileBackgroundUploadSessionResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let response = self
            .client_api
            .execute_public_endpoint(&metadata, EndpointRateLimitCategory::Write, move || {
                let client_api = self.client_api.clone();
                async move {
                    client_api
                        .complete_user_profile_background_upload_session(req)
                        .await
                }
            })
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn get_user_profile_background_object(
        &self,
        request: Request<GetUserProfileBackgroundObjectRequest>,
    ) -> Result<Response<Self::GetUserProfileBackgroundObjectStream>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let download = self
            .client_api
            .execute_public_endpoint(&metadata, EndpointRateLimitCategory::Read, move || {
                let client_api = self.client_api.clone();
                async move { client_api.get_user_profile_background_object(req).await }
            })
            .await
            .map_err(map_api_error)?;
        let stream =
            synctv_api_common::impls::client::file_download::profile_background_chunk_stream(
                download,
            )
            .map(|result| result.map_err(map_api_error));
        Ok(Response::new(Box::pin(stream)))
    }

    async fn update_user_profile_background(
        &self,
        request: Request<UpdateUserProfileBackgroundRequest>,
    ) -> Result<Response<UpdateUserProfileBackgroundResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .update_user_profile_background(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn clear_user_profile_background(
        &self,
        request: Request<ClearUserProfileBackgroundRequest>,
    ) -> Result<Response<UpdateUserProfileBackgroundResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .clear_user_profile_background(&authenticated.user_id())
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn update_user_avatar(
        &self,
        request: Request<UpdateUserAvatarRequest>,
    ) -> Result<Response<User>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .update_user_avatar(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn clear_user_avatar(
        &self,
        request: Request<synctv_proto::client::ClearUserAvatarRequest>,
    ) -> Result<Response<User>, Status> {
        let metadata = self.request_metadata(&request)?;
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api.clear_user_avatar(&authenticated.user_id()).await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn start_email_bind(
        &self,
        request: Request<StartEmailBindRequest>,
    ) -> Result<Response<StartEmailBindResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .start_email_bind(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn confirm_email_bind(
        &self,
        request: Request<ConfirmEmailBindRequest>,
    ) -> Result<Response<User>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .confirm_email_bind(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn unbind_email(
        &self,
        request: Request<UnbindEmailRequest>,
    ) -> Result<Response<User>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api.unbind_email(&authenticated.user_id(), req).await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn start_sensitive_operation_verification(
        &self,
        request: Request<StartSensitiveOperationVerificationRequest>,
    ) -> Result<Response<SensitiveOperationVerificationOutcome>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .start_sensitive_operation_verification(
                            &authenticated.user_id(),
                            authenticated.claims.auth_context(),
                            req,
                        )
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn start_sensitive_operation_passkey(
        &self,
        request: Request<StartSensitiveOperationPasskeyRequest>,
    ) -> Result<Response<StartSensitiveOperationPasskeyResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .start_sensitive_operation_passkey(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn request_sensitive_operation_email_code(
        &self,
        request: Request<RequestSensitiveOperationEmailCodeRequest>,
    ) -> Result<Response<RequestSensitiveOperationEmailCodeResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .request_sensitive_operation_email_code(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn finish_sensitive_operation_verification(
        &self,
        request: Request<FinishSensitiveOperationVerificationRequest>,
    ) -> Result<Response<SensitiveOperationVerificationOutcome>, Status> {
        let metadata = self.request_metadata(&request)?;
        let client_ip = metadata.client_ip;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint_with_control(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |request_control, authenticated| async move {
                    client_api
                        .finish_sensitive_operation_verification(
                            &authenticated.user_id(),
                            req,
                            client_ip,
                            Some(&request_control),
                        )
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn start_opaque_password_update(
        &self,
        request: Request<StartOpaquePasswordUpdateRequest>,
    ) -> Result<Response<StartOpaquePasswordUpdateResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .start_opaque_password_update(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn finish_opaque_password_update(
        &self,
        request: Request<FinishOpaquePasswordUpdateRequest>,
    ) -> Result<Response<User>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .finish_opaque_password_update(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn start_passkey_bind(
        &self,
        request: Request<StartPasskeyBindRequest>,
    ) -> Result<Response<StartPasskeyBindResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .start_passkey_bind(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn finish_passkey_bind(
        &self,
        request: Request<FinishPasskeyBindRequest>,
    ) -> Result<Response<PasskeyCredential>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .finish_passkey_bind_request(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn list_passkeys(
        &self,
        request: Request<ListPasskeysRequest>,
    ) -> Result<Response<ListPasskeysResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api.list_passkeys(&authenticated.user_id()).await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn delete_passkey(
        &self,
        request: Request<DeletePasskeyRequest>,
    ) -> Result<Response<DeletePasskeyResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .delete_passkey(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn start_totp_setup(
        &self,
        request: Request<StartTotpSetupRequest>,
    ) -> Result<Response<StartTotpSetupResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |auth| async move { client_api.start_totp_setup(&auth.user_id(), req).await },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn finish_totp_setup(
        &self,
        request: Request<FinishTotpSetupRequest>,
    ) -> Result<Response<TotpRecoveryCodesResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |auth| async move { client_api.finish_totp_setup(&auth.user_id(), req).await },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn regenerate_totp_recovery_codes(
        &self,
        request: Request<RegenerateTotpRecoveryCodesRequest>,
    ) -> Result<Response<TotpRecoveryCodesResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |auth| async move {
                    client_api
                        .regenerate_totp_recovery_codes(&auth.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn delete_totp(
        &self,
        request: Request<DeleteTotpRequest>,
    ) -> Result<Response<DeleteTotpResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |auth| async move { client_api.delete_totp(&auth.user_id(), req).await },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn get_user_preferences(
        &self,
        request: Request<GetUserPreferencesRequest>,
    ) -> Result<Response<GetUserPreferencesResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .get_user_preferences(&authenticated.user_id())
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn update_user_preferences(
        &self,
        request: Request<UpdateUserPreferencesRequest>,
    ) -> Result<Response<UpdateUserPreferencesResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .update_user_preferences(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn set_two_factor_enabled(
        &self,
        request: Request<SetTwoFactorEnabledRequest>,
    ) -> Result<Response<GetUserPreferencesResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .set_two_factor_enabled(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn close_account(
        &self,
        request: Request<CloseAccountRequest>,
    ) -> Result<Response<CloseAccountResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api.close_account(&authenticated.user_id()).await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn block_user(
        &self,
        request: Request<BlockUserRequest>,
    ) -> Result<Response<BlockUserResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api.block_user(&authenticated.user_id(), req).await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn unblock_user(
        &self,
        request: Request<UnblockUserRequest>,
    ) -> Result<Response<UnblockUserResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api.unblock_user(&authenticated.user_id(), req).await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn list_blocked_users(
        &self,
        request: Request<ListBlockedUsersRequest>,
    ) -> Result<Response<ListBlockedUsersResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .list_blocked_users(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn get_user_profile(
        &self,
        request: Request<GetUserProfileRequest>,
    ) -> Result<Response<GetUserProfileResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .get_user_profile(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn update_user_signature(
        &self,
        request: Request<UpdateUserSignatureRequest>,
    ) -> Result<Response<UpdateUserSignatureResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .update_user_signature(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn follow_user(
        &self,
        request: Request<FollowUserRequest>,
    ) -> Result<Response<FollowUserResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api.follow_user(&authenticated.user_id(), req).await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn unfollow_user(
        &self,
        request: Request<UnfollowUserRequest>,
    ) -> Result<Response<UnfollowUserResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .unfollow_user(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn search_users(
        &self,
        request: Request<SearchUsersRequest>,
    ) -> Result<Response<SearchUsersResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api.search_users(&authenticated.user_id(), req).await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn get_privacy_settings(
        &self,
        request: Request<GetPrivacySettingsRequest>,
    ) -> Result<Response<PrivacySettings>, Status> {
        let metadata = self.request_metadata(&request)?;
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .get_privacy_settings(&authenticated.user_id())
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn update_privacy_settings(
        &self,
        request: Request<UpdatePrivacySettingsRequest>,
    ) -> Result<Response<PrivacySettings>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .update_privacy_settings(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn list_user_gift_showcase(
        &self,
        request: Request<ListUserGiftShowcaseRequest>,
    ) -> Result<Response<ListUserGiftShowcaseResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .list_user_gift_showcase(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn list_following(
        &self,
        request: Request<ListFollowingRequest>,
    ) -> Result<Response<ListFollowingResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .list_following(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn list_followers(
        &self,
        request: Request<ListFollowersRequest>,
    ) -> Result<Response<ListFollowersResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .list_followers(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn create_room(
        &self,
        request: Request<CreateRoomRequest>,
    ) -> Result<Response<Room>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    Box::pin(client_api.create_room(&authenticated.user_id(), req)).await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn get_room(
        &self,
        request: Request<GetRoomRequest>,
    ) -> Result<Response<GetRoomResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let room_id = req.room_id.clone();
        let response = self
            .execute_room_actor_endpoint(
                metadata,
                room_id,
                EndpointRateLimitCategory::Read,
                move |client_api, actor| async move { client_api.get_room_for_actor(&actor).await },
            )
            .await?;
        Ok(Response::new(response))
    }

    async fn join_room(
        &self,
        request: Request<JoinRoomRequest>,
    ) -> Result<Response<JoinRoomResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let client_ip = metadata.client_ip.map(|ip| ip.to_string());
        let req = request.into_inner();
        let room_id = req.room_id.clone();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint_with_control(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |request_control, authenticated| async move {
                    Box::pin(client_api.join_room_with_control(
                        &authenticated.user_id(),
                        &room_id,
                        req,
                        client_ip.as_deref(),
                        Some(&request_control),
                    ))
                    .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn start_room_password_login(
        &self,
        request: Request<StartRoomPasswordLoginRequest>,
    ) -> Result<Response<StartRoomPasswordLoginResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let client_ip = metadata.client_ip.map(|ip| ip.to_string());
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint_with_control(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |request_control, authenticated| async move {
                    client_api
                        .start_room_password_login_with_control(
                            &authenticated.user_id(),
                            req,
                            client_ip.as_deref(),
                            Some(&request_control),
                        )
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn finish_room_password_login(
        &self,
        request: Request<FinishRoomPasswordLoginRequest>,
    ) -> Result<Response<JoinRoomResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let client_ip = metadata.client_ip.map(|ip| ip.to_string());
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint_with_control(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |_request_control, authenticated| async move {
                    Box::pin(client_api.finish_room_password_login_with_control(
                        &authenticated.user_id(),
                        None,
                        req,
                        client_ip.as_deref(),
                    ))
                    .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn get_room_discovery(
        &self,
        request: Request<GetRoomDiscoveryRequest>,
    ) -> Result<Response<RoomDiscoveryItem>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .get_room_discovery(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn discover_rooms(
        &self,
        request: Request<DiscoverRoomsRequest>,
    ) -> Result<Response<DiscoverRoomsResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .discover_rooms(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn list_my_rooms(
        &self,
        request: Request<ListMyRoomsRequest>,
    ) -> Result<Response<ListMyRoomsResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .list_my_rooms(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn favorite_room(
        &self,
        request: Request<FavoriteRoomRequest>,
    ) -> Result<Response<FavoriteRoomResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .favorite_room(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn unfavorite_room(
        &self,
        request: Request<UnfavoriteRoomRequest>,
    ) -> Result<Response<UnfavoriteRoomResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .unfavorite_room(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn list_favorite_rooms(
        &self,
        request: Request<ListFavoriteRoomsRequest>,
    ) -> Result<Response<ListFavoriteRoomsResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .list_favorite_rooms(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn record_watch_history(
        &self,
        request: Request<RecordWatchHistoryRequest>,
    ) -> Result<Response<RecordWatchHistoryResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .record_watch_history(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn list_watch_history(
        &self,
        request: Request<ListWatchHistoryRequest>,
    ) -> Result<Response<ListWatchHistoryResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .list_watch_history(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn delete_watch_history_entry(
        &self,
        request: Request<DeleteWatchHistoryEntryRequest>,
    ) -> Result<Response<DeleteWatchHistoryEntryResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .delete_watch_history_entry(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn clear_watch_history(
        &self,
        request: Request<ClearWatchHistoryRequest>,
    ) -> Result<Response<ClearWatchHistoryResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .clear_watch_history(&authenticated.user_id())
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn add_media_favorite(
        &self,
        request: Request<AddMediaFavoriteRequest>,
    ) -> Result<Response<AddMediaFavoriteResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .add_media_favorite(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn remove_media_favorite(
        &self,
        request: Request<RemoveMediaFavoriteRequest>,
    ) -> Result<Response<RemoveMediaFavoriteResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .remove_media_favorite(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn list_media_favorites(
        &self,
        request: Request<ListMediaFavoritesRequest>,
    ) -> Result<Response<ListMediaFavoritesResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .list_media_favorites(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn get_media_favorite_states(
        &self,
        request: Request<GetMediaFavoriteStatesRequest>,
    ) -> Result<Response<GetMediaFavoriteStatesResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .get_media_favorite_states(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn get_check_in_status(
        &self,
        request: Request<GetCheckInStatusRequest>,
    ) -> Result<Response<GetCheckInStatusResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .get_check_in_status(&authenticated.user_id())
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn claim_check_in(
        &self,
        request: Request<ClaimCheckInRequest>,
    ) -> Result<Response<ClaimCheckInResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api.claim_check_in(&authenticated.user_id()).await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn list_point_transactions(
        &self,
        request: Request<ListPointTransactionsRequest>,
    ) -> Result<Response<ListPointTransactionsResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .list_point_transactions(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn get_couple_space(
        &self,
        request: Request<GetCoupleSpaceRequest>,
    ) -> Result<Response<GetCoupleSpaceResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api.get_couple_space(&authenticated.user_id()).await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn invite_couple_partner(
        &self,
        request: Request<InviteCouplePartnerRequest>,
    ) -> Result<Response<InviteCouplePartnerResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .invite_couple_partner(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn accept_couple_invite(
        &self,
        request: Request<AcceptCoupleInviteRequest>,
    ) -> Result<Response<AcceptCoupleInviteResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .accept_couple_invite(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn dismiss_couple_invite(
        &self,
        request: Request<DismissCoupleInviteRequest>,
    ) -> Result<Response<DismissCoupleInviteResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .dismiss_couple_invite(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn unbind_couple_space(
        &self,
        request: Request<UnbindCoupleSpaceRequest>,
    ) -> Result<Response<UnbindCoupleSpaceResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .unbind_couple_space(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn update_couple_space(
        &self,
        request: Request<UpdateCoupleSpaceRequest>,
    ) -> Result<Response<UpdateCoupleSpaceResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .update_couple_space(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn add_couple_favorite(
        &self,
        request: Request<AddCoupleFavoriteRequest>,
    ) -> Result<Response<AddCoupleFavoriteResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .add_couple_favorite(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn remove_couple_favorite(
        &self,
        request: Request<RemoveCoupleFavoriteRequest>,
    ) -> Result<Response<RemoveCoupleFavoriteResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .remove_couple_favorite(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    // ==================== 爱的小窝 ====================
    // Generated alongside the HTTP routes from one table; both transports
    // reach the same ClientApiImpl method.

    async fn get_couple_nest_home(
        &self,
        request: Request<GetCoupleNestHomeRequest>,
    ) -> Result<Response<GetCoupleNestHomeResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .get_couple_nest_home(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn list_couple_memories(
        &self,
        request: Request<ListCoupleMemoriesRequest>,
    ) -> Result<Response<ListCoupleMemoriesResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .list_couple_memories(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn create_couple_memory(
        &self,
        request: Request<CreateCoupleMemoryRequest>,
    ) -> Result<Response<CreateCoupleMemoryResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .create_couple_memory(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn update_couple_memory(
        &self,
        request: Request<UpdateCoupleMemoryRequest>,
    ) -> Result<Response<UpdateCoupleMemoryResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .update_couple_memory(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn delete_couple_memory(
        &self,
        request: Request<DeleteCoupleMemoryRequest>,
    ) -> Result<Response<DeleteCoupleMemoryResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .delete_couple_memory(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn list_couple_albums(
        &self,
        request: Request<ListCoupleAlbumsRequest>,
    ) -> Result<Response<ListCoupleAlbumsResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .list_couple_albums(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn create_couple_album(
        &self,
        request: Request<CreateCoupleAlbumRequest>,
    ) -> Result<Response<CreateCoupleAlbumResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .create_couple_album(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn rename_couple_album(
        &self,
        request: Request<RenameCoupleAlbumRequest>,
    ) -> Result<Response<RenameCoupleAlbumResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .rename_couple_album(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn delete_couple_album(
        &self,
        request: Request<DeleteCoupleAlbumRequest>,
    ) -> Result<Response<DeleteCoupleAlbumResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .delete_couple_album(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn list_couple_media(
        &self,
        request: Request<ListCoupleMediaRequest>,
    ) -> Result<Response<ListCoupleMediaResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .list_couple_media(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn add_couple_media(
        &self,
        request: Request<AddCoupleMediaRequest>,
    ) -> Result<Response<AddCoupleMediaResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .add_couple_media(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn move_couple_media(
        &self,
        request: Request<MoveCoupleMediaRequest>,
    ) -> Result<Response<MoveCoupleMediaResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .move_couple_media(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn delete_couple_media(
        &self,
        request: Request<DeleteCoupleMediaRequest>,
    ) -> Result<Response<DeleteCoupleMediaResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .delete_couple_media(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn set_couple_space_cover(
        &self,
        request: Request<SetCoupleSpaceCoverRequest>,
    ) -> Result<Response<SetCoupleSpaceCoverResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .set_couple_space_cover(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn list_couple_anniversaries(
        &self,
        request: Request<ListCoupleAnniversariesRequest>,
    ) -> Result<Response<ListCoupleAnniversariesResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .list_couple_anniversaries(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn create_couple_anniversary(
        &self,
        request: Request<CreateCoupleAnniversaryRequest>,
    ) -> Result<Response<CreateCoupleAnniversaryResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .create_couple_anniversary(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn update_couple_anniversary(
        &self,
        request: Request<UpdateCoupleAnniversaryRequest>,
    ) -> Result<Response<UpdateCoupleAnniversaryResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .update_couple_anniversary(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn delete_couple_anniversary(
        &self,
        request: Request<DeleteCoupleAnniversaryRequest>,
    ) -> Result<Response<DeleteCoupleAnniversaryResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .delete_couple_anniversary(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn list_couple_todos(
        &self,
        request: Request<ListCoupleTodosRequest>,
    ) -> Result<Response<ListCoupleTodosResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .list_couple_todos(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn create_couple_todo(
        &self,
        request: Request<CreateCoupleTodoRequest>,
    ) -> Result<Response<CreateCoupleTodoResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .create_couple_todo(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn update_couple_todo(
        &self,
        request: Request<UpdateCoupleTodoRequest>,
    ) -> Result<Response<UpdateCoupleTodoResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .update_couple_todo(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn delete_couple_todo(
        &self,
        request: Request<DeleteCoupleTodoRequest>,
    ) -> Result<Response<DeleteCoupleTodoResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .delete_couple_todo(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn get_couple_pet(
        &self,
        request: Request<GetCouplePetRequest>,
    ) -> Result<Response<GetCouplePetResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .get_couple_pet(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn adopt_couple_pet(
        &self,
        request: Request<AdoptCouplePetRequest>,
    ) -> Result<Response<AdoptCouplePetResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .adopt_couple_pet(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn interact_with_couple_pet(
        &self,
        request: Request<InteractWithCouplePetRequest>,
    ) -> Result<Response<InteractWithCouplePetResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .interact_with_couple_pet(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn update_couple_pet(
        &self,
        request: Request<UpdateCouplePetRequest>,
    ) -> Result<Response<UpdateCouplePetResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .update_couple_pet(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn get_couple_pet_history(
        &self,
        request: Request<GetCouplePetHistoryRequest>,
    ) -> Result<Response<GetCouplePetHistoryResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .get_couple_pet_history(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn list_couple_ai_works(
        &self,
        request: Request<ListCoupleAiWorksRequest>,
    ) -> Result<Response<ListCoupleAiWorksResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .list_couple_ai_works(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn save_couple_ai_work(
        &self,
        request: Request<SaveCoupleAiWorkRequest>,
    ) -> Result<Response<SaveCoupleAiWorkResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .save_couple_ai_work(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn delete_couple_ai_work(
        &self,
        request: Request<DeleteCoupleAiWorkRequest>,
    ) -> Result<Response<DeleteCoupleAiWorkResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .delete_couple_ai_work(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn list_couple_favorites(
        &self,
        request: Request<ListCoupleFavoritesRequest>,
    ) -> Result<Response<ListCoupleFavoritesResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .list_couple_favorites(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn list_couple_shared_watch_entries(
        &self,
        request: Request<ListCoupleSharedWatchEntriesRequest>,
    ) -> Result<Response<ListCoupleSharedWatchEntriesResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .list_couple_shared_watch_entries(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn send_friend_request(
        &self,
        request: Request<SendFriendRequestRequest>,
    ) -> Result<Response<SendFriendRequestResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .send_friend_request(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn accept_friend_request(
        &self,
        request: Request<AcceptFriendRequestRequest>,
    ) -> Result<Response<AcceptFriendRequestResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .accept_friend_request(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn decline_friend_request(
        &self,
        request: Request<DeclineFriendRequestRequest>,
    ) -> Result<Response<DeclineFriendRequestResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .decline_friend_request(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn cancel_friend_request(
        &self,
        request: Request<CancelFriendRequestRequest>,
    ) -> Result<Response<CancelFriendRequestResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .cancel_friend_request(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn list_friend_requests(
        &self,
        request: Request<ListFriendRequestsRequest>,
    ) -> Result<Response<ListFriendRequestsResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .list_friend_requests(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn list_friends(
        &self,
        request: Request<ListFriendsRequest>,
    ) -> Result<Response<ListFriendsResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api.list_friends(&authenticated.user_id(), req).await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn remove_friend(
        &self,
        request: Request<RemoveFriendRequest>,
    ) -> Result<Response<RemoveFriendResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .remove_friend(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn get_social_unread_summary(
        &self,
        request: Request<GetSocialUnreadSummaryRequest>,
    ) -> Result<Response<GetSocialUnreadSummaryResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .get_social_unread_summary(&authenticated.user_id())
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn open_direct_conversation(
        &self,
        request: Request<OpenDirectConversationRequest>,
    ) -> Result<Response<OpenDirectConversationResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .open_direct_conversation(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn create_group_conversation(
        &self,
        request: Request<CreateGroupConversationRequest>,
    ) -> Result<Response<CreateGroupConversationResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .create_group_conversation(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn list_conversations(
        &self,
        request: Request<ListConversationsRequest>,
    ) -> Result<Response<ListConversationsResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .list_conversations(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn get_conversation(
        &self,
        request: Request<GetConversationRequest>,
    ) -> Result<Response<GetConversationResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .get_conversation(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn list_conversation_messages(
        &self,
        request: Request<ListConversationMessagesRequest>,
    ) -> Result<Response<ListConversationMessagesResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .list_conversation_messages(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn send_conversation_message(
        &self,
        request: Request<SendConversationMessageRequest>,
    ) -> Result<Response<SendConversationMessageResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .send_conversation_message(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn delete_conversation_message(
        &self,
        request: Request<DeleteConversationMessageRequest>,
    ) -> Result<Response<DeleteConversationMessageResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .delete_conversation_message(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn mark_conversation_read(
        &self,
        request: Request<MarkConversationReadRequest>,
    ) -> Result<Response<MarkConversationReadResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .mark_conversation_read(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn set_conversation_muted(
        &self,
        request: Request<SetConversationMutedRequest>,
    ) -> Result<Response<SetConversationMutedResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .set_conversation_muted(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn list_conversation_members(
        &self,
        request: Request<ListConversationMembersRequest>,
    ) -> Result<Response<ListConversationMembersResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .list_conversation_members(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn add_conversation_members(
        &self,
        request: Request<AddConversationMembersRequest>,
    ) -> Result<Response<AddConversationMembersResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .add_conversation_members(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn remove_conversation_member(
        &self,
        request: Request<RemoveConversationMemberRequest>,
    ) -> Result<Response<RemoveConversationMemberResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .remove_conversation_member(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn leave_conversation(
        &self,
        request: Request<LeaveConversationRequest>,
    ) -> Result<Response<LeaveConversationResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .leave_conversation(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn set_conversation_title(
        &self,
        request: Request<SetConversationTitleRequest>,
    ) -> Result<Response<SetConversationTitleResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .set_conversation_title(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn delete_conversation(
        &self,
        request: Request<DeleteConversationRequest>,
    ) -> Result<Response<DeleteConversationResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api
                        .delete_conversation(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }
    async fn list_gifts(
        &self,
        request: Request<ListGiftsRequest>,
    ) -> Result<Response<ListGiftsResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api.list_gifts(&authenticated.user_id()).await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn list_vod_sources(
        &self,
        request: Request<ListVodSourcesRequest>,
    ) -> Result<Response<ListVodSourcesResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |_authenticated| async move { client_api.list_vod_sources().await },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn send_gift(
        &self,
        request: Request<SendGiftRequest>,
    ) -> Result<Response<SendGiftResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Write,
                move |authenticated| async move {
                    client_api.send_gift(&authenticated.user_id(), req).await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn list_gift_records(
        &self,
        request: Request<ListGiftRecordsRequest>,
    ) -> Result<Response<ListGiftRecordsResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .list_gift_records(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn list_room_gifts(
        &self,
        request: Request<ListRoomGiftsRequest>,
    ) -> Result<Response<ListRoomGiftsResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .list_room_gifts(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }

    async fn get_gift_stats(
        &self,
        request: Request<GetGiftStatsRequest>,
    ) -> Result<Response<GetGiftStatsResponse>, Status> {
        let metadata = self.request_metadata(&request)?;
        let req = request.into_inner();
        let executor = self.client_api.clone();
        let client_api = self.client_api.clone();
        let response = executor
            .execute_user_endpoint(
                &metadata,
                EndpointRateLimitCategory::Read,
                move |authenticated| async move {
                    client_api
                        .get_gift_stats(&authenticated.user_id(), req)
                        .await
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(Response::new(response))
    }
}
