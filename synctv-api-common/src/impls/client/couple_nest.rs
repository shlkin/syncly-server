//! 爱的小窝: the nest a bound pair keeps — timeline, albums, media,
//! anniversaries, the shared list, the pet and the saved AI pieces.
//!
//! Every call names the space it acts on and the service resolves membership
//! from it, so nothing here has to reason about who may see what: a space has
//! exactly two members and both may read and write all of it.
//!
//! Dates cross the wire as `YYYY-MM-DD` strings rather than timestamps. A
//! memory is filed under a *day*, and a timestamp would drag the reader's
//! timezone into deciding which day that was.

use synctv_core::models::{
    CoupleAiKind, CoupleAiWork, CoupleAlbum, CoupleAnniversary, CoupleAnniversaryUpdate,
    CoupleMedia, CoupleMediaKind, CoupleMediaScope, CoupleMemory, CoupleMemoryUpdate,
    CouplePetAction, CouplePetView, CoupleTodo, CoupleTodoUpdate, NewCoupleMedia, UserId,
    DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE,
};

use super::convert::{
    optional_date_from_proto, optional_date_to_proto, optional_user_id_to_proto, total_to_proto,
};
use super::ClientApiImpl;
use crate::impls::ApiError;

/// Home shows a strip of the library, so its media list is short and paging it
/// would be pointless.
const NEST_MEDIA_PAGE_DEFAULT: i32 = 60;

impl ClientApiImpl {
    // --------------------------------------------------------- 首页 ------

    pub async fn get_couple_nest_home(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::GetCoupleNestHomeRequest,
    ) -> Result<synctv_proto::client::GetCoupleNestHomeResponse, ApiError> {
        let space_id = self.decode_space(&req.space_id)?;
        let home = self
            .user_service
            .couple_nest_home(space_id, user_id)
            .await?;

        Ok(synctv_proto::client::GetCoupleNestHomeResponse {
            cover: home
                .cover
                .map(|media| self.media_to_proto(media))
                .transpose()?,
            next_anniversary: match (home.next_anniversary, home.next_anniversary_on) {
                (Some(anniversary), Some(on)) => Some(self.anniversary_to_proto(anniversary, on)?),
                _ => None,
            },
            recent_media: self.media_list_to_proto(home.recent_media)?,
            open_todos: self.todo_list_to_proto(home.open_todos)?,
            open_todo_count: total_to_proto(home.open_todo_count, "couple open todo")?,
            done_todo_count: total_to_proto(home.done_todo_count, "couple done todo")?,
            memory_count: total_to_proto(home.memory_count, "couple memory")?,
            media_count: total_to_proto(home.media_count, "couple media")?,
            pet: home.pet.map(|pet| self.pet_to_proto(pet)).transpose()?,
        })
    }

    // -------------------------------------------------------- 时光轴 ------

    pub async fn list_couple_memories(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::ListCoupleMemoriesRequest,
    ) -> Result<synctv_proto::client::ListCoupleMemoriesResponse, ApiError> {
        let space_id = self.decode_space(&req.space_id)?;
        let pagination = crate::impls::proto_page_params(
            req.page,
            req.page_size,
            DEFAULT_PAGE_SIZE,
            MAX_PAGE_SIZE,
        );
        let (memories, media, total) = self
            .user_service
            .list_couple_memories(space_id, user_id, pagination)
            .await?;

        let mut out = Vec::with_capacity(memories.len());
        for memory in memories {
            // The whole page's media arrived in one query; each entry takes the
            // slice that names it.
            let attached = media
                .iter()
                .filter(|item| item.memory_id == Some(memory.id))
                .cloned()
                .collect::<Vec<_>>();
            out.push(self.memory_to_proto(memory, attached)?);
        }

        Ok(synctv_proto::client::ListCoupleMemoriesResponse {
            memories: out,
            total: total_to_proto(total, "couple memory")?,
        })
    }

    pub async fn create_couple_memory(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::CreateCoupleMemoryRequest,
    ) -> Result<synctv_proto::client::CreateCoupleMemoryResponse, ApiError> {
        let space_id = self.decode_space(&req.space_id)?;
        let happened_on = optional_date_from_proto(&req.happened_on, "happenedOn")?
            .ok_or_else(|| ApiError::InvalidInput("happenedOn is required".to_string()))?;
        let memory = self
            .user_service
            .create_couple_memory(
                space_id,
                user_id,
                happened_on,
                &req.title,
                &req.content,
                &req.location,
            )
            .await?;

        Ok(synctv_proto::client::CreateCoupleMemoryResponse {
            memory: Some(self.memory_to_proto(memory, Vec::new())?),
        })
    }

