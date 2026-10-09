use serde_json::{Map, Value};

use super::{LoopsClient, Page};
use crate::error::Result;
use crate::generated::{
    AudienceSegmentResponse, CreateAudienceSegmentRequestParams, EventPattern,
    GetAudienceSegmentRequest, GetAudienceSegmentResponse, GetEventPatternByNameRequest,
    GetEventPatternRequest, GetEventPatternResponse, ListAudienceSegmentsRequest,
    ListAudienceSegmentsRequestQuery, ListAudienceSegmentsResponse,
    ListAudienceSegmentsResponseEnum, ListEventPatternsRequest, ListEventPatternsRequestQuery,
    ListEventPatternsResponse, ListEventPatternsResponseEnum,
};
use crate::routes::Route;

impl LoopsClient {
    /// List audience segments, most recently created first.
    pub async fn list_audience_segments(&self, page: Page) -> Result<ListAudienceSegmentsResponse> {
        const ACTION: &str = "list audience segments";
        let query = ListAudienceSegmentsRequestQuery {
            per_page: page.per_page_param(),
            cursor: page.cursor,
        };
        let raw = self
            .send(Route::ListAudienceSegments, &[], |r| r.query(&query))
            .await?;
        match raw
            .parse(ACTION, ListAudienceSegmentsRequest::parse_response)
            .await?
        {
            ListAudienceSegmentsResponseEnum::Ok(segments) => Ok(segments),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Get an audience segment.
    pub async fn get_audience_segment(
        &self,
        audience_segment_id: &str,
    ) -> Result<AudienceSegmentResponse> {
        const ACTION: &str = "get audience segment";
        let raw = self
            .send(Route::GetAudienceSegment, &[audience_segment_id], |r| r)
            .await?;
        match raw
            .parse(ACTION, GetAudienceSegmentRequest::parse_response)
            .await?
        {
            GetAudienceSegmentResponse::Ok(segment) => Ok(segment),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Create an audience segment.
    ///
    /// `filter` is the segment's condition tree, e.g.
    /// `{"match": "all", "conditions": [{"type": "property", "key": "plan",
    /// "operator": "equals", "value": "pro"}]}`. `match` is `"all"` or
    /// `"any"`; each condition is a `property`, `optIn` or `activity`
    /// condition (see the Loops docs for the grammar).
    pub async fn create_audience_segment(
        &self,
        name: &str,
        filter: Value,
        description: Option<&str>,
    ) -> Result<AudienceSegmentResponse> {
        const ACTION: &str = "create audience segment";
        let mut body = Map::new();
        body.insert("name".to_owned(), Value::from(name));
        body.insert("filter".to_owned(), filter);
        if let Some(description) = description {
            body.insert("description".to_owned(), Value::from(description));
        }
        let raw = self
            .send(Route::CreateAudienceSegment, &[], |r| r.json(&body))
            .await?;
        match raw
            .parse(ACTION, CreateAudienceSegmentRequestParams::parse_response)
            .await?
        {
            GetAudienceSegmentResponse::Ok(segment) => Ok(segment),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// List event patterns: the event types workflow triggers can listen
    /// for, custom or from integrated webhook platforms.
    pub async fn list_event_patterns(&self, page: Page) -> Result<ListEventPatternsResponse> {
        const ACTION: &str = "list event patterns";
        let query = ListEventPatternsRequestQuery {
            per_page: page.per_page_param(),
            cursor: page.cursor,
        };
        let raw = self
            .send(Route::ListEventPatterns, &[], |r| r.query(&query))
            .await?;
        match raw
            .parse(ACTION, ListEventPatternsRequest::parse_response)
            .await?
        {
            ListEventPatternsResponseEnum::Ok(patterns) => Ok(patterns),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Get an event pattern by ID.
    pub async fn get_event_pattern(&self, event_pattern_id: &str) -> Result<EventPattern> {
        const ACTION: &str = "get event pattern";
        let raw = self
            .send(Route::GetEventPattern, &[event_pattern_id], |r| r)
            .await?;
        match raw
            .parse(ACTION, GetEventPatternRequest::parse_response)
            .await?
        {
            GetEventPatternResponse::Ok(pattern) => Ok(pattern),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Get an event pattern by its case-sensitive event name.
    pub async fn get_event_pattern_by_name(&self, event_name: &str) -> Result<EventPattern> {
        const ACTION: &str = "get event pattern by name";
        let raw = self
            .send(Route::GetEventPatternByName, &[event_name], |r| r)
            .await?;
        match raw
            .parse(ACTION, GetEventPatternByNameRequest::parse_response)
            .await?
        {
            GetEventPatternResponse::Ok(pattern) => Ok(pattern),
            _ => Err(raw.unexpected(ACTION)),
        }
    }
}
