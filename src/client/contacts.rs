use http::StatusCode;
use serde_json::{Map, Value};

use super::{
    ContactFields, ContactId, LoopsClient, check, mailing_lists_value, merge_properties,
    single_identifier,
};
use crate::error::{LoopsError, Result};
use crate::generated::{
    ContactProperty, ContactPropertyCreateRequest, ContactPropertyCreateRequestType,
    ContactRequest, ContactSuccessResponse, ContactSuppressionRemoveResponse,
    ContactSuppressionStatusResponse, ContactUpdateRequest, CreateContactRequest,
    CreateContactResponse, FindContactRequestQuery, GetContactSuppressionRequest,
    GetContactSuppressionRequestQuery, GetContactSuppressionResponse, ListContactPropertiesRequest,
    ListContactPropertiesResponse, ListDedicatedSendingIpsRequest, ListDedicatedSendingIpsResponse,
    ListMailingListsRequest, ListMailingListsResponse, MailingList,
    RemoveContactSuppressionRequest, RemoveContactSuppressionRequestQuery,
    RemoveContactSuppressionResponse, UpdateContactRequest, UpdateContactResponse,
};
use crate::responses::Contact;
use crate::routes::Route;

impl LoopsClient {
    /// Create a contact.
    ///
    /// Fails with [`LoopsError::ContactExists`] when a contact with this email
    /// or user ID already exists; use [`upsert_contact`](Self::upsert_contact)
    /// to create or update.
    pub async fn create_contact(
        &self,
        email: &str,
        user_id: Option<&str>,
        fields: ContactFields,
    ) -> Result<ContactSuccessResponse> {
        const ACTION: &str = "create contact";
        self.validate_email(Some(email))?;
        let request = ContactRequest {
            email: email.to_owned(),
            first_name: fields.first_name,
            last_name: fields.last_name,
            source: fields.source,
            subscribed: fields.subscribed,
            user_group: fields.user_group,
            user_id: user_id.map(str::to_owned),
            mailing_lists: mailing_lists_value(&fields.mailing_lists),
            additional_properties: indexmap::IndexMap::new(),
        };
        check(&request)?;
        let body = with_properties(&request, fields.custom_properties, ACTION)?;
        let raw = self
            .send(Route::CreateContact, &[], |r| r.json(&body))
            .await?;
        if raw.status == StatusCode::CONFLICT {
            return Err(LoopsError::ContactExists {
                message: raw
                    .message()
                    .unwrap_or_else(|| "A contact with this email already exists".to_owned()),
            });
        }
        match raw
            .parse(ACTION, CreateContactRequest::parse_response)
            .await?
        {
            CreateContactResponse::Ok(response) => Ok(response),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Create a contact, or update the one matching `contact`.
    ///
    /// To change a contact's email address the contact needs a user ID: pass
    /// [`ContactId::Both`] and Loops matches on either value.
    pub async fn upsert_contact(
        &self,
        contact: ContactId,
        fields: ContactFields,
    ) -> Result<ContactSuccessResponse> {
        const ACTION: &str = "upsert contact";
        self.validate_email(contact.email_address())?;
        let request = ContactUpdateRequest {
            email: contact.email_address().map(str::to_owned),
            first_name: fields.first_name,
            last_name: fields.last_name,
            source: fields.source,
            subscribed: fields.subscribed,
            user_group: fields.user_group,
            user_id: contact.user_id_value().map(str::to_owned),
            mailing_lists: mailing_lists_value(&fields.mailing_lists),
            additional_properties: indexmap::IndexMap::new(),
        };
        let body = with_properties(&request, fields.custom_properties, ACTION)?;
        let raw = self
            .send(Route::UpsertContact, &[], |r| r.json(&body))
            .await?;
        match raw
            .parse(ACTION, UpdateContactRequest::parse_response)
            .await?
        {
            UpdateContactResponse::Ok(response) => Ok(response),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Find a contact by email or user ID.
    ///
    /// Returns `None` when Loops answers the lookup with a failure (400 or
    /// 404) rather than raising, and `Some` with the matches otherwise. An
    /// unknown contact is usually `Some(vec![])`. The contacts carry their
    /// custom properties in [`Contact::properties`].
    ///
    /// # Errors
    ///
    /// [`LoopsError::InvalidInput`] for [`ContactId::Both`]: Loops looks up
    /// by exactly one identifier.
    pub async fn find_contact(&self, contact: ContactId) -> Result<Option<Vec<Contact>>> {
        const ACTION: &str = "find contact";
        single_identifier(&contact, ACTION)?;
        self.validate_email(contact.email_address())?;
        let query = FindContactRequestQuery {
            email: contact.email_address().map(str::to_owned),
            user_id: contact.user_id_value().map(str::to_owned),
        };
        let raw = self
            .send(Route::FindContact, &[], |r| r.query(&query))
            .await?;
        if matches!(raw.status, StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND) {
            return Ok(None);
        }
        // The spec's Contact schema omits custom properties although the
        // endpoint returns them, so the hand-written model keeps them.
        raw.parse_as(ACTION).map(Some)
    }

    /// Delete a contact by email or user ID.
    ///
    /// Returns `true` once deleted and `false` when Loops answers with a
    /// failure (400 or 404), e.g. because no such contact exists.
    ///
    /// # Errors
    ///
    /// [`LoopsError::InvalidInput`] for [`ContactId::Both`]: Loops deletes by
    /// exactly one identifier.
    pub async fn delete_contact(&self, contact: ContactId) -> Result<bool> {
        const ACTION: &str = "delete contact";
        let body = match &contact {
            ContactId::Email(email) => serde_json::json!({ "email": email }),
            ContactId::UserId(user_id) => serde_json::json!({ "userId": user_id }),
            ContactId::Both { .. } => {
                return Err(LoopsError::InvalidInput(
                    "Provide either email or user_id, not both".to_owned(),
                ));
            }
        };
        self.validate_email(contact.email_address())?;
        let raw = self
            .send(Route::DeleteContact, &[], |r| r.json(&body))
            .await?;
        if matches!(raw.status, StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND) {
            return Ok(false);
        }
        raw.parse(
            ACTION,
            crate::generated::DeleteContactRequest::parse_response,
        )
        .await
        .map(|response| matches!(response, crate::generated::DeleteContactResponse::Ok(_)))
    }

    /// Create a custom contact property.
    ///
    /// Returns the response body as JSON, since Loops answers with more
    /// fields than the spec documents.
    pub async fn create_contact_property(
        &self,
        name: &str,
        property_type: ContactPropertyCreateRequestType,
    ) -> Result<Value> {
        const ACTION: &str = "create contact property";
        let body = ContactPropertyCreateRequest {
            name: name.to_owned(),
            r#type: property_type,
        };
        check(&body)?;
        let raw = self
            .send(Route::CreateContactProperty, &[], |r| r.json(&body))
            .await?;
        raw.parse_json(ACTION)
    }

    /// List every contact property, default and custom.
    pub async fn list_contact_properties(&self) -> Result<Vec<ContactProperty>> {
        const ACTION: &str = "list contact properties";
        let raw = self.send(Route::ListContactProperties, &[], |r| r).await?;
        match raw
            .parse(ACTION, ListContactPropertiesRequest::parse_response)
            .await?
        {
            ListContactPropertiesResponse::Ok(properties) => Ok(properties),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// List the team's mailing lists.
    pub async fn list_mailing_lists(&self) -> Result<Vec<MailingList>> {
        const ACTION: &str = "list mailing lists";
        let raw = self.send(Route::ListMailingLists, &[], |r| r).await?;
        match raw
            .parse(ACTION, ListMailingListsRequest::parse_response)
            .await?
        {
            ListMailingListsResponse::Ok(lists) => Ok(lists),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// List Loops' dedicated sending IP addresses.
    pub async fn list_sending_ips(&self) -> Result<Vec<String>> {
        const ACTION: &str = "list sending IPs";
        let raw = self.send(Route::ListSendingIps, &[], |r| r).await?;
        match raw
            .parse(ACTION, ListDedicatedSendingIpsRequest::parse_response)
            .await?
        {
            ListDedicatedSendingIpsResponse::Ok(ips) => Ok(ips),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Get a contact's suppression status and the remaining removal quota.
    ///
    /// # Errors
    ///
    /// [`LoopsError::InvalidInput`] for [`ContactId::Both`].
    pub async fn get_contact_suppression(
        &self,
        contact: ContactId,
    ) -> Result<ContactSuppressionStatusResponse> {
        const ACTION: &str = "get suppression status";
        single_identifier(&contact, ACTION)?;
        let query = GetContactSuppressionRequestQuery {
            email: contact.email_address().map(str::to_owned),
            user_id: contact.user_id_value().map(str::to_owned),
        };
        let raw = self
            .send(Route::GetContactSuppression, &[], |r| r.query(&query))
            .await?;
        match raw
            .parse(ACTION, GetContactSuppressionRequest::parse_response)
            .await?
        {
            GetContactSuppressionResponse::Ok(status) => Ok(status),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Remove a contact from the suppression list.
    ///
    /// # Errors
    ///
    /// [`LoopsError::InvalidInput`] for [`ContactId::Both`].
    pub async fn remove_contact_suppression(
        &self,
        contact: ContactId,
    ) -> Result<ContactSuppressionRemoveResponse> {
        const ACTION: &str = "remove suppression";
        single_identifier(&contact, ACTION)?;
        let query = RemoveContactSuppressionRequestQuery {
            email: contact.email_address().map(str::to_owned),
            user_id: contact.user_id_value().map(str::to_owned),
        };
        let raw = self
            .send(Route::RemoveContactSuppression, &[], |r| r.query(&query))
            .await?;
        match raw
            .parse(ACTION, RemoveContactSuppressionRequest::parse_response)
            .await?
        {
            RemoveContactSuppressionResponse::Ok(response) => Ok(response),
            _ => Err(raw.unexpected(ACTION)),
        }
    }
}

/// Serialize a generated contact body and merge custom properties into it as
/// top-level keys. The generated model only allows string-valued extras,
/// while Loops accepts strings, numbers, booleans and dates.
fn with_properties(
    request: &impl serde::Serialize,
    properties: Map<String, Value>,
    action: &str,
) -> Result<Value> {
    let Value::Object(mut body) = serde_json::to_value(request)
        .map_err(|err| LoopsError::InvalidInput(format!("Could not serialize contact: {err}")))?
    else {
        unreachable!("contact requests serialize to JSON objects");
    };
    merge_properties(&mut body, properties, CONTACT_FIELDS, action)?;
    Ok(Value::Object(body))
}

/// Keys the typed contact fields own; custom properties may not reuse them.
const CONTACT_FIELDS: &[&str] = &[
    "email",
    "firstName",
    "lastName",
    "source",
    "subscribed",
    "userGroup",
    "userId",
    "mailingLists",
];
