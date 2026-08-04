use super::{FrontendEndpoint, FrontendHttpMethod, FrontendPathParam};

pub const SPEC_CATALOG_PATH: &str = "/api/specs/catalog";
pub const SPEC_READ_PATH: &str = "/api/specs/read";
pub const SPEC_RESOLVE_SOURCE_PATH: &str = "/api/specs/resolve-source";
pub const PROJECT_SPEC_SOURCES_PATH: &str = "/api/projects/{projectId}/spec-sources";
pub const SPEC_WATCH_PATH: &str = "/api/specs/watch";

const PROJECT_ID_PATH_PARAM: FrontendPathParam = FrontendPathParam {
    rust_field_name: "project_id",
    wire_name: "projectId",
};
const PROJECT_PATH_PARAMS: &[FrontendPathParam] = &[PROJECT_ID_PATH_PARAM];
const NO_PATH_PARAMS: &[FrontendPathParam] = &[];

pub(super) const SPEC_ENDPOINTS: &[FrontendEndpoint] = &[
    FrontendEndpoint {
        operation_name: "getSpecCatalog",
        namespace: "spec",
        member_name: "catalog",
        method: FrontendHttpMethod::Post,
        path_template: SPEC_CATALOG_PATH,
        request_type: "GetSpecCatalogRequest",
        response_type: "SpecCatalogResponse",
        path_params: NO_PATH_PARAMS,
        has_json_body: true,
    },
    FrontendEndpoint {
        operation_name: "readSpec",
        namespace: "spec",
        member_name: "read",
        method: FrontendHttpMethod::Post,
        path_template: SPEC_READ_PATH,
        request_type: "ReadSpecRequest",
        response_type: "ReadSpecResponse",
        path_params: NO_PATH_PARAMS,
        has_json_body: true,
    },
    FrontendEndpoint {
        operation_name: "resolveSpecSource",
        namespace: "spec",
        member_name: "resolveSource",
        method: FrontendHttpMethod::Post,
        path_template: SPEC_RESOLVE_SOURCE_PATH,
        request_type: "ResolveSpecSourceRequest",
        response_type: "ResolveSpecSourceResponse",
        path_params: NO_PATH_PARAMS,
        has_json_body: true,
    },
    FrontendEndpoint {
        operation_name: "updateProjectSpecSources",
        namespace: "spec",
        member_name: "updateProjectSources",
        method: FrontendHttpMethod::Put,
        path_template: PROJECT_SPEC_SOURCES_PATH,
        request_type: "UpdateProjectSpecSourcesRequest",
        response_type: "UpdateProjectSpecSourcesResponse",
        path_params: PROJECT_PATH_PARAMS,
        has_json_body: true,
    },
    FrontendEndpoint {
        operation_name: "watchSpecs",
        namespace: "spec",
        member_name: "watch",
        method: FrontendHttpMethod::Post,
        path_template: SPEC_WATCH_PATH,
        request_type: "WatchSpecsRequest",
        response_type: "WorkspaceFileEventBatch",
        path_params: NO_PATH_PARAMS,
        has_json_body: true,
    },
];
