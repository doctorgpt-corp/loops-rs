use http::StatusCode;
use serde_json::Value;

use super::{LoopsClient, Page, TransactionalOptions, check};
use crate::error::{LoopsError, Result};
use crate::generated::{
    CreateTransactionalEmailRequest, CreateTransactionalEmailResponse, CreateTransactionalRequest,
    EnsureTransactionalDraftRequest, EnsureTransactionalDraftResponse,
    GetTransactionalEmailMetricsRequest, GetTransactionalEmailMetricsResponse,
    GetTransactionalEmailRequest, GetTransactionalEmailResponse,
    ListPublishedTransactionalEmailsRequestQuery, ListTransactionalEmailsRequest,
    ListTransactionalEmailsRequestQuery, ListTransactionalEmailsResponse,
    ListTransactionalsResourceResponse, PublishTransactionalEmailRequest,
    PublishTransactionalEmailResponse, SendTransactionalEmailRequest,
    SendTransactionalEmailRequestHeader, SendTransactionalEmailResponse,
    TransactionalDraftResponse, TransactionalMetricsResponse, TransactionalRequest,
    TransactionalResource, TransactionalSuccessResponse, UpdateTransactionalEmailRequest,
    UpdateTransactionalRequest,
};
use crate::responses::TransactionalEmailsResponse;
use crate::routes::Route;

