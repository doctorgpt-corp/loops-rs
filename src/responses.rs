//! Hand-written response models for endpoints whose generated model is too
//! loose to be useful.

use serde::Deserialize;
use serde_json::{Map, Value};

/// Pagination block of [`TransactionalEmailsResponse`].
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct TransactionalEmailPagination {
    /// Total number of published transactional emails.
    pub total_results: u64,
    /// Number of results in this page.
    pub returned_results: u64,
    /// Page size.
    pub per_page: u64,
    /// Total number of pages.
    pub total_pages: u64,
    /// Cursor for the next page, when there is one.
    #[serde(default)]
    pub next_cursor: Option<String>,
    /// URL of the next page, when there is one.
    #[serde(default)]
    pub next_page: Option<String>,
}

/// A published transactional email, as listed by `list_transactional_emails`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct TransactionalEmail {
    /// Transactional email ID, used as `transactional_id` when sending.
    pub id: String,
    /// Name.
    pub name: String,
    /// When it was last updated.
    pub last_updated: String,
    /// Names of the data variables the email expects.
    pub data_variables: Vec<String>,
}

/// One page of published transactional emails.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct TransactionalEmailsResponse {
    /// Pagination details.
    pub pagination: TransactionalEmailPagination,
    /// The transactional emails.
    pub data: Vec<TransactionalEmail>,
}

/// A contact, as returned by `find_contact`.
///
/// Hand-written because the spec's `Contact` schema leaves out the custom
/// properties Loops returns with every contact; they land in
/// [`properties`](Self::properties).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Contact {
    /// The contact's Loops ID.
    pub id: Option<String>,
    /// Email address.
    pub email: Option<String>,
    /// First name.
    pub first_name: Option<String>,
    /// Last name.
    pub last_name: Option<String>,
    /// Where the contact came from, e.g. `"API"`.
    pub source: Option<String>,
    /// Whether the contact receives campaign and workflow emails.
    pub subscribed: Option<bool>,
    /// User group.
    pub user_group: Option<String>,
    /// User ID.
    pub user_id: Option<String>,
    /// Mailing list IDs mapped to whether the contact is on that list.
    #[serde(default, deserialize_with = "null_as_empty")]
    pub mailing_lists: Map<String, Value>,
    /// Double opt-in status (`"pending"`, `"accepted"`, `"rejected"`), or
    /// `None` when double opt-in does not apply.
    pub opt_in_status: Option<String>,
    /// Every other key: the contact's custom properties.
    #[serde(flatten)]
    pub properties: Map<String, Value>,
}

fn null_as_empty<'de, D>(deserializer: D) -> Result<Map<String, Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<Map<String, Value>>::deserialize(deserializer)?.unwrap_or_default())
}
