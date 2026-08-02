use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Declares one discovery rule that groups spec documents under a display name.
///
/// The glob is part of the public payload because the chat view has to recognize a spec
/// the moment an agent writes one, before the backend index has observed the change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "spec.ts")]
pub struct SpecSource {
    pub name: String,
    pub glob: String,
}

/// Describes one spec document surfaced in the catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "spec.ts")]
pub struct SpecDocument {
    pub id: String,
    pub source_name: String,
    pub path: String,
    pub title: String,
}

/// Selects the workspace whose specs should be listed.
///
/// Omitting `task_id` targets the project root. Supplying it targets that task's
/// workspace, which for worktree-backed tasks is a different branch with a different set
/// of spec files.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "spec.ts")]
pub struct ListSpecsRequest {
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub task_id: Option<String>,
}

/// Returns the resolved workspace, its configured sources, and every discovered document.
///
/// Sources are returned in configuration order so the catalog can group without inventing
/// its own ordering, and are returned even when empty so the chat view can match paths
/// against their globs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "spec.ts")]
pub struct ListSpecsResponse {
    pub workspace_root: String,
    pub sources: Vec<SpecSource>,
    pub specs: Vec<SpecDocument>,
}

/// Selects one spec document by its workspace-relative path.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "spec.ts")]
pub struct ReadSpecRequest {
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub task_id: Option<String>,
    pub path: String,
}

/// Returns one spec document together with its raw markdown body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "spec.ts")]
pub struct ReadSpecResponse {
    pub spec: SpecDocument,
    pub content: String,
}

/// Exports every TypeScript binding declared in this module into the target directory.
pub(crate) fn export(config: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    SpecSource::export(config)?;
    SpecDocument::export(config)?;
    ListSpecsRequest::export(config)?;
    ListSpecsResponse::export(config)?;
    ReadSpecRequest::export(config)?;
    ReadSpecResponse::export(config)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ListSpecsRequest, ListSpecsResponse, ReadSpecRequest, ReadSpecResponse, SpecDocument,
        SpecSource,
    };
    use pretty_assertions::assert_eq;
    use serde_json::json;

    /// Verifies spec contracts serialize into the camel-cased payloads the catalog consumes.
    #[test]
    fn serializes_spec_contracts() {
        let document = SpecDocument {
            id: "add-auth".to_string(),
            source_name: "OpenSpec".to_string(),
            path: "openspec/changes/add-auth/proposal.md".to_string(),
            title: "Add authentication".to_string(),
        };

        assert_eq!(
            serde_json::to_value(ListSpecsResponse {
                workspace_root: "/workspace/ora".to_string(),
                sources: vec![SpecSource {
                    name: "OpenSpec".to_string(),
                    glob: "openspec/changes/**/*.md".to_string(),
                }],
                specs: vec![document.clone()],
            })
            .unwrap(),
            json!({
                "workspaceRoot": "/workspace/ora",
                "sources": [{ "name": "OpenSpec", "glob": "openspec/changes/**/*.md" }],
                "specs": [{
                    "id": "add-auth",
                    "sourceName": "OpenSpec",
                    "path": "openspec/changes/add-auth/proposal.md",
                    "title": "Add authentication",
                }],
            })
        );
        assert_eq!(
            serde_json::to_value(ReadSpecResponse {
                spec: document,
                content: "# Add authentication\n".to_string(),
            })
            .unwrap(),
            json!({
                "spec": {
                    "id": "add-auth",
                    "sourceName": "OpenSpec",
                    "path": "openspec/changes/add-auth/proposal.md",
                    "title": "Add authentication",
                },
                "content": "# Add authentication\n",
            })
        );
    }

    /// Verifies the optional task scope disappears from the wire when the project root is targeted.
    #[test]
    fn omits_absent_task_scope() {
        assert_eq!(
            serde_json::to_value(ListSpecsRequest {
                project_id: "project-1".to_string(),
                task_id: None,
            })
            .unwrap(),
            json!({ "projectId": "project-1" })
        );
        assert_eq!(
            serde_json::to_value(ReadSpecRequest {
                project_id: "project-1".to_string(),
                task_id: Some("task-1".to_string()),
                path: "docs/specs/design.md".to_string(),
            })
            .unwrap(),
            json!({
                "projectId": "project-1",
                "taskId": "task-1",
                "path": "docs/specs/design.md",
            })
        );
    }
}
