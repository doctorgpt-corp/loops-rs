use serde_json::{Map, Value};

use super::{LoopsClient, Page, check};
use crate::error::{LoopsError, Result};
use crate::generated::{
    ChangeWorkflowMailingListRequestParams, ChangeWorkflowMailingListResponse,
    ChangeWorkflowMailingListResponseEnum, CreateWorkflowRequest, CreateWorkflowRequestParams,
    CreateWorkflowResponse, DeleteWorkflowRequestParams, DeleteWorkflowResponse,
    GetWorkflowRequest, GetWorkflowResponse, ListWorkflowsRequest, ListWorkflowsRequestQuery,
    ListWorkflowsResponse, ListWorkflowsResponseEnum, SimplifiedWorkflow,
    UpdateWorkflowPropertiesRequestParams, UpdateWorkflowPropertiesResponse,
    WorkflowQueuedContactPolicy,
};
use crate::routes::Route;

impl LoopsClient {
    /// List workflows.
    pub async fn list_workflows(&self, page: Page) -> Result<ListWorkflowsResponse> {
        const ACTION: &str = "list workflows";
        let query = ListWorkflowsRequestQuery {
            per_page: page.per_page_param(),
            cursor: page.cursor,
        };
        let raw = self
            .send(Route::ListWorkflows, &[], |r| r.query(&query))
            .await?;
        match raw
            .parse(ACTION, ListWorkflowsRequest::parse_response)
            .await?
        {
            ListWorkflowsResponseEnum::Ok(workflows) => Ok(workflows),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Get a workflow with its node graph.
    pub async fn get_workflow(&self, workflow_id: &str) -> Result<SimplifiedWorkflow> {
        const ACTION: &str = "get workflow";
        let raw = self.send(Route::GetWorkflow, &[workflow_id], |r| r).await?;
        match raw
            .parse(ACTION, GetWorkflowRequest::parse_response)
            .await?
        {
            GetWorkflowResponse::Ok(workflow) => Ok(workflow),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Create a draft workflow with a blank trigger and an exit node. Build
    /// out its graph with the workflow node methods.
    ///
    /// Without `workflow_group_id`, Loops files the workflow in the team's
    /// Unsorted group.
    pub async fn create_workflow(
        &self,
        name: &str,
        description: Option<&str>,
        mailing_list_id: Option<&str>,
        workflow_group_id: Option<&str>,
    ) -> Result<SimplifiedWorkflow> {
        const ACTION: &str = "create workflow";
        let body = CreateWorkflowRequest {
            name: name.to_owned(),
            description: description.map(str::to_owned),
            mailing_list_id: mailing_list_id.map(str::to_owned),
            workflow_group_id: workflow_group_id.map(str::to_owned),
        };
        check(&body)?;
        let raw = self
            .send(Route::CreateWorkflow, &[], |r| r.json(&body))
            .await?;
        match raw
            .parse(ACTION, CreateWorkflowRequestParams::parse_response)
            .await?
        {
            CreateWorkflowResponse::Ok(workflow) => Ok(workflow),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Rename a workflow, change its description, and/or move it to another
    /// workflow group. At least one of the three must be `Some`, or the call
    /// fails with [`LoopsError::InvalidInput`] before any request is sent.
    ///
    /// `expected_revision_id` is the optimistic concurrency token: pass the
    /// latest `workflow_revision_id`, or `None` for a workflow that has none
    /// yet. A stale value fails with 409.
    pub async fn update_workflow(
        &self,
        workflow_id: &str,
        expected_revision_id: Option<&str>,
        name: Option<&str>,
        description: Option<&str>,
        workflow_group_id: Option<&str>,
    ) -> Result<SimplifiedWorkflow> {
        const ACTION: &str = "update workflow";
        if name.is_none() && description.is_none() && workflow_group_id.is_none() {
            return Err(LoopsError::InvalidInput(
                "update_workflow requires name, description or workflow_group_id".to_owned(),
            ));
        }
        let mut body = revision_body(expected_revision_id);
        if let Some(name) = name {
            body.insert("name".to_owned(), Value::from(name));
        }
        if let Some(description) = description {
            body.insert("description".to_owned(), Value::from(description));
        }
        if let Some(workflow_group_id) = workflow_group_id {
            body.insert("workflowGroupId".to_owned(), Value::from(workflow_group_id));
        }
        let raw = self
            .send(Route::UpdateWorkflow, &[workflow_id], |r| r.json(&body))
            .await?;
        match raw
            .parse(
                ACTION,
                UpdateWorkflowPropertiesRequestParams::parse_response,
            )
            .await?
        {
            UpdateWorkflowPropertiesResponse::Ok(workflow) => Ok(workflow),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Set (`Some`) or clear (`None`) the mailing list of a workflow.
    ///
    /// With `dry_run`, or when contacts queued in the workflow would be
    /// excluded by the new list, Loops answers with a preview instead of
    /// applying the change; retry with
    /// [`WorkflowQueuedContactPolicy::Discard`] to apply it and discard them.
    pub async fn change_workflow_mailing_list(
        &self,
        workflow_id: &str,
        expected_revision_id: Option<&str>,
        mailing_list_id: Option<&str>,
        dry_run: Option<bool>,
        queued_contact_policy: Option<WorkflowQueuedContactPolicy>,
    ) -> Result<ChangeWorkflowMailingListResponse> {
        const ACTION: &str = "change workflow mailing list";
        let mut body = revision_body(expected_revision_id);
        body.insert(
            "mailingListId".to_owned(),
            mailing_list_id.map_or(Value::Null, Value::from),
        );
        if let Some(dry_run) = dry_run {
            body.insert("dryRun".to_owned(), Value::Bool(dry_run));
        }
        if let Some(policy) = queued_contact_policy {
            body.insert(
                "queuedContactPolicy".to_owned(),
                queued_contact_policy_value(&policy),
            );
        }
        let raw = self
            .send(Route::ChangeWorkflowMailingList, &[workflow_id], |r| {
                r.json(&body)
            })
            .await?;
        match raw
            .parse(
                ACTION,
                ChangeWorkflowMailingListRequestParams::parse_response,
            )
            .await?
        {
            ChangeWorkflowMailingListResponseEnum::Ok(response) => Ok(response),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Delete a workflow. Returns `true` once deleted.
    ///
    /// A workflow that is sending or still has queued contacts is not deleted
    /// on the first attempt: Loops answers 409 saying what stands in the way.
    /// Retry with `confirm_delete: Some(true)` to delete it, stop sending and
    /// cancel the queued contacts.
    pub async fn delete_workflow(
        &self,
        workflow_id: &str,
        expected_revision_id: Option<&str>,
        confirm_delete: Option<bool>,
    ) -> Result<bool> {
        const ACTION: &str = "delete workflow";
        let mut body = revision_body(expected_revision_id);
        if let Some(confirm) = confirm_delete {
            body.insert("confirmDelete".to_owned(), Value::Bool(confirm));
        }
        let raw = self
            .send(Route::DeleteWorkflow, &[workflow_id], |r| r.json(&body))
            .await?;
        match raw
            .parse(ACTION, DeleteWorkflowRequestParams::parse_response)
            .await?
        {
            DeleteWorkflowResponse::NoContent => Ok(true),
            _ => Err(raw.unexpected(ACTION)),
        }
    }
}

/// A body carrying `expectedRevisionId`, which Loops requires on every
/// workflow mutation and accepts as `null` for workflows without a revision
/// yet. The generated models drop a `None` instead of sending `null`, so
/// these bodies are built as JSON.
pub(super) fn revision_body(expected_revision_id: Option<&str>) -> Map<String, Value> {
    let mut body = Map::new();
    body.insert(
        "expectedRevisionId".to_owned(),
        expected_revision_id.map_or(Value::Null, Value::from),
    );
    body
}

pub(super) fn queued_contact_policy_value(policy: &WorkflowQueuedContactPolicy) -> Value {
    Value::from(match policy {
        WorkflowQueuedContactPolicy::Fail => "fail",
        WorkflowQueuedContactPolicy::Discard => "discard",
    })
}