    pub async fn update_couple_memory(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::UpdateCoupleMemoryRequest,
    ) -> Result<synctv_proto::client::UpdateCoupleMemoryResponse, ApiError> {
        let memory_id = self.decode_memory(&req.id)?;
        let happened_on = match req.happened_on.as_deref() {
            Some(value) => optional_date_from_proto(value, "happenedOn")?,
            None => None,
        };
        let memory = self
            .user_service
            .update_couple_memory(
                memory_id,
                user_id,
                CoupleMemoryUpdate {
                    happened_on,
                    title: req.title,
                    content: req.content,
                    location: req.location,
                },
            )
            .await?;

        Ok(synctv_proto::client::UpdateCoupleMemoryResponse {
            memory: Some(self.memory_to_proto(memory, Vec::new())?),
        })
    }

    pub async fn delete_couple_memory(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::DeleteCoupleMemoryRequest,
    ) -> Result<synctv_proto::client::DeleteCoupleMemoryResponse, ApiError> {
        let memory_id = self.decode_memory(&req.id)?;
        self.user_service
            .delete_couple_memory(memory_id, user_id)
            .await?;
        Ok(synctv_proto::client::DeleteCoupleMemoryResponse {})
    }

    // ---------------------------------------------------------- 相册 ------

    pub async fn list_couple_albums(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::ListCoupleAlbumsRequest,
    ) -> Result<synctv_proto::client::ListCoupleAlbumsResponse, ApiError> {
        let space_id = self.decode_space(&req.space_id)?;
        let albums = self
            .user_service
            .list_couple_albums(space_id, user_id)
            .await?;
        // What is not in any album still has to be reachable, so the shelf is
        // told how much is loose.
        let (_, unfiled) = self
            .user_service
            .list_couple_media(
                space_id,
                user_id,
                CoupleMediaScope::Unfiled,
                crate::impls::proto_page_params(1, 1, DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE),
            )
            .await?;

        Ok(synctv_proto::client::ListCoupleAlbumsResponse {
            albums: albums
                .into_iter()
                .map(|album| self.album_to_proto(album))
                .collect::<Result<Vec<_>, _>>()?,
            unfiled_count: total_to_proto(unfiled, "unfiled couple media")?,
        })
    }

    pub async fn create_couple_album(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::CreateCoupleAlbumRequest,
    ) -> Result<synctv_proto::client::CreateCoupleAlbumResponse, ApiError> {
        let space_id = self.decode_space(&req.space_id)?;
        let album = self
            .user_service
            .create_couple_album(space_id, user_id, &req.name)
            .await?;
        Ok(synctv_proto::client::CreateCoupleAlbumResponse {
            album: Some(self.album_to_proto(album)?),
        })
    }

    pub async fn rename_couple_album(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::RenameCoupleAlbumRequest,
    ) -> Result<synctv_proto::client::RenameCoupleAlbumResponse, ApiError> {
        let album_id = self.decode_album(&req.id)?;
        self.user_service
            .rename_couple_album(album_id, user_id, &req.name)
            .await?;
        Ok(synctv_proto::client::RenameCoupleAlbumResponse {})
    }

    pub async fn delete_couple_album(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::DeleteCoupleAlbumRequest,
    ) -> Result<synctv_proto::client::DeleteCoupleAlbumResponse, ApiError> {
        let album_id = self.decode_album(&req.id)?;
        self.user_service
            .delete_couple_album(album_id, user_id)
            .await?;
        Ok(synctv_proto::client::DeleteCoupleAlbumResponse {})
    }

    // ---------------------------------------------------------- 媒体 ------

    pub async fn list_couple_media(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::ListCoupleMediaRequest,
    ) -> Result<synctv_proto::client::ListCoupleMediaResponse, ApiError> {
        let space_id = self.decode_space(&req.space_id)?;
        // The narrowest filter wins, so a request carrying two of them is not
        // silently answered with the wrong one.
        let scope = match (req.album_id.as_deref(), req.memory_id.as_deref()) {
            (Some(album), _) => CoupleMediaScope::Album(self.decode_album(album)?),
            (None, Some(memory)) => CoupleMediaScope::Memory(self.decode_memory(memory)?),
            (None, None) if req.unfiled_only => CoupleMediaScope::Unfiled,
            (None, None) => CoupleMediaScope::All,
        };
        let page_size = if req.page_size == 0 {
            NEST_MEDIA_PAGE_DEFAULT
        } else {
            req.page_size
        };
        let (media, total) = self
            .user_service
            .list_couple_media(
                space_id,
                user_id,
                scope,
                crate::impls::proto_page_params(
                    req.page,
                    page_size,
                    DEFAULT_PAGE_SIZE,
                    MAX_PAGE_SIZE,
                ),
            )
            .await?;

        Ok(synctv_proto::client::ListCoupleMediaResponse {
            media: self.media_list_to_proto(media)?,
            total: total_to_proto(total, "couple media")?,
        })
    }

