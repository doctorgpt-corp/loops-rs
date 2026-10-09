use serde_json::{Map, Value};

use super::{EmailFormat, EmailMessageUpdate, LoopsClient, PreviewOptions, check};
use crate::error::{LoopsError, Result};
use crate::generated::{
    CompleteUploadRequest, CompleteUploadResponse, CompleteUploadResponseEnum, CreateUploadRequest,
    CreateUploadRequestParams, CreateUploadResponse, CreateUploadResponseEnum,
    EmailMessageGuardianResponse, EmailMessagePreviewResponse, EmailMessageResponse,
    GetEmailMessageGuardianRequest, GetEmailMessageGuardianResponse, GetEmailMessageRequest,
    GetEmailMessageResponse, PreviewEmailMessageRequest, PreviewEmailMessageResponse,
    UpdateEmailMessageRequest, UpdateEmailMessageRequestParams, UpdateEmailMessageResponse,
};
use crate::routes::Route;

impl LoopsClient {
    /// Get an email message.
    pub async fn get_email_message(&self, email_message_id: &str) -> Result<EmailMessageResponse> {
        const ACTION: &str = "get email message";
        let raw = self
            .send(Route::GetEmailMessage, &[email_message_id], |r| r)
            .await?;
        match raw
            .parse(ACTION, GetEmailMessageRequest::parse_response)
            .await?
        {
            GetEmailMessageResponse::Ok(message) => Ok(message),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Update an email message. Only the fields set on `update` change.
    ///
    /// # Errors
    ///
    /// [`LoopsError::UnsafeEmail`] when safe mode is on and the CC or BCC
    /// address is outside the allowed domains: those addresses receive every
    /// email sent from the message. [`LoopsError::Api`] with status 409 when
    /// `expected_revision_id` is stale.
    pub async fn update_email_message(
        &self,
        email_message_id: &str,
        update: EmailMessageUpdate,
    ) -> Result<EmailMessageResponse> {
        const ACTION: &str = "update email message";
        self.validate_email(update.cc_email.as_deref())?;
        self.validate_email(update.bcc_email.as_deref())?;
        let body = UpdateEmailMessageRequest {
            expected_revision_id: update.expected_revision_id,
            subject: update.subject,
            preview_text: update.preview_text,
            from_name: update.from_name,
            from_email: update.from_email,
            reply_to_email: update.reply_to_email,
            cc_email: update.cc_email,
            bcc_email: update.bcc_email,
            language_code: update.language_code,
            email_format: update.email_format.map(|format| match format {
                EmailFormat::Plain => crate::generated::EmailFormat::Plain,
                EmailFormat::Styled => crate::generated::EmailFormat::Styled,
            }),
            lmx: update.lmx,
            contact_properties_fallbacks: update
                .contact_properties_fallbacks
                .map(|m| m.into_iter().collect()),
            event_properties_fallbacks: update
                .event_properties_fallbacks
                .map(|m| m.into_iter().collect()),
            data_variables_fallbacks: update
                .data_variables_fallbacks
                .map(|m| m.into_iter().collect()),
        };
        let raw = self
            .send(Route::UpdateEmailMessage, &[email_message_id], |r| {
                r.json(&body)
            })
            .await?;
        match raw
            .parse(ACTION, UpdateEmailMessageRequestParams::parse_response)
            .await?
        {
            UpdateEmailMessageResponse::Ok(message) => Ok(message),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Run Guardian content checks against a saved email message: missing or
    /// invalid links, unsupported properties, malformed buttons, missing
    /// required data variables and the like. MJML messages are not supported.
    pub async fn get_email_message_guardian(
        &self,
        email_message_id: &str,
    ) -> Result<EmailMessageGuardianResponse> {
        const ACTION: &str = "get email message guardian checks";
        let raw = self
            .send(Route::GetEmailMessageGuardian, &[email_message_id], |r| r)
            .await?;
        match raw
            .parse(ACTION, GetEmailMessageGuardianRequest::parse_response)
            .await?
        {
            GetEmailMessageGuardianResponse::Ok(report) => Ok(report),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Send a preview of an email message to one or more addresses, each
    /// checked against safe mode.
    pub async fn preview_email_message(
        &self,
        email_message_id: &str,
        emails: &[&str],
        options: PreviewOptions,
    ) -> Result<EmailMessagePreviewResponse> {
        const ACTION: &str = "preview email message";
        for email in emails {
            self.validate_email(Some(email))?;
        }
        if emails.is_empty() {
            return Err(LoopsError::InvalidInput(
                "preview_email_message needs at least one recipient".to_owned(),
            ));
        }
        // The generated body only allows string values; Loops renders
        // numbers and booleans too, so the body is built as JSON.
        let mut body = Map::new();
        body.insert("emails".to_owned(), Value::from(emails.to_vec()));
        for (key, values) in [
            ("contactProperties", options.contact_properties),
            ("eventProperties", options.event_properties),
            ("dataVariables", options.data_variables),
        ] {
            if !values.is_empty() {
                body.insert(key.to_owned(), Value::Object(values));
            }
        }
        let raw = self
            .send(Route::PreviewEmailMessage, &[email_message_id], |r| {
                r.json(&body)
            })
            .await?;
        match raw
            .parse(ACTION, PreviewEmailMessageRequest::parse_response)
            .await?
        {
            PreviewEmailMessageResponse::Ok(preview) => Ok(preview),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Start an asset upload. Upload the file to the returned presigned URL,
    /// then call [`complete_upload`](Self::complete_upload).
    pub async fn create_upload(
        &self,
        content_type: &str,
        content_length: u64,
    ) -> Result<CreateUploadResponse> {
        const ACTION: &str = "create upload";
        let body = CreateUploadRequest {
            content_type: content_type.to_owned(),
            content_length: i64::try_from(content_length).map_err(|_| {
                LoopsError::InvalidInput(format!("content_length {content_length} is too large"))
            })?,
        };
        check(&body)?;
        let raw = self
            .send(Route::CreateUpload, &[], |r| r.json(&body))
            .await?;
        match raw
            .parse(ACTION, CreateUploadRequestParams::parse_response)
            .await?
        {
            CreateUploadResponseEnum::Ok(upload) => Ok(upload),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Mark an upload as complete.
    ///
    /// Loops reports an exceeded upload limit as HTTP 429, which surfaces as
    /// [`LoopsError::RateLimit`].
    pub async fn complete_upload(&self, upload_id: &str) -> Result<CompleteUploadResponse> {
        const ACTION: &str = "complete upload";
        let raw = self
            .send(Route::CompleteUpload, &[upload_id], |r| r)
            .await?;
        match raw
            .parse(ACTION, CompleteUploadRequest::parse_response)
            .await?
        {
            CompleteUploadResponseEnum::Ok(upload) => Ok(upload),
            _ => Err(raw.unexpected(ACTION)),
        }
    }
}
