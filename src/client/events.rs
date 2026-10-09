use http::StatusCode;
use serde_json::{Map, Value};

use super::{ContactId, EventOptions, LoopsClient, check, mailing_lists_value, merge_properties};
use crate::error::Result;
use crate::generated::{
    EventSuccessResponse, SendEventRequest, SendEventRequestHeader, SendEventResponse,
};
use crate::routes::Route;

impl LoopsClient {
    /// Send an event to trigger workflows and emails for a contact.
    ///
    /// An idempotency key is generated when `options` does not carry one.
    pub async fn send_event(
        &self,
        event_name: &str,
        contact: ContactId,
        options: EventOptions,
    ) -> Result<EventSuccessResponse> {
        const ACTION: &str = "send event";
        self.validate_email(contact.email_address())?;
        let header = SendEventRequestHeader {
            idempotency_key: Some(
                options
                    .idempotency_key
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            ),
        };
        check(&header)?;
        let headers = http::HeaderMap::try_from(&header).map_err(|err| {
            crate::LoopsError::InvalidInput(format!("Invalid idempotency key: {err}"))
        })?;

        // The spec models this body as a oneOf over email/userId with contact
        // properties as extra top-level keys, which the generator leaves as raw JSON.
        let mut body = Map::new();
        body.insert("eventName".to_owned(), Value::from(event_name));
        if let Some(email) = contact.email_address() {
            body.insert("email".to_owned(), Value::from(email));
        }
        if let Some(user_id) = contact.user_id_value() {
            body.insert("userId".to_owned(), Value::from(user_id));
        }
        if !options.event_properties.is_empty() {
            body.insert(
                "eventProperties".to_owned(),
                Value::Object(options.event_properties),
            );
        }
        if let Some(lists) = mailing_lists_value(&options.mailing_lists) {
            body.insert("mailingLists".to_owned(), lists);
        }
        merge_properties(
            &mut body,
            options.contact_properties,
            &[
                "eventName",
                "email",
                "userId",
                "eventProperties",
                "mailingLists",
            ],
            ACTION,
        )?;

        let raw = self
            .send(Route::SendEvent, &[], |r| r.headers(headers).json(&body))
            .await?;
        if raw.status == StatusCode::CONFLICT {
            return Err(raw.failure_with("Idempotency key conflict"));
        }
        match raw.parse(ACTION, SendEventRequest::parse_response).await? {
            SendEventResponse::Ok(response) => Ok(response),
            _ => Err(raw.unexpected(ACTION)),
        }
    }
}
