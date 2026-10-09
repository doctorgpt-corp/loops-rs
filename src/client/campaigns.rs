use super::{LoopsClient, Page, check};
use crate::error::Result;
use crate::generated::{
    CampaignResponse, CreateCampaignRequest, CreateCampaignRequestParams, CreateCampaignResponse,
    CreateCampaignResponseEnum, EmailMetricsResponse, GetCampaignMetricsRequest,
    GetCampaignMetricsResponse, GetCampaignRequest, GetCampaignResponse, ListCampaignsRequest,
    ListCampaignsRequestQuery, ListCampaignsResponse, ListCampaignsResponseEnum,
    UpdateCampaignRequest, UpdateCampaignRequestParams, UpdateCampaignResponse,
};
use crate::routes::Route;

impl LoopsClient {
    /// List campaigns.
    pub async fn list_campaigns(&self, page: Page) -> Result<ListCampaignsResponse> {
        const ACTION: &str = "list campaigns";
        let query = ListCampaignsRequestQuery {
            per_page: page.per_page_param(),
            cursor: page.cursor,
        };
        let raw = self
            .send(Route::ListCampaigns, &[], |r| r.query(&query))
            .await?;
        match raw
            .parse(ACTION, ListCampaignsRequest::parse_response)
            .await?
        {
            ListCampaignsResponseEnum::Ok(campaigns) => Ok(campaigns),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Get a campaign.
    pub async fn get_campaign(&self, campaign_id: &str) -> Result<CampaignResponse> {
        const ACTION: &str = "get campaign";
        let raw = self.send(Route::GetCampaign, &[campaign_id], |r| r).await?;
        match raw
            .parse(ACTION, GetCampaignRequest::parse_response)
            .await?
        {
            GetCampaignResponse::Ok(campaign) => Ok(campaign),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Create a draft campaign, together with its email message.
    pub async fn create_campaign(&self, name: &str) -> Result<CreateCampaignResponse> {
        const ACTION: &str = "create campaign";
        let body = CreateCampaignRequest {
            name: name.to_owned(),
            ..Default::default()
        };
        check(&body)?;
        let raw = self
            .send(Route::CreateCampaign, &[], |r| r.json(&body))
            .await?;
        match raw
            .parse(ACTION, CreateCampaignRequestParams::parse_response)
            .await?
        {
            CreateCampaignResponseEnum::Created(campaign) => Ok(campaign),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Rename a campaign.
    pub async fn update_campaign(&self, campaign_id: &str, name: &str) -> Result<CampaignResponse> {
        const ACTION: &str = "update campaign";
        let body = UpdateCampaignRequest {
            name: Some(name.to_owned()),
            ..Default::default()
        };
        let raw = self
            .send(Route::UpdateCampaign, &[campaign_id], |r| r.json(&body))
            .await?;
        match raw
            .parse(ACTION, UpdateCampaignRequestParams::parse_response)
            .await?
        {
            UpdateCampaignResponse::Ok(campaign) => Ok(campaign),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// All-time engagement metrics of a sent campaign. Opens and clicks count
    /// sends, not events: a send opened five times counts once.
    ///
    /// # Errors
    ///
    /// [`LoopsError::Api`](crate::LoopsError::Api) with status 400 when the
    /// campaign has not been sent or has no email message.
    pub async fn get_campaign_metrics(&self, campaign_id: &str) -> Result<EmailMetricsResponse> {
        const ACTION: &str = "get campaign metrics";
        let raw = self
            .send(Route::GetCampaignMetrics, &[campaign_id], |r| r)
            .await?;
        match raw
            .parse(ACTION, GetCampaignMetricsRequest::parse_response)
            .await?
        {
            GetCampaignMetricsResponse::Ok(metrics) => Ok(metrics),
            _ => Err(raw.unexpected(ACTION)),
        }
    }
}