    pub async fn add_couple_media(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::AddCoupleMediaRequest,
    ) -> Result<synctv_proto::client::AddCoupleMediaResponse, ApiError> {
        let space_id = self.decode_space(&req.space_id)?;
        let album_id = req
            .album_id
            .as_deref()
            .map(|value| self.decode_album(value))
            .transpose()?;
        let memory_id = req
            .memory_id
            .as_deref()
            .map(|value| self.decode_memory(value))
            .transpose()?;
        let kind = media_kind_from_proto(req.kind)
            .ok_or_else(|| ApiError::InvalidInput("A media kind is required".to_string()))?;

        let media = self
            .user_service
            .add_couple_media(
                space_id,
                user_id,
                NewCoupleMedia {
                    album_id,
                    memory_id,
                    kind,
                    storage_backend: req.storage_backend,
                    object_key: req.object_key,
                    mime_type: req.mime_type,
                    // Zero is "not stated" on the wire; a real zero-byte object
                    // is not worth distinguishing from one.
                    size_bytes: (req.size_bytes > 0).then_some(req.size_bytes),
                    width: (req.width > 0).then_some(req.width),
                    height: (req.height > 0).then_some(req.height),
                    taken_on: optional_date_from_proto(&req.taken_on, "takenOn")?,
                    caption: req.caption,
                },
            )
            .await?;

        Ok(synctv_proto::client::AddCoupleMediaResponse {
            media: Some(self.media_to_proto(media)?),
        })
    }

    pub async fn move_couple_media(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::MoveCoupleMediaRequest,
    ) -> Result<synctv_proto::client::MoveCoupleMediaResponse, ApiError> {
        let media_id = self.decode_media(&req.id)?;
        let album_id = req
            .album_id
            .as_deref()
            .map(|value| self.decode_album(value))
            .transpose()?;
        self.user_service
            .move_couple_media(media_id, user_id, album_id)
            .await?;
        Ok(synctv_proto::client::MoveCoupleMediaResponse {})
    }

    pub async fn delete_couple_media(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::DeleteCoupleMediaRequest,
    ) -> Result<synctv_proto::client::DeleteCoupleMediaResponse, ApiError> {
        let media_id = self.decode_media(&req.id)?;
        self.user_service
            .delete_couple_media(media_id, user_id)
            .await?;
        Ok(synctv_proto::client::DeleteCoupleMediaResponse {})
    }

    pub async fn set_couple_space_cover(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::SetCoupleSpaceCoverRequest,
    ) -> Result<synctv_proto::client::SetCoupleSpaceCoverResponse, ApiError> {
        let space_id = self.decode_space(&req.space_id)?;
        let media_id = req
            .media_id
            .as_deref()
            .map(|value| self.decode_media(value))
            .transpose()?;
        self.user_service
            .set_couple_space_cover(space_id, user_id, media_id)
            .await?;
        Ok(synctv_proto::client::SetCoupleSpaceCoverResponse {})
    }

    // -------------------------------------------------------- 纪念日 ------

    pub async fn list_couple_anniversaries(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::ListCoupleAnniversariesRequest,
    ) -> Result<synctv_proto::client::ListCoupleAnniversariesResponse, ApiError> {
        let space_id = self.decode_space(&req.space_id)?;
        let anniversaries = self
            .user_service
            .list_couple_anniversaries(space_id, user_id)
            .await?;

        let today = chrono::Utc::now().date_naive();
        // Sorted by when each next comes round, not by its stored date: a
        // birthday in January is the soonest thing in December.
        let mut resolved = anniversaries
            .into_iter()
            .map(|anniversary| {
                let on = anniversary.next_occurrence(today);
                (anniversary, on)
            })
            .collect::<Vec<_>>();
        resolved.sort_by_key(|(_, on)| (*on < today, *on));

        Ok(synctv_proto::client::ListCoupleAnniversariesResponse {
            anniversaries: resolved
                .into_iter()
                .map(|(anniversary, on)| self.anniversary_to_proto(anniversary, on))
                .collect::<Result<Vec<_>, _>>()?,
        })
    }

    pub async fn create_couple_anniversary(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::CreateCoupleAnniversaryRequest,
    ) -> Result<synctv_proto::client::CreateCoupleAnniversaryResponse, ApiError> {
        let space_id = self.decode_space(&req.space_id)?;
        let happens_on = optional_date_from_proto(&req.happens_on, "happensOn")?
            .ok_or_else(|| ApiError::InvalidInput("happensOn is required".to_string()))?;
        let anniversary = self
            .user_service
            .create_couple_anniversary(
                space_id,
                user_id,
                &req.title,
                happens_on,
                req.repeat_yearly,
                &req.note,
            )
            .await?;

        let today = chrono::Utc::now().date_naive();
        let on = anniversary.next_occurrence(today);
        Ok(synctv_proto::client::CreateCoupleAnniversaryResponse {
            anniversary: Some(self.anniversary_to_proto(anniversary, on)?),
        })
    }

