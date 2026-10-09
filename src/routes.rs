//! The named table of every Loops endpoint [`LoopsClient`](crate::LoopsClient) calls.
//!
//! The client builds its request URLs from this table, the `testing` mock
//! registers one route per entry, and a test checks every entry against the
//! shipped OpenAPI spec, so a path renamed upstream fails the build instead of
//! a request.

/// An HTTP method used by a [`Route`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    /// `GET`
    Get,
    /// `POST`
    Post,
    /// `PUT`
    Put,
    /// `DELETE`
    Delete,
}

impl Method {
    /// The method name in upper case, as sent on the wire.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
        }
    }

    pub(crate) fn to_http(self) -> http::Method {
        match self {
            Self::Get => http::Method::GET,
            Self::Post => http::Method::POST,
            Self::Put => http::Method::PUT,
            Self::Delete => http::Method::DELETE,
        }
    }
}

macro_rules! routes {
    ($($variant:ident = ($name:literal, $method:ident, $path:literal),)+) => {
        /// A Loops endpoint, named after the [`LoopsClient`](crate::LoopsClient)
        /// method that calls it.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum Route {
            $(
                #[doc = concat!("`", stringify!($method), " ", $path, "` (`", $name, "`)")]
                $variant,
            )+
        }

        impl Route {
            /// Every route, in table order.
            pub const ALL: &'static [Route] = &[$(Route::$variant),+];

            /// The snake-case route name, e.g. `"transactional"`.
            pub const fn name(self) -> &'static str {
                match self { $(Route::$variant => $name),+ }
            }

            /// The HTTP method.
            pub const fn method(self) -> Method {
                match self { $(Route::$variant => Method::$method),+ }
            }

            /// The path template relative to the base URL, with `{...}`
            /// placeholders for path parameters, e.g. `"/v1/campaigns/{id}"`.
            pub const fn path(self) -> &'static str {
                match self { $(Route::$variant => $path),+ }
            }

            /// Look a route up by its [`name`](Self::name).
            pub fn from_name(name: &str) -> Option<Route> {
                Route::ALL.iter().copied().find(|route| route.name() == name)
            }
        }
    };
}

