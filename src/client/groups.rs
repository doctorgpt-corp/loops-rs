use super::{LoopsClient, Page, check};
use crate::error::Result;
use crate::generated::{
    CreateCampaignGroupRequest, CreateCampaignGroupResponse, CreateGroupRequest,
    CreateTransactionalGroupRequest, GetCampaignGroupRequest, GetCampaignGroupResponse,
    GetTransactionalGroupRequest, GroupResponse, ListCampaignGroupsRequest,
    ListCampaignGroupsRequestQuery, ListCampaignGroupsResponse, ListGroupsResponse,
    ListTransactionalGroupsRequest, ListTransactionalGroupsRequestQuery,
    UpdateCampaignGroupRequest, UpdateGroupRequest, UpdateTransactionalGroupRequest,
};
use crate::routes::Route;

impl LoopsClient {
    /// List campaign groups.
    pub async fn list_campaign_groups(&self, page: Page) -> Result<ListGroupsResponse> {
        const ACTION: &str = "list campaign groups";
        let query = ListCampaignGroupsRequestQuery {
            per_page: page.per_page_param(),
            cursor: page.cursor,
        };
        let raw = self
            .send(Route::ListCampaignGroups, &[], |r| r.query(&query))
            .await?;
        match raw
            .parse(ACTION, ListCampaignGroupsRequest::parse_response)
            .await?
        {
            ListCampaignGroupsResponse::Ok(groups) => Ok(groups),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Get a campaign group.
    pub async fn get_campaign_group(&self, campaign_group_id: &str) -> Result<GroupResponse> {
        const ACTION: &str = "get campaign group";
        let raw = self
            .send(Route::GetCampaignGroup, &[campaign_group_id], |r| r)
            .await?;
        match raw
            .parse(ACTION, GetCampaignGroupRequest::parse_response)
            .await?
        {
            GetCampaignGroupResponse::Ok(group) => Ok(group),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Create a campaign group.
    pub async fn create_campaign_group(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> Result<GroupResponse> {
        const ACTION: &str = "create campaign group";
        let body = group_body(name, description);
        check(&body)?;
        let raw = self
            .send(Route::CreateCampaignGroup, &[], |r| r.json(&body))
            .await?;
        match raw
            .parse(ACTION, CreateCampaignGroupRequest::parse_response)
            .await?
        {
            CreateCampaignGroupResponse::Ok(group) => Ok(group),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Rename a campaign group and/or change its description.
    pub async fn update_campaign_group(
        &self,
        campaign_group_id: &str,
        name: Option<&str>,
        description: Option<&str>,
    ) -> Result<GroupResponse> {
        const ACTION: &str = "update campaign group";
        let body = UpdateGroupRequest {
            name: name.map(str::to_owned),
            description: description.map(str::to_owned),
        };
        let raw = self
            .send(Route::UpdateCampaignGroup, &[campaign_group_id], |r| {
                r.json(&body)
            })
            .await?;
        match raw
            .parse(ACTION, UpdateCampaignGroupRequest::parse_response)
            .await?
        {
            GetCampaignGroupResponse::Ok(group) => Ok(group),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// List transactional groups.
    pub async fn list_transactional_groups(&self, page: Page) -> Result<ListGroupsResponse> {
        const ACTION: &str = "list transactional groups";
        let query = ListTransactionalGroupsRequestQuery {
            per_page: page.per_page_param(),
            cursor: page.cursor,
        };
        let raw = self
            .send(Route::ListTransactionalGroups, &[], |r| r.query(&query))
            .await?;
        match raw
            .parse(ACTION, ListTransactionalGroupsRequest::parse_response)
            .await?
        {
            ListCampaignGroupsResponse::Ok(groups) => Ok(groups),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Get a transactional group.
    pub async fn get_transactional_group(
        &self,
        transactional_group_id: &str,
    ) -> Result<GroupResponse> {
        const ACTION: &str = "get transactional group";
        let raw = self
            .send(
                Route::GetTransactionalGroup,
                &[transactional_group_id],
                |r| r,
            )
            .await?;
        match raw
            .parse(ACTION, GetTransactionalGroupRequest::parse_response)
            .await?
        {
            GetCampaignGroupResponse::Ok(group) => Ok(group),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Create a transactional group.
    pub async fn create_transactional_group(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> Result<GroupResponse> {
        const ACTION: &str = "create transactional group";
        let body = group_body(name, description);
        check(&body)?;
        let raw = self
            .send(Route::CreateTransactionalGroup, &[], |r| r.json(&body))
            .await?;
        match raw
            .parse(ACTION, CreateTransactionalGroupRequest::parse_response)
            .await?
        {
            CreateCampaignGroupResponse::Ok(group) => Ok(group),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Rename a transactional group and/or change its description.
    pub async fn update_transactional_group(
        &self,
        transactional_group_id: &str,
        name: Option<&str>,
        description: Option<&str>,
    ) -> Result<GroupResponse> {
        const ACTION: &str = "update transactional group";
        let body = UpdateGroupRequest {
            name: name.map(str::to_owned),
            description: description.map(str::to_owned),
        };
        let raw = self
            .send(
                Route::UpdateTransactionalGroup,
                &[transactional_group_id],
                |r| r.json(&body),
            )
            .await?;
        match raw
            .parse(ACTION, UpdateTransactionalGroupRequest::parse_response)
            .await?
        {
            GetCampaignGroupResponse::Ok(group) => Ok(group),
            _ => Err(raw.unexpected(ACTION)),
        }
    }
}

fn group_body(name: &str, description: Option<&str>) -> CreateGroupRequest {
    CreateGroupRequest {
        name: name.to_owned(),
        description: description.filter(|d| !d.is_empty()).map(str::to_owned),
    }
}