impl LoopsClient {
    /// Send a transactional email.
    ///
    /// An idempotency key is generated when `options` does not carry one.
    ///
    /// # Errors
    ///
    /// [`LoopsError::Api`] with status 404 when the transactional email does
    /// not exist, 409 when the idempotency key was already used, and 400 for
    /// other rejections such as an unpublished template.
    pub async fn send_transactional_email(
        &self,
        transactional_id: &str,
        email: &str,
        options: TransactionalOptions,
    ) -> Result<TransactionalSuccessResponse> {
        const ACTION: &str = "send transactional email";
        self.validate_email(Some(email))?;
        let header = SendTransactionalEmailRequestHeader {
            idempotency_key: Some(
                options
                    .idempotency_key
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            ),
        };
        check(&header)?;
        let headers = http::HeaderMap::try_from(&header)
            .map_err(|err| LoopsError::InvalidInput(format!("Invalid idempotency key: {err}")))?;
        let body = TransactionalRequest {
            email: email.to_owned(),
            transactional_id: transactional_id.to_owned(),
            add_to_audience: options.add_to_audience,
            data_variables: (!options.data_variables.is_empty())
                .then_some(Value::Object(options.data_variables)),
            attachments: (!options.attachments.is_empty()).then_some(options.attachments),
        };
        check(&body)?;
        let raw = self
            .send(Route::Transactional, &[], |r| {
                r.headers(headers).json(&body)
            })
            .await?;
        match raw.status {
            StatusCode::NOT_FOUND => return Err(raw.failure_with("Transactional email not found")),
            StatusCode::CONFLICT => return Err(raw.failure_with("Idempotency key conflict")),
            _ => {}
        }
        match raw
            .parse(ACTION, SendTransactionalEmailRequest::parse_response)
            .await?
        {
            SendTransactionalEmailResponse::Ok(response) => Ok(response),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// List published transactional emails.
    pub async fn list_transactional_emails(
        &self,
        page: Page,
    ) -> Result<TransactionalEmailsResponse> {
        let query = ListPublishedTransactionalEmailsRequestQuery {
            per_page: page.per_page_param(),
            cursor: page.cursor,
        };
        let raw = self
            .send(Route::ListTransactional, &[], |r| r.query(&query))
            .await?;
        raw.parse_as("list transactional emails")
    }

    /// List transactional email templates, drafts included.
    pub async fn list_transactional_templates(
        &self,
        page: Page,
    ) -> Result<ListTransactionalsResourceResponse> {
        const ACTION: &str = "list transactional templates";
        let query = ListTransactionalEmailsRequestQuery {
            per_page: page.per_page_param(),
            cursor: page.cursor,
        };
        let raw = self
            .send(Route::ListTransactionalTemplates, &[], |r| r.query(&query))
            .await?;
        match raw
            .parse(ACTION, ListTransactionalEmailsRequest::parse_response)
            .await?
        {
            ListTransactionalEmailsResponse::Ok(response) => Ok(response),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Get a transactional email template.
    pub async fn get_transactional_template(
        &self,
        transactional_id: &str,
    ) -> Result<TransactionalResource> {
        const ACTION: &str = "get transactional template";
        let raw = self
            .send(Route::GetTransactionalTemplate, &[transactional_id], |r| r)
            .await?;
        match raw
            .parse(ACTION, GetTransactionalEmailRequest::parse_response)
            .await?
        {
            GetTransactionalEmailResponse::Ok(template) => Ok(template),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// The names of the data variables a transactional email template
    /// expects, for building the `data_variables` of
    /// [`send_transactional_email`](Self::send_transactional_email). Empty for
    /// unpublished templates.
    pub async fn get_transactional_variables(&self, transactional_id: &str) -> Result<Vec<String>> {
        Ok(self
            .get_transactional_template(transactional_id)
            .await?
            .data_variables)
    }

    /// All-time delivery metrics of a transactional email template, totalled
    /// across its published versions.
    pub async fn get_transactional_template_metrics(
        &self,
        transactional_id: &str,
    ) -> Result<TransactionalMetricsResponse> {
        const ACTION: &str = "get transactional template metrics";
        let raw = self
            .send(
                Route::GetTransactionalTemplateMetrics,
                &[transactional_id],
                |r| r,
            )
            .await?;
        match raw
            .parse(ACTION, GetTransactionalEmailMetricsRequest::parse_response)
            .await?
        {
            GetTransactionalEmailMetricsResponse::Ok(metrics) => Ok(metrics),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Create a draft transactional email template.
    pub async fn create_transactional_template(
        &self,
        name: &str,
        transactional_group_id: Option<&str>,
    ) -> Result<TransactionalDraftResponse> {
        const ACTION: &str = "create transactional template";
        let body = CreateTransactionalRequest {
            name: name.to_owned(),
            transactional_group_id: transactional_group_id.map(str::to_owned),
        };
        check(&body)?;
        let raw = self
            .send(Route::CreateTransactionalTemplate, &[], |r| r.json(&body))
            .await?;
        match raw
            .parse(ACTION, CreateTransactionalEmailRequest::parse_response)
            .await?
        {
            CreateTransactionalEmailResponse::Created(draft) => Ok(draft),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Rename a transactional email template or move it to another group.
    pub async fn update_transactional_template(
        &self,
        transactional_id: &str,
        name: Option<&str>,
        transactional_group_id: Option<&str>,
    ) -> Result<TransactionalResource> {
        const ACTION: &str = "update transactional template";
        let body = UpdateTransactionalRequest {
            name: name.map(str::to_owned),
            transactional_group_id: transactional_group_id.map(str::to_owned),
        };
        let raw = self
            .send(
                Route::UpdateTransactionalTemplate,
                &[transactional_id],
                |r| r.json(&body),
            )
            .await?;
        match raw
            .parse(ACTION, UpdateTransactionalEmailRequest::parse_response)
            .await?
        {
            GetTransactionalEmailResponse::Ok(template) => Ok(template),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Create the draft of a transactional email template, or return the
    /// existing one.
    pub async fn draft_transactional_template(
        &self,
        transactional_id: &str,
    ) -> Result<TransactionalDraftResponse> {
        const ACTION: &str = "draft transactional template";
        let raw = self
            .send(
                Route::DraftTransactionalTemplate,
                &[transactional_id],
                |r| r,
            )
            .await?;
        match raw
            .parse(ACTION, EnsureTransactionalDraftRequest::parse_response)
            .await?
        {
            EnsureTransactionalDraftResponse::Ok(draft) => Ok(draft),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Publish the draft of a transactional email template.
    pub async fn publish_transactional_template(
        &self,
        transactional_id: &str,
    ) -> Result<TransactionalResource> {
        const ACTION: &str = "publish transactional template";
        let raw = self
            .send(
                Route::PublishTransactionalTemplate,
                &[transactional_id],
                |r| r,
            )
            .await?;
        match raw
            .parse(ACTION, PublishTransactionalEmailRequest::parse_response)
            .await?
        {
            PublishTransactionalEmailResponse::Ok(template) => Ok(template),
            _ => Err(raw.unexpected(ACTION)),
        }
    }
}
