use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::generated::{CreateWorkflowNodeTypeName, WorkflowQueuedContactPolicy};

/// How a contact is identified: by email, by user ID, or both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContactId {
    /// The contact's email address.
    Email(String),
    /// The contact's user ID.
    UserId(String),
    /// Both. On upsert, Loops matches either value and updates the other.
    Both {
        /// The contact's email address.
        email: String,
        /// The contact's user ID.
        user_id: String,
    },
}

impl ContactId {
    /// Identify the contact by email.
    pub fn email(email: impl Into<String>) -> Self {
        Self::Email(email.into())
    }

    /// Identify the contact by user ID.
    pub fn user_id(user_id: impl Into<String>) -> Self {
        Self::UserId(user_id.into())
    }

    /// Identify the contact by email and user ID.
    pub fn both(email: impl Into<String>, user_id: impl Into<String>) -> Self {
        Self::Both {
            email: email.into(),
            user_id: user_id.into(),
        }
    }

    /// The email address, when one was given.
    pub fn email_address(&self) -> Option<&str> {
        match self {
            Self::Email(email) | Self::Both { email, .. } => Some(email),
            Self::UserId(_) => None,
        }
    }

    /// The user ID, when one was given.
    pub fn user_id_value(&self) -> Option<&str> {
        match self {
            Self::UserId(user_id) | Self::Both { user_id, .. } => Some(user_id),
            Self::Email(_) => None,
        }
    }
}

/// Contact fields for `create_contact` and `upsert_contact`. Unset fields are
/// left out of the request.
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
#[must_use]
pub struct ContactFields {
    /// First name.
    pub first_name: Option<String>,
    /// Last name.
    pub last_name: Option<String>,
    /// Custom source value replacing the default `"API"`.
    pub source: Option<String>,
    /// Subscription status. On upsert, `Some(true)` re-subscribes a contact
    /// that unsubscribed; leave it unset unless that is the intent.
    pub subscribed: Option<bool>,
    /// User group.
    pub user_group: Option<String>,
    /// Mailing list IDs mapped to whether the contact should be on that list.
    pub mailing_lists: BTreeMap<String, bool>,
    /// Custom contact properties, sent as top-level keys.
    pub custom_properties: Map<String, Value>,
}

impl ContactFields {
    /// No fields set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the first name.
    pub fn first_name(mut self, value: impl Into<String>) -> Self {
        self.first_name = Some(value.into());
        self
    }

    /// Set the last name.
    pub fn last_name(mut self, value: impl Into<String>) -> Self {
        self.last_name = Some(value.into());
        self
    }

    /// Set the source.
    pub fn source(mut self, value: impl Into<String>) -> Self {
        self.source = Some(value.into());
        self
    }

    /// Set the subscription status.
    pub fn subscribed(mut self, value: bool) -> Self {
        self.subscribed = Some(value);
        self
    }

    /// Set the user group.
    pub fn user_group(mut self, value: impl Into<String>) -> Self {
        self.user_group = Some(value.into());
        self
    }

    /// Add the contact to (`true`) or remove it from (`false`) a mailing list.
    pub fn mailing_list(mut self, list_id: impl Into<String>, subscribed: bool) -> Self {
        self.mailing_lists.insert(list_id.into(), subscribed);
        self
    }

    /// Set a custom contact property (string, number, boolean or date).
    pub fn property(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.custom_properties.insert(key.into(), value.into());
        self
    }
}

/// Pagination for the list endpoints.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
#[must_use]
pub struct Page {
    /// Results per page, 10 to 50. Loops defaults to 20.
    pub per_page: Option<u32>,
    /// The `pagination.nextCursor` of the previous page.
    pub cursor: Option<String>,
}

impl Page {
    /// The first page at the default size.
    pub fn first() -> Self {
        Self::default()
    }

    /// Set the page size.
    pub fn per_page(mut self, per_page: u32) -> Self {
        self.per_page = Some(per_page);
        self
    }

    /// Continue from a cursor.
    pub fn cursor(mut self, cursor: impl Into<String>) -> Self {
        self.cursor = Some(cursor.into());
        self
    }

    pub(crate) fn per_page_param(&self) -> Option<String> {
        self.per_page.map(|per_page| per_page.to_string())
    }
}

/// Optional parts of `send_event`.
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
#[must_use]
pub struct EventOptions {
    /// Event properties, available in emails the event sends.
    pub event_properties: Map<String, Value>,
    /// Mailing list IDs mapped to whether the contact should be on that list.
    pub mailing_lists: BTreeMap<String, bool>,
    /// Contact properties to update, sent as top-level keys.
    pub contact_properties: Map<String, Value>,
    /// Idempotency key, up to 100 characters. A UUID v4 is generated when unset.
    pub idempotency_key: Option<String>,
}

impl EventOptions {
    /// No options set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set an event property.
    pub fn event_property(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.event_properties.insert(key.into(), value.into());
        self
    }

