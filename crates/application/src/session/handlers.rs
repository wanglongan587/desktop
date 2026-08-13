use crate::session::mapper::map_session;
use crate::session::ports::SessionRepository;
use crate::{ApplicationError, Clock};
use ora_contracts::{
    DeleteSessionRequest, DeleteSessionResponse, GetSessionRequest, GetSessionResponse,
    ListSessionsRequest, ListSessionsResponse,
};
use ora_domain::SessionId;

/// Handles one session lookup without depending on transport-specific concerns.
pub struct GetSessionHandler<Repository> {
    repository: Repository,
}

impl<Repository> GetSessionHandler<Repository> {
    pub fn new(repository: Repository) -> Self {
        Self { repository }
    }
}

impl<Repository> GetSessionHandler<Repository>
where
    Repository: SessionRepository,
{
    /// Loads one visible session or returns a stable not-found application error.
    pub fn handle(
        &self,
        request: GetSessionRequest,
    ) -> Result<GetSessionResponse, ApplicationError> {
        let session_id = SessionId::new(request.session_id);
        match self
            .repository
            .find_session(&session_id)
            .map_err(ApplicationError::from_session_repository_error)?
        {
            Some(session) => Ok(GetSessionResponse {
                session: map_session(session),
            }),
            None => Err(ApplicationError::SessionNotFound {
                session_id: session_id.to_string(),
            }),
        }
    }
}

/// Handles session listing without depending on transport-specific concerns.
pub struct ListSessionsHandler<Repository> {
    repository: Repository,
}

impl<Repository> ListSessionsHandler<Repository> {
    pub fn new(repository: Repository) -> Self {
        Self { repository }
    }
}

impl<Repository> ListSessionsHandler<Repository>
where
    Repository: SessionRepository,
{
    /// Lists every visible session and maps each one into the shared contract view.
    pub fn handle(
        &self,
        _request: ListSessionsRequest,
    ) -> Result<ListSessionsResponse, ApplicationError> {
        let sessions = self
            .repository
            .list_sessions()
            .map_err(ApplicationError::from_session_repository_error)?;
        Ok(ListSessionsResponse {
            sessions: sessions.into_iter().map(map_session).collect(),
        })
    }
}

/// Handles Ora-only session deletion without deleting provider-owned history.
pub struct DeleteSessionHandler<Repository, ClockSource> {
    repository: Repository,
    clock: ClockSource,
}

impl<Repository, ClockSource> DeleteSessionHandler<Repository, ClockSource> {
    pub fn new(repository: Repository, clock: ClockSource) -> Self {
        Self { repository, clock }
    }
}

impl<Repository, ClockSource> DeleteSessionHandler<Repository, ClockSource>
where
    Repository: SessionRepository,
    ClockSource: Clock,
{
    /// Soft-deletes one stopped Ora session record.
    pub fn handle(
        &self,
        request: DeleteSessionRequest,
    ) -> Result<DeleteSessionResponse, ApplicationError> {
        let session_id = SessionId::new(request.session_id);
        let deleted = self
            .repository
            .soft_delete_session(&session_id, self.clock.now_timestamp_millis())
            .map_err(ApplicationError::from_session_repository_error)?;
        if deleted {
            Ok(DeleteSessionResponse {
                session_id: session_id.to_string(),
            })
        } else {
            Err(ApplicationError::SessionNotFound {
                session_id: session_id.to_string(),
            })
        }
    }
}