    pub async fn update_couple_anniversary(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::UpdateCoupleAnniversaryRequest,
    ) -> Result<synctv_proto::client::UpdateCoupleAnniversaryResponse, ApiError> {
        let anniversary_id = self.decode_anniversary(&req.id)?;
        let happens_on = match req.happens_on.as_deref() {
            Some(value) => optional_date_from_proto(value, "happensOn")?,
            None => None,
        };
        let anniversary = self
            .user_service
            .update_couple_anniversary(
                anniversary_id,
                user_id,
                CoupleAnniversaryUpdate {
                    title: req.title,
                    happens_on,
                    repeat_yearly: req.repeat_yearly,
                    note: req.note,
                },
            )
            .await?;

        let today = chrono::Utc::now().date_naive();
        let on = anniversary.next_occurrence(today);
        Ok(synctv_proto::client::UpdateCoupleAnniversaryResponse {
            anniversary: Some(self.anniversary_to_proto(anniversary, on)?),
        })
    }

    pub async fn delete_couple_anniversary(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::DeleteCoupleAnniversaryRequest,
    ) -> Result<synctv_proto::client::DeleteCoupleAnniversaryResponse, ApiError> {
        let anniversary_id = self.decode_anniversary(&req.id)?;
        self.user_service
            .delete_couple_anniversary(anniversary_id, user_id)
            .await?;
        Ok(synctv_proto::client::DeleteCoupleAnniversaryResponse {})
    }

    // ---------------------------------------------------------- 清单 ------

    pub async fn list_couple_todos(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::ListCoupleTodosRequest,
    ) -> Result<synctv_proto::client::ListCoupleTodosResponse, ApiError> {
        let space_id = self.decode_space(&req.space_id)?;
        let todos = self
            .user_service
            .list_couple_todos(space_id, user_id)
            .await?;
        Ok(synctv_proto::client::ListCoupleTodosResponse {
            todos: self.todo_list_to_proto(todos)?,
        })
    }

    pub async fn create_couple_todo(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::CreateCoupleTodoRequest,
    ) -> Result<synctv_proto::client::CreateCoupleTodoResponse, ApiError> {
        let space_id = self.decode_space(&req.space_id)?;
        let todo = self
            .user_service
            .create_couple_todo(
                space_id,
                user_id,
                &req.title,
                &req.note,
                &req.category,
                i16::try_from(req.priority).unwrap_or(0),
                optional_date_from_proto(&req.plan_date, "planDate")?,
            )
            .await?;
        Ok(synctv_proto::client::CreateCoupleTodoResponse {
            todo: Some(self.todo_to_proto(todo)?),
        })
    }

    pub async fn update_couple_todo(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::UpdateCoupleTodoRequest,
    ) -> Result<synctv_proto::client::UpdateCoupleTodoResponse, ApiError> {
        let todo_id = self.decode_todo(&req.id)?;
        // `clearPlanDate` beats a supplied date: a client that sets both means
        // to clear it, and guessing the other way would keep a date the user
        // asked to remove.
        let plan_date = if req.clear_plan_date {
            Some(None)
        } else {
            match req.plan_date.as_deref() {
                Some(value) => Some(optional_date_from_proto(value, "planDate")?),
                None => None,
            }
        };
        let todo = self
            .user_service
            .update_couple_todo(
                todo_id,
                user_id,
                CoupleTodoUpdate {
                    title: req.title,
                    note: req.note,
                    category: req.category,
                    priority: req.priority.map(|value| i16::try_from(value).unwrap_or(0)),
                    plan_date,
                    done: req.done,
                },
            )
            .await?;
        Ok(synctv_proto::client::UpdateCoupleTodoResponse {
            todo: Some(self.todo_to_proto(todo)?),
        })
    }

    pub async fn delete_couple_todo(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::DeleteCoupleTodoRequest,
    ) -> Result<synctv_proto::client::DeleteCoupleTodoResponse, ApiError> {
        let todo_id = self.decode_todo(&req.id)?;
        self.user_service
            .delete_couple_todo(todo_id, user_id)
            .await?;
        Ok(synctv_proto::client::DeleteCoupleTodoResponse {})
    }

    // ------------------------------------------------------ 宠物小屋 ------

    pub async fn get_couple_pet(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::GetCouplePetRequest,
    ) -> Result<synctv_proto::client::GetCouplePetResponse, ApiError> {
        let space_id = self.decode_space(&req.space_id)?;
        let pet = self.user_service.couple_pet(space_id, user_id).await?;
        Ok(synctv_proto::client::GetCouplePetResponse {
            pet: pet.map(|pet| self.pet_to_proto(pet)).transpose()?,
        })
    }

    pub async fn adopt_couple_pet(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::AdoptCouplePetRequest,
    ) -> Result<synctv_proto::client::AdoptCouplePetResponse, ApiError> {
        let space_id = self.decode_space(&req.space_id)?;
        let pet = self
            .user_service
            .adopt_couple_pet(
                space_id,
                user_id,
                &req.name,
                i16::try_from(req.species).unwrap_or(1),
            )
            .await?;
        Ok(synctv_proto::client::AdoptCouplePetResponse {
            pet: Some(self.pet_to_proto(pet)?),
        })
    }