    /// Add the contact to (`true`) or remove it from (`false`) a mailing list.
    pub fn mailing_list(mut self, list_id: impl Into<String>, subscribed: bool) -> Self {
        self.mailing_lists.insert(list_id.into(), subscribed);
        self
    }

    /// Update a contact property.
    pub fn contact_property(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.contact_properties.insert(key.into(), value.into());
        self
    }

    /// Set the idempotency key.
    pub fn idempotency_key(mut self, key: impl Into<String>) -> Self {
        self.idempotency_key = Some(key.into());
        self
    }
}

/// Optional parts of `send_transactional_email`.
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
#[must_use]
pub struct TransactionalOptions {
    /// Values for the template's data variables (strings, numbers, or arrays
    /// for array blocks).
    pub data_variables: Map<String, Value>,
    /// File attachments. Loops support must enable attachments first.
    pub attachments: Vec<crate::generated::TransactionalRequestAttachment>,
    /// Create the contact in your audience if it does not exist yet.
    pub add_to_audience: Option<bool>,
    /// Idempotency key, up to 100 characters. A UUID v4 is generated when unset.
    pub idempotency_key: Option<String>,
}

impl TransactionalOptions {
    /// No options set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a data variable.
    pub fn data_variable(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.data_variables.insert(key.into(), value.into());
        self
    }

    /// Attach a file. `data` is the base64-encoded file content.
    pub fn attachment(
        mut self,
        filename: impl Into<String>,
        content_type: impl Into<String>,
        data: impl Into<String>,
    ) -> Self {
        self.attachments
            .push(crate::generated::TransactionalRequestAttachment {
                filename: filename.into(),
                content_type: content_type.into(),
                data: data.into(),
            });
        self
    }

    /// Set whether to create the contact.
    pub fn add_to_audience(mut self, add: bool) -> Self {
        self.add_to_audience = Some(add);
        self
    }

    /// Set the idempotency key.
    pub fn idempotency_key(mut self, key: impl Into<String>) -> Self {
        self.idempotency_key = Some(key.into());
        self
    }
}

/// How an email message renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmailFormat {
    /// Plain text.
    Plain,
    /// Styled HTML.
    Styled,
}

/// Changes for `update_email_message`. Unset fields are left unchanged.
///
/// The three fallback maps merge per key on the server: `Some(value)` sets a
/// fallback, `None` deletes it, and keys left out are unchanged.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
#[must_use]
pub struct EmailMessageUpdate {
    /// Subject line.
    pub subject: Option<String>,
    /// Preview text shown in the inbox.
    pub preview_text: Option<String>,
    /// Sender display name.
    pub from_name: Option<String>,
    /// Sender username, without `@` or domain; the team domain is appended.
    pub from_email: Option<String>,
    /// Reply-to address (empty or a valid email).
    pub reply_to_email: Option<String>,
    /// Body as LMX, with styles in a `<Style />` tag.
    pub lmx: Option<String>,
    /// Optimistic concurrency token; a stale value fails with 409.
    pub expected_revision_id: Option<String>,
    /// CC address. Needs CC/BCC enabled for the team; not for campaign
    /// emails. Checked against safe mode.
    pub cc_email: Option<String>,
    /// BCC address. Same requirements as `cc_email`.
    pub bcc_email: Option<String>,
    /// ISO 639-1 language code, e.g. `"en"`.
    pub language_code: Option<String>,
    /// Rendering format.
    pub email_format: Option<EmailFormat>,
    /// Fallbacks keyed by contact property name.
    pub contact_properties_fallbacks: Option<BTreeMap<String, Option<String>>>,
    /// Fallbacks keyed by event property name.
    pub event_properties_fallbacks: Option<BTreeMap<String, Option<String>>>,
    /// Fallbacks keyed by data variable name.
    pub data_variables_fallbacks: Option<BTreeMap<String, Option<String>>>,
}

impl EmailMessageUpdate {
    /// No changes.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the subject.
    pub fn subject(mut self, value: impl Into<String>) -> Self {
        self.subject = Some(value.into());
        self
    }

    /// Set the preview text.
    pub fn preview_text(mut self, value: impl Into<String>) -> Self {
        self.preview_text = Some(value.into());
        self
    }

    /// Set the sender display name.
    pub fn from_name(mut self, value: impl Into<String>) -> Self {
        self.from_name = Some(value.into());
        self
    }

    /// Set the sender username.
    pub fn from_email(mut self, value: impl Into<String>) -> Self {
        self.from_email = Some(value.into());
        self
    }

    /// Set the reply-to address.
    pub fn reply_to_email(mut self, value: impl Into<String>) -> Self {
        self.reply_to_email = Some(value.into());
        self
    }

    /// Set the LMX body.
    pub fn lmx(mut self, value: impl Into<String>) -> Self {
        self.lmx = Some(value.into());
        self
    }

