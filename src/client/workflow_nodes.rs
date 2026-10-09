//! Workflow node endpoints.
//!
//! Node payloads are polymorphic on `typeName`, and the generated models
//! flatten those unions badly (a node read keeps only its revision ID), so
//! node reads and node mutations return the response body as JSON.

use serde_json::Value;

use super::params::node_type_name;
use super::workflows::{queued_contact_policy_value, revision_body};
use super::{DeleteNodeOptions, LoopsClient, NodePlacement};
use crate::error::Result;
use crate::generated::{
    AddWorkflowBranchRequestParams, AddWorkflowBranchResponse, AddWorkflowBranchResponseEnum,
    CreateWorkflowNodeTypeName, DeleteWorkflowNodeRecursivelyRequest,
    DeleteWorkflowNodeRequestParams, DeleteWorkflowNodeResponse, DeleteWorkflowNodeResponseEnum,
    EmailMetricsResponse, GetWorkflowNodeMetricsRequest, GetWorkflowNodeMetricsResponse,
};
use crate::routes::Route;

impl LoopsClient {
    /// Get a workflow node as JSON; its shape depends on `typeName`.
    pub async fn get_workflow_node(&self, workflow_id: &str, node_id: &str) -> Result<Value> {
        let raw = self
            .send(Route::GetWorkflowNode, &[workflow_id, node_id], |r| r)
            .await?;
        raw.parse_json("get workflow node")
    }

    /// All-time engagement metrics of a `SendEmailAction` node. Any other
    /// node type fails with 400; a node that has not sent yet reports zeros.
    pub async fn get_workflow_node_metrics(
        &self,
        workflow_id: &str,
        node_id: &str,
    ) -> Result<EmailMetricsResponse> {
        const ACTION: &str = "get workflow node metrics";
        let raw = self
            .send(
                Route::GetWorkflowNodeMetrics,
                &[workflow_id, node_id],
                |r| r,
            )
            .await?;
        match raw
            .parse(ACTION, GetWorkflowNodeMetricsRequest::parse_response)
            .await?
        {
            GetWorkflowNodeMetricsResponse::Ok(metrics) => Ok(metrics),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Add a node to a workflow at `placement`. The node is created with
    /// default settings; configure it with
    /// [`update_workflow_node`](Self::update_workflow_node).
    ///
    /// Returns the response as JSON: the created `node` and the updated
    /// `workflow`.
    pub async fn create_workflow_node(
        &self,
        workflow_id: &str,
        node_type: CreateWorkflowNodeTypeName,
        expected_revision_id: Option<&str>,
        placement: NodePlacement,
    ) -> Result<Value> {
        let mut body = revision_body(expected_revision_id);
        body.insert("nodeTypeName".to_owned(), node_type_name(&node_type));
        let (mode, from, to) = match placement {
            NodePlacement::Between {
                from_node_id,
                to_node_id,
            } => ("between", Some(from_node_id), Some(to_node_id)),
            NodePlacement::Before { to_node_id } => ("before", None, Some(to_node_id)),
            NodePlacement::After { from_node_id } => ("after", Some(from_node_id), None),
        };
        body.insert("insertMode".to_owned(), Value::from(mode));
        if let Some(from) = from {
            body.insert("fromNodeId".to_owned(), Value::from(from));
        }
        if let Some(to) = to {
            body.insert("toNodeId".to_owned(), Value::from(to));
        }
        let raw = self
            .send(Route::CreateWorkflowNode, &[workflow_id], |r| r.json(&body))
            .await?;
        raw.parse_json("create workflow node")
    }

    /// Update a node's settings. `payload` holds the node-type-specific
    /// fields, e.g. `{"amount": 2, "unit": "d"}` for a `TimerAction`.
    ///
    /// Returns the response as JSON: the updated node's fields plus the
    /// latest `workflow`.
    pub async fn update_workflow_node(
        &self,
        workflow_id: &str,
        node_id: &str,
        expected_revision_id: Option<&str>,
        payload: Value,
    ) -> Result<Value> {
        let mut body = revision_body(expected_revision_id);
        body.insert("payload".to_owned(), payload);
        let raw = self
            .send(Route::UpdateWorkflowNode, &[workflow_id, node_id], |r| {
                r.json(&body)
            })
            .await?;
        raw.parse_json("update workflow node")
    }

    /// Add an unconfigured branch to a `BranchNode` or `ExperimentBranchNode`.
    pub async fn add_workflow_branch(
        &self,
        workflow_id: &str,
        node_id: &str,
        expected_revision_id: Option<&str>,
    ) -> Result<AddWorkflowBranchResponse> {
        const ACTION: &str = "add workflow branch";
        let body = revision_body(expected_revision_id);
        let raw = self
            .send(Route::AddWorkflowBranch, &[workflow_id, node_id], |r| {
                r.json(&body)
            })
            .await?;
        match raw
            .parse(ACTION, AddWorkflowBranchRequestParams::parse_response)
            .await?
        {
            AddWorkflowBranchResponseEnum::Ok(response) => Ok(response),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Move the single outgoing connection of `node_id` to
    /// `new_target_node_id`. Branch nodes have several outputs and cannot be
    /// rerouted this way; the old target must keep another incoming
    /// connection.
    ///
    /// Returns the response as JSON: the updated source node plus the latest
    /// `workflow`.
    pub async fn reroute_workflow_node_connection(
        &self,
        workflow_id: &str,
        node_id: &str,
        expected_revision_id: Option<&str>,
        new_target_node_id: &str,
    ) -> Result<Value> {
        let mut body = revision_body(expected_revision_id);
        body.insert(
            "newTargetNodeId".to_owned(),
            Value::from(new_target_node_id),
        );
        let raw = self
            .send(Route::RerouteNodeConnection, &[workflow_id, node_id], |r| {
                r.json(&body)
            })
            .await?;
        raw.parse_json("reroute workflow node connection")
    }

    /// Delete a node, or with [`DeleteNodeOptions::recursive`] the node and
    /// everything downstream of it.
    ///
    /// With `dry_run`, or when contacts are queued at the node, Loops answers
    /// with a preview instead of deleting.
    pub async fn delete_workflow_node(
        &self,
        workflow_id: &str,
        node_id: &str,
        expected_revision_id: Option<&str>,
        options: DeleteNodeOptions,
    ) -> Result<DeleteWorkflowNodeResponse> {
        const ACTION: &str = "delete workflow node";
        let mut body = revision_body(expected_revision_id);
        if let Some(dry_run) = options.dry_run {
            body.insert("dryRun".to_owned(), Value::Bool(dry_run));
        }
        if let Some(policy) = options.queued_contact_policy {
            body.insert(
                "queuedContactPolicy".to_owned(),
                queued_contact_policy_value(&policy),
            );
        }
        let route = if options.recursive {
            Route::DeleteWorkflowNodeRecursively
        } else {
            Route::DeleteWorkflowNode
        };
        let raw = self
            .send(route, &[workflow_id, node_id], |r| r.json(&body))
            .await?;
        let parsed = if options.recursive {
            raw.parse(ACTION, DeleteWorkflowNodeRecursivelyRequest::parse_response)
                .await?
        } else {
            raw.parse(ACTION, DeleteWorkflowNodeRequestParams::parse_response)
                .await?
        };
        match parsed {
            DeleteWorkflowNodeResponseEnum::Ok(response) => Ok(response),
            _ => Err(raw.unexpected(ACTION)),
        }
    }
}