    pub async fn interact_with_couple_pet(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::InteractWithCouplePetRequest,
    ) -> Result<synctv_proto::client::InteractWithCouplePetResponse, ApiError> {
        let space_id = self.decode_space(&req.space_id)?;
        let action = pet_action_from_proto(req.action)
            .ok_or_else(|| ApiError::InvalidInput("An action is required".to_string()))?;
        let pet = self
            .user_service
            .interact_with_couple_pet(space_id, user_id, action)
            .await?;
        Ok(synctv_proto::client::InteractWithCouplePetResponse {
            pet: Some(self.pet_to_proto(pet)?),
        })
    }

    pub async fn update_couple_pet(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::UpdateCouplePetRequest,
    ) -> Result<synctv_proto::client::UpdateCouplePetResponse, ApiError> {
        let space_id = self.decode_space(&req.space_id)?;
        let pet = self
            .user_service
            .update_couple_pet(
                space_id,
                user_id,
                synctv_core::models::CouplePetUpdate {
                    name: req.name,
                    species: req.species.map(|value| i16::try_from(value).unwrap_or(1)),
                    skin: req.skin.map(|value| i16::try_from(value).unwrap_or(0)),
                    accessory: req.accessory.map(|value| i16::try_from(value).unwrap_or(0)),
                },
            )
            .await?;
        Ok(synctv_proto::client::UpdateCouplePetResponse {
            pet: Some(self.pet_to_proto(pet)?),
        })
    }

    pub async fn get_couple_pet_history(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::GetCouplePetHistoryRequest,
    ) -> Result<synctv_proto::client::GetCouplePetHistoryResponse, ApiError> {
        let space_id = self.decode_space(&req.space_id)?;
        let days = self
            .user_service
            .couple_pet_history(space_id, user_id)
            .await?;
        Ok(synctv_proto::client::GetCouplePetHistoryResponse {
            days: days
                .into_iter()
                .map(|day| {
                    Ok(synctv_proto::client::CouplePetDay {
                        on_date: optional_date_to_proto(Some(day.on_date)),
                        members: total_to_proto(day.members, "couple pet check-in member")?,
                        interactions: total_to_proto(
                            day.interactions,
                            "couple pet check-in interaction",
                        )?,
                    })
                })
                .collect::<Result<Vec<_>, ApiError>>()?,
        })
    }

    // -------------------------------------------------------- 心动 AI ------

    pub async fn list_couple_ai_works(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::ListCoupleAiWorksRequest,
    ) -> Result<synctv_proto::client::ListCoupleAiWorksResponse, ApiError> {
        let space_id = self.decode_space(&req.space_id)?;
        let (works, total) = self
            .user_service
            .list_couple_ai_works(
                space_id,
                user_id,
                ai_kind_from_proto(req.kind),
                crate::impls::proto_page_params(
                    req.page,
                    req.page_size,
                    DEFAULT_PAGE_SIZE,
                    MAX_PAGE_SIZE,
                ),
            )
            .await?;
        Ok(synctv_proto::client::ListCoupleAiWorksResponse {
            works: works
                .into_iter()
                .map(|work| self.ai_work_to_proto(work))
                .collect::<Result<Vec<_>, _>>()?,
            total: total_to_proto(total, "couple ai work")?,
        })
    }