    /// Set the expected revision.
    pub fn expected_revision_id(mut self, value: impl Into<String>) -> Self {
        self.expected_revision_id = Some(value.into());
        self
    }

    /// Set the CC address.
    pub fn cc_email(mut self, value: impl Into<String>) -> Self {
        self.cc_email = Some(value.into());
        self
    }

    /// Set the BCC address.
    pub fn bcc_email(mut self, value: impl Into<String>) -> Self {
        self.bcc_email = Some(value.into());
        self
    }

    /// Set the language code.
    pub fn language_code(mut self, value: impl Into<String>) -> Self {
        self.language_code = Some(value.into());
        self
    }

    /// Set the rendering format.
    pub fn email_format(mut self, value: EmailFormat) -> Self {
        self.email_format = Some(value);
        self
    }

    /// Set (`Some`) or delete (`None`) a contact property fallback.
    pub fn contact_property_fallback(
        mut self,
        key: impl Into<String>,
        value: Option<&str>,
    ) -> Self {
        self.contact_properties_fallbacks
            .get_or_insert_with(BTreeMap::new)
            .insert(key.into(), value.map(str::to_owned));
        self
    }

    /// Set (`Some`) or delete (`None`) an event property fallback.
    pub fn event_property_fallback(mut self, key: impl Into<String>, value: Option<&str>) -> Self {
        self.event_properties_fallbacks
            .get_or_insert_with(BTreeMap::new)
            .insert(key.into(), value.map(str::to_owned));
        self
    }

    /// Set (`Some`) or delete (`None`) a data variable fallback.
    pub fn data_variable_fallback(mut self, key: impl Into<String>, value: Option<&str>) -> Self {
        self.data_variables_fallbacks
            .get_or_insert_with(BTreeMap::new)
            .insert(key.into(), value.map(str::to_owned));
        self
    }
}

/// Values used to render `preview_email_message`.
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
#[must_use]
pub struct PreviewOptions {
    /// Contact property values.
    pub contact_properties: Map<String, Value>,
    /// Event property values.
    pub event_properties: Map<String, Value>,
    /// Data variable values.
    pub data_variables: Map<String, Value>,
}

impl PreviewOptions {
    /// No values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a contact property value.
    pub fn contact_property(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.contact_properties.insert(key.into(), value.into());
        self
    }

    /// Set an event property value.
    pub fn event_property(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.event_properties.insert(key.into(), value.into());
        self
    }

    /// Set a data variable value.
    pub fn data_variable(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.data_variables.insert(key.into(), value.into());
        self
    }
}

/// Where `create_workflow_node` inserts the new node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodePlacement {
    /// Into the edge from `from_node_id` to `to_node_id`.
    Between {
        /// Source of the edge.
        from_node_id: String,
        /// Target of the edge.
        to_node_id: String,
    },
    /// Ahead of `to_node_id`, which needs at least one incoming parent and
    /// cannot be a trigger.
    Before {
        /// The node to insert ahead of.
        to_node_id: String,
    },
    /// Behind `from_node_id`, which must have exactly one outgoing node. Use
    /// [`Between`](Self::Between) when it has several.
    After {
        /// The node to insert behind.
        from_node_id: String,
    },
}

impl NodePlacement {
    /// Insert into the edge between two nodes.
    pub fn between(from_node_id: impl Into<String>, to_node_id: impl Into<String>) -> Self {
        Self::Between {
            from_node_id: from_node_id.into(),
            to_node_id: to_node_id.into(),
        }
    }

    /// Insert ahead of a node.
    pub fn before(to_node_id: impl Into<String>) -> Self {
        Self::Before {
            to_node_id: to_node_id.into(),
        }
    }

    /// Insert behind a node.
    pub fn after(from_node_id: impl Into<String>) -> Self {
        Self::After {
            from_node_id: from_node_id.into(),
        }
    }
}

/// Optional parts of `delete_workflow_node`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
#[must_use]
pub struct DeleteNodeOptions {
    /// Report what would be deleted without deleting it.
    pub dry_run: Option<bool>,
    /// What to do with contacts queued at the node. Loops defaults to `Fail`.
    pub queued_contact_policy: Option<WorkflowQueuedContactPolicy>,
    /// Also delete everything downstream of the node, instead of re-linking
    /// its edge.
    pub recursive: bool,
}

impl DeleteNodeOptions {
    /// Delete the single node.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set dry-run mode.
    pub fn dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = Some(dry_run);
        self
    }

    /// Set the queued-contact policy.
    pub fn queued_contact_policy(mut self, policy: WorkflowQueuedContactPolicy) -> Self {
        self.queued_contact_policy = Some(policy);
        self
    }

    /// Delete the node's whole downstream subtree.
    pub fn recursive(mut self, recursive: bool) -> Self {
        self.recursive = recursive;
        self
    }
}

pub(crate) fn node_type_name(node_type: &CreateWorkflowNodeTypeName) -> Value {
    serde_json::to_value(node_type).unwrap_or(Value::Null)
}
