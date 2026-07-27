use ora_application::{ApplicationError, GetSessionHandler, ListSessionsHandler};
use ora_contracts::{
    GetSessionRequest, GetSessionResponse, ListSessionsRequest, ListSessionsResponse,
};
use ora_db::{RepositoryPool, SqliteSessionRepository};

/// Groups persisted session query handlers; runtime mutations live in agent_runtime.
pub(crate) struct SessionApi {
    get: GetSessionHandler<SqliteSessionRepository>,
    list: ListSessionsHandler<SqliteSessionRepository>,
}

impl SessionApi {
    /// Builds session handlers from the shared repository pool.
    pub(crate) fn new(pool: RepositoryPool) -> Self {
        let repository = SqliteSessionRepository::new(pool);
        Self {
            get: GetSessionHandler::new(repository.clone()),
            list: ListSessionsHandler::new(repository),
        }
    }

    /// Executes one session lookup through the application handler.
    pub(crate) fn get(
        &self,
        request: GetSessionRequest,
    ) -> Result<GetSessionResponse, ApplicationError> {
        self.get.handle(request)
    }

    /// Executes session listing through the application handler.
    pub(crate) fn list(
        &self,
        request: ListSessionsRequest,
    ) -> Result<ListSessionsResponse, ApplicationError> {
        self.list.handle(request)
    }
}