    pub async fn save_couple_ai_work(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::SaveCoupleAiWorkRequest,
    ) -> Result<synctv_proto::client::SaveCoupleAiWorkResponse, ApiError> {
        let space_id = self.decode_space(&req.space_id)?;
        let kind = ai_kind_from_proto(req.kind)
            .ok_or_else(|| ApiError::InvalidInput("A kind is required".to_string()))?;
        // An unparseable prompt is kept as an empty object rather than refused:
        // the piece is the thing worth saving, and losing it over its metadata
        // would be the wrong trade.
        let prompt = serde_json::from_str::<serde_json::Value>(&req.prompt_json)
            .ok()
            .filter(serde_json::Value::is_object)
            .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));

        let work = self
            .user_service
            .save_couple_ai_work(space_id, user_id, kind, &req.title, prompt, &req.content)
            .await?;
        Ok(synctv_proto::client::SaveCoupleAiWorkResponse {
            work: Some(self.ai_work_to_proto(work)?),
        })
    }

    pub async fn delete_couple_ai_work(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::DeleteCoupleAiWorkRequest,
    ) -> Result<synctv_proto::client::DeleteCoupleAiWorkResponse, ApiError> {
        let work_id = self.decode_ai_work(&req.id)?;
        self.user_service
            .delete_couple_ai_work(work_id, user_id)
            .await?;
        Ok(synctv_proto::client::DeleteCoupleAiWorkResponse {})
    }

    // ------------------------------------------------- media objects ----

    /// Stores a photo or video and files it into the nest in one call.
    ///
    /// Object uploads elsewhere go through a chunked session because they can
    /// be gigabytes. A couple photo is a few megabytes, and a session handshake
    /// for that is three round trips to save nothing — so this takes the bytes
    /// directly, under a body limit the route enforces.
    #[allow(clippy::too_many_arguments)]
    pub async fn upload_couple_media_object(
        &self,
        user_id: &UserId,
        space_id: &str,
        kind: i32,
        mime_type: &str,
        data: Vec<u8>,
        album_id: Option<&str>,
        memory_id: Option<&str>,
        caption: &str,
        taken_on: &str,
        width: i32,
        height: i32,
    ) -> Result<synctv_proto::client::CoupleMedia, ApiError> {
        let space = self.decode_space(space_id)?;
        let kind = media_kind_from_proto(kind)
            .ok_or_else(|| ApiError::InvalidInput("A media kind is required".to_string()))?;
        if data.is_empty() {
            return Err(ApiError::InvalidInput("The upload is empty".to_string()));
        }
        let storage = crate::impls::stored_files::first_file_storage([
            self.user_service.file_storage_service(),
            self.room_service.file_storage_service(),
        ])
        .ok_or_else(|| ApiError::Internal("no file storage is configured".to_string()))?;

        let backend = storage.backend_name().to_string();
        // Namespaced per space so a listing of the bucket cannot mix two
        // couples together, and random so a key cannot be guessed from a
        // neighbouring one.
        let object_key = format!("couples/{space}/{}", uuid::Uuid::new_v4());
        let size_bytes = i64::try_from(data.len()).unwrap_or(i64::MAX);
        storage
            .put_object_by_key(
                &backend,
                &object_key,
                mime_type,
                data,
                synctv_core::models::FileMetadata::default(),
            )
            .await
            .map_err(ApiError::from)?;

        let media = self
            .user_service
            .add_couple_media(
                space,
                user_id,
                NewCoupleMedia {
                    album_id: album_id.map(|value| self.decode_album(value)).transpose()?,
                    memory_id: memory_id
                        .map(|value| self.decode_memory(value))
                        .transpose()?,
                    kind,
                    storage_backend: backend,
                    object_key,
                    mime_type: mime_type.to_string(),
                    size_bytes: Some(size_bytes),
                    width: (width > 0).then_some(width),
                    height: (height > 0).then_some(height),
                    taken_on: optional_date_from_proto(taken_on, "takenOn")?,
                    caption: caption.to_string(),
                },
            )
            .await?;
        self.media_to_proto(media)
    }

    /// Streams one item back, once the caller is shown to be in its space.
    pub async fn couple_media_object_download(
        &self,
        user_id: &UserId,
        media_id: &str,
    ) -> Result<synctv_core::models::FileObjectDownload, ApiError> {
        let media_id = self.decode_media(media_id)?;
        let media = self.user_service.couple_media(media_id, user_id).await?;
        let storage = crate::impls::stored_files::first_file_storage([
            self.user_service.file_storage_service(),
            self.room_service.file_storage_service(),
        ])
        .ok_or_else(|| ApiError::Internal("no file storage is configured".to_string()))?;

        let blob = storage
            .get_object_by_key(&media.storage_backend, &media.object_key)
            .await
            .map_err(ApiError::from)?;
        // The blob is already whole in memory, so the "stream" is one chunk.
        // Going through FileObjectDownload anyway keeps the HTTP layer on the
        // single response helper every other object route uses.
        let data = blob.data;
        Ok(synctv_core::models::FileObjectDownload {
            metadata: synctv_core::models::FileObjectMetadata {
                storage_backend: blob.storage_backend,
                object_key: blob.object_key,
                mime_type: blob.mime_type,
                size_bytes: blob.size_bytes,
                total_size_bytes: blob.total_size_bytes,
                content_manifest_sha256: blob.content_manifest_sha256,
                compression: blob.compression,
                range: blob.range,
                metadata: blob.metadata,
                created_at: blob.created_at,
            },
            stream: Box::pin(futures::stream::once(async move { Ok(data) })),
        })
    }

    // ------------------------------------------------------ conversion ----

    fn memory_to_proto(
        &self,
        memory: CoupleMemory,
        media: Vec<CoupleMedia>,
    ) -> Result<synctv_proto::client::CoupleMemory, ApiError> {
        Ok(synctv_proto::client::CoupleMemory {
            id: self
                .public_id_codec
                .encode_couple_memory_id(memory.id)
                .map_err(ApiError::Internal)?,
            author_user_id: optional_user_id_to_proto(
                memory.author_user_id,
                &self.public_id_codec,
            )?,
            happened_on: optional_date_to_proto(Some(memory.happened_on)),
            title: memory.title,
            content: memory.content,
            location: memory.location,
            created_at: memory.created_at.timestamp(),
            updated_at: memory.updated_at.timestamp(),
            media: self.media_list_to_proto(media)?,
        })
    }

    fn album_to_proto(
        &self,
        album: CoupleAlbum,
    ) -> Result<synctv_proto::client::CoupleAlbum, ApiError> {
        Ok(synctv_proto::client::CoupleAlbum {
            id: self
                .public_id_codec
                .encode_couple_album_id(album.id)
                .map_err(ApiError::Internal)?,
            created_by_user_id: optional_user_id_to_proto(
                album.created_by_user_id,
                &self.public_id_codec,
            )?,
            name: album.name,
            created_at: album.created_at.timestamp(),
            media_count: total_to_proto(album.media_count, "couple album media")?,
            cover: album
                .cover
                .map(|media| self.media_to_proto(media))
                .transpose()?,
        })
    }

    fn media_list_to_proto(
        &self,
        media: Vec<CoupleMedia>,
    ) -> Result<Vec<synctv_proto::client::CoupleMedia>, ApiError> {
        media
            .into_iter()
            .map(|item| self.media_to_proto(item))
            .collect()
    }

    fn media_to_proto(
        &self,
        media: CoupleMedia,
    ) -> Result<synctv_proto::client::CoupleMedia, ApiError> {
        let id = self
            .public_id_codec
            .encode_couple_media_id(media.id)
            .map_err(ApiError::Internal)?;
        Ok(synctv_proto::client::CoupleMedia {
            id: id.clone(),
            album_id: media
                .album_id
                .map(|id| self.public_id_codec.encode_couple_album_id(id))
                .transpose()
                .map_err(ApiError::Internal)?
                .unwrap_or_default(),
            memory_id: media
                .memory_id
                .map(|id| self.public_id_codec.encode_couple_memory_id(id))
                .transpose()
                .map_err(ApiError::Internal)?
                .unwrap_or_default(),
            uploader_user_id: optional_user_id_to_proto(
                media.uploader_user_id,
                &self.public_id_codec,
            )?,
            kind: media_kind_to_proto(media.kind),
            storage_backend: media.storage_backend,
            object_key: media.object_key,
            mime_type: media.mime_type,
            size_bytes: media.size_bytes.unwrap_or_default(),
            width: media.width.unwrap_or_default(),
            height: media.height.unwrap_or_default(),
            taken_on: optional_date_to_proto(media.taken_on),
            caption: media.caption,
            created_at: media.created_at.timestamp(),
            url: couple_media_object_path(&id),
        })
    }

    fn anniversary_to_proto(
        &self,
        anniversary: CoupleAnniversary,
        next_on: chrono::NaiveDate,
    ) -> Result<synctv_proto::client::CoupleAnniversary, ApiError> {
        let today = chrono::Utc::now().date_naive();
        Ok(synctv_proto::client::CoupleAnniversary {
            id: self
                .public_id_codec
                .encode_couple_anniversary_id(anniversary.id)
                .map_err(ApiError::Internal)?,
            created_by_user_id: optional_user_id_to_proto(
                anniversary.created_by_user_id,
                &self.public_id_codec,
            )?,
            title: anniversary.title,
            happens_on: optional_date_to_proto(Some(anniversary.happens_on)),
            repeat_yearly: anniversary.repeat_yearly,
            note: anniversary.note,
            created_at: anniversary.created_at.timestamp(),
            updated_at: anniversary.updated_at.timestamp(),
            next_occurrence_on: optional_date_to_proto(Some(next_on)),
            // Negative once a one-off has passed, which is what lets a client
            // render it as history rather than as an impossible countdown.
            days_until: i32::try_from((next_on - today).num_days()).unwrap_or(0),
        })
    }

    fn todo_list_to_proto(
        &self,
        todos: Vec<CoupleTodo>,
    ) -> Result<Vec<synctv_proto::client::CoupleTodo>, ApiError> {
        todos
            .into_iter()
            .map(|todo| self.todo_to_proto(todo))
            .collect()
    }

    fn todo_to_proto(
        &self,
        todo: CoupleTodo,
    ) -> Result<synctv_proto::client::CoupleTodo, ApiError> {
        Ok(synctv_proto::client::CoupleTodo {
            id: self
                .public_id_codec
                .encode_couple_todo_id(todo.id)
                .map_err(ApiError::Internal)?,
            created_by_user_id: optional_user_id_to_proto(
                todo.created_by_user_id,
                &self.public_id_codec,
            )?,
            title: todo.title,
            note: todo.note,
            category: todo.category,
            priority: i32::from(todo.priority),
            plan_date: optional_date_to_proto(todo.plan_date),
            done_by_user_id: optional_user_id_to_proto(
                todo.done_by_user_id,
                &self.public_id_codec,
            )?,
            done_at: todo.done_at.map(|at| at.timestamp()).unwrap_or_default(),
            created_at: todo.created_at.timestamp(),
            updated_at: todo.updated_at.timestamp(),
        })
    }

    fn pet_to_proto(
        &self,
        view: CouplePetView,
    ) -> Result<synctv_proto::client::CouplePet, ApiError> {
        Ok(synctv_proto::client::CouplePet {
            name: view.pet.name,
            species: i32::from(view.pet.species),
            skin: i32::from(view.pet.skin),
            accessory: i32::from(view.pet.accessory),
            stage: i32::from(view.pet.stage),
            experience: view.pet.experience,
            // The decayed values, not the stored ones.
            hunger: i32::from(view.hunger),
            mood: i32::from(view.mood),
            streak_days: total_to_proto(view.streak_days, "couple pet streak")?,
            both_checked_in_today: view.both_checked_in_today,
            experience_into_stage: view.experience_into_stage,
            experience_for_next_stage: view.experience_for_next_stage,
            created_at: view.pet.created_at.timestamp(),
        })
    }

    fn ai_work_to_proto(
        &self,
        work: CoupleAiWork,
    ) -> Result<synctv_proto::client::CoupleAiWork, ApiError> {
        Ok(synctv_proto::client::CoupleAiWork {
            id: self
                .public_id_codec
                .encode_couple_ai_work_id(work.id)
                .map_err(ApiError::Internal)?,
            author_user_id: optional_user_id_to_proto(work.author_user_id, &self.public_id_codec)?,
            kind: ai_kind_to_proto(work.kind),
            title: work.title,
            prompt_json: work.prompt.to_string(),
            content: work.content,
            created_at: work.created_at.timestamp(),
        })
    }

    // ---------------------------------------------------------- ids ------

    fn decode_space(&self, value: &str) -> Result<i64, ApiError> {
        self.public_id_codec
            .decode_couple_space_id(value)
            .map_err(ApiError::InvalidInput)
    }

    fn decode_memory(&self, value: &str) -> Result<i64, ApiError> {
        self.public_id_codec
            .decode_couple_memory_id(value)
            .map_err(ApiError::InvalidInput)
    }

    fn decode_album(&self, value: &str) -> Result<i64, ApiError> {
        self.public_id_codec
            .decode_couple_album_id(value)
            .map_err(ApiError::InvalidInput)
    }

    fn decode_media(&self, value: &str) -> Result<i64, ApiError> {
        self.public_id_codec
            .decode_couple_media_id(value)
            .map_err(ApiError::InvalidInput)
    }

    fn decode_anniversary(&self, value: &str) -> Result<i64, ApiError> {
        self.public_id_codec
            .decode_couple_anniversary_id(value)
            .map_err(ApiError::InvalidInput)
    }

    fn decode_todo(&self, value: &str) -> Result<i64, ApiError> {
        self.public_id_codec
            .decode_couple_todo_id(value)
            .map_err(ApiError::InvalidInput)
    }

    fn decode_ai_work(&self, value: &str) -> Result<i64, ApiError> {
        self.public_id_codec
            .decode_couple_ai_work_id(value)
            .map_err(ApiError::InvalidInput)
    }
}