routes! {
    Health = ("health", Get, "/v1/api-key"),
    CreateContact = ("create_contact", Post, "/v1/contacts/create"),
    UpsertContact = ("upsert_contact", Put, "/v1/contacts/update"),
    FindContact = ("find_contact", Get, "/v1/contacts/find"),
    DeleteContact = ("delete_contact", Post, "/v1/contacts/delete"),
    ListContactProperties = ("list_contact_properties", Get, "/v1/contacts/properties"),
    CreateContactProperty = ("create_contact_property", Post, "/v1/contacts/properties"),
    GetContactSuppression = ("get_contact_suppression", Get, "/v1/contacts/suppression"),
    RemoveContactSuppression = ("remove_contact_suppression", Delete, "/v1/contacts/suppression"),
    SendEvent = ("send_event", Post, "/v1/events/send"),
    ListMailingLists = ("list_mailing_lists", Get, "/v1/lists"),
    Transactional = ("transactional", Post, "/v1/transactional"),
    ListTransactional = ("list_transactional", Get, "/v1/transactional"),
    ListSendingIps = ("list_sending_ips", Get, "/v1/dedicated-sending-ips"),
    ListCampaigns = ("list_campaigns", Get, "/v1/campaigns"),
    CreateCampaign = ("create_campaign", Post, "/v1/campaigns"),
    GetCampaign = ("get_campaign", Get, "/v1/campaigns/{id}"),
    UpdateCampaign = ("update_campaign", Post, "/v1/campaigns/{id}"),
    ListComponents = ("list_components", Get, "/v1/components"),
    GetComponent = ("get_component", Get, "/v1/components/{id}"),
    ListThemes = ("list_themes", Get, "/v1/themes"),
    GetTheme = ("get_theme", Get, "/v1/themes/{id}"),
    GetEmailMessage = ("get_email_message", Get, "/v1/email-messages/{id}"),
    UpdateEmailMessage = ("update_email_message", Post, "/v1/email-messages/{id}"),
    ListTransactionalTemplates = ("list_transactional_templates", Get, "/v1/transactional-emails"),
    GetTransactionalTemplate = ("get_transactional_template", Get, "/v1/transactional-emails/{id}"),
    CreateTransactionalTemplate = ("create_transactional_template", Post, "/v1/transactional-emails"),
    UpdateTransactionalTemplate = ("update_transactional_template", Post, "/v1/transactional-emails/{id}"),
    DraftTransactionalTemplate = ("draft_transactional_template", Post, "/v1/transactional-emails/{id}/draft"),
    PublishTransactionalTemplate = ("publish_transactional_template", Post, "/v1/transactional-emails/{id}/publish"),
    CreateUpload = ("create_upload", Post, "/v1/uploads"),
    CompleteUpload = ("complete_upload", Post, "/v1/uploads/{id}/complete"),
    ListWorkflows = ("list_workflows", Get, "/v1/workflows"),
    GetWorkflow = ("get_workflow", Get, "/v1/workflows/{id}"),
    GetWorkflowNode = ("get_workflow_node", Get, "/v1/workflows/{id}/nodes/{node_id}"),
    ListAudienceSegments = ("list_audience_segments", Get, "/v1/audience-segments"),
    GetAudienceSegment = ("get_audience_segment", Get, "/v1/audience-segments/{id}"),
    ListCampaignGroups = ("list_campaign_groups", Get, "/v1/campaign-groups"),
    GetCampaignGroup = ("get_campaign_group", Get, "/v1/campaign-groups/{id}"),
    CreateCampaignGroup = ("create_campaign_group", Post, "/v1/campaign-groups"),
    UpdateCampaignGroup = ("update_campaign_group", Post, "/v1/campaign-groups/{id}"),
    ListTransactionalGroups = ("list_transactional_groups", Get, "/v1/transactional-groups"),
    GetTransactionalGroup = ("get_transactional_group", Get, "/v1/transactional-groups/{id}"),
    CreateTransactionalGroup = ("create_transactional_group", Post, "/v1/transactional-groups"),
    UpdateTransactionalGroup = ("update_transactional_group", Post, "/v1/transactional-groups/{id}"),
    PreviewEmailMessage = ("preview_email_message", Post, "/v1/email-messages/{id}/preview"),
    CreateAudienceSegment = ("create_audience_segment", Post, "/v1/audience-segments"),
    CreateComponent = ("create_component", Post, "/v1/components"),
    UpdateComponent = ("update_component", Post, "/v1/components/{id}"),
    CreateTheme = ("create_theme", Post, "/v1/themes"),
    UpdateTheme = ("update_theme", Post, "/v1/themes/{id}"),
    GetEmailMessageGuardian = ("get_email_message_guardian", Get, "/v1/email-messages/{id}/guardian"),
    ListEventPatterns = ("list_event_patterns", Get, "/v1/event-patterns"),
    GetEventPattern = ("get_event_pattern", Get, "/v1/event-patterns/{id}"),
    GetEventPatternByName = ("get_event_pattern_by_name", Get, "/v1/event-patterns/by-name/{name}"),
    CreateWorkflow = ("create_workflow", Post, "/v1/workflows"),
    UpdateWorkflow = ("update_workflow", Post, "/v1/workflows/{id}"),
    ChangeWorkflowMailingList = ("change_workflow_mailing_list", Post, "/v1/workflows/{id}/mailing-list"),
    DeleteWorkflow = ("delete_workflow", Delete, "/v1/workflows/{id}"),
    CreateWorkflowNode = ("create_workflow_node", Post, "/v1/workflows/{id}/nodes"),
    UpdateWorkflowNode = ("update_workflow_node", Post, "/v1/workflows/{id}/nodes/{node_id}"),
    AddWorkflowBranch = ("add_workflow_branch", Post, "/v1/workflows/{id}/nodes/{node_id}/add-branch"),
    RerouteNodeConnection = ("reroute_node_connection", Post, "/v1/workflows/{id}/nodes/{node_id}/reroute"),
    DeleteWorkflowNode = ("delete_workflow_node", Delete, "/v1/workflows/{id}/nodes/{node_id}"),
    DeleteWorkflowNodeRecursively = ("delete_workflow_node_recursively", Delete, "/v1/workflows/{id}/nodes/{node_id}/recursive"),
    GetCampaignMetrics = ("get_campaign_metrics", Get, "/v1/campaigns/{id}/metrics"),
    GetTransactionalTemplateMetrics = ("get_transactional_template_metrics", Get, "/v1/transactional-emails/{id}/metrics"),
    GetWorkflowNodeMetrics = ("get_workflow_node_metrics", Get, "/v1/workflows/{id}/nodes/{node_id}/metrics"),
}