/// Where a client fetches one item's bytes. Relative, so it works against
/// whichever address the app reached this server on.
fn couple_media_object_path(media_id: &str) -> String {
    format!("/api/user/couple/nest/media-objects/{media_id}")
}

fn media_kind_to_proto(kind: CoupleMediaKind) -> i32 {
    match kind {
        CoupleMediaKind::Photo => synctv_proto::client::CoupleMediaKind::Photo as i32,
        CoupleMediaKind::Video => synctv_proto::client::CoupleMediaKind::Video as i32,
    }
}

fn media_kind_from_proto(kind: i32) -> Option<CoupleMediaKind> {
    match synctv_proto::client::CoupleMediaKind::try_from(kind) {
        Ok(synctv_proto::client::CoupleMediaKind::Photo) => Some(CoupleMediaKind::Photo),
        Ok(synctv_proto::client::CoupleMediaKind::Video) => Some(CoupleMediaKind::Video),
        _ => None,
    }
}

fn ai_kind_to_proto(kind: CoupleAiKind) -> i32 {
    match kind {
        CoupleAiKind::DatePlan => synctv_proto::client::CoupleAiKind::DatePlan as i32,
        CoupleAiKind::Diary => synctv_proto::client::CoupleAiKind::Diary as i32,
        CoupleAiKind::LoveLetter => synctv_proto::client::CoupleAiKind::LoveLetter as i32,
    }
}

fn ai_kind_from_proto(kind: i32) -> Option<CoupleAiKind> {
    match synctv_proto::client::CoupleAiKind::try_from(kind) {
        Ok(synctv_proto::client::CoupleAiKind::DatePlan) => Some(CoupleAiKind::DatePlan),
        Ok(synctv_proto::client::CoupleAiKind::Diary) => Some(CoupleAiKind::Diary),
        Ok(synctv_proto::client::CoupleAiKind::LoveLetter) => Some(CoupleAiKind::LoveLetter),
        _ => None,
    }
}

fn pet_action_from_proto(action: i32) -> Option<CouplePetAction> {
    match synctv_proto::client::CouplePetAction::try_from(action) {
        Ok(synctv_proto::client::CouplePetAction::Feed) => Some(CouplePetAction::Feed),
        Ok(synctv_proto::client::CouplePetAction::Play) => Some(CouplePetAction::Play),
        Ok(synctv_proto::client::CouplePetAction::Pet) => Some(CouplePetAction::Pet),
        _ => None,
    }
}
