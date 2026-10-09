use super::{LoopsClient, Page, check};
use crate::error::Result;
use crate::generated::{
    ComponentResponse, CreateComponentBody, CreateComponentRequest, CreateComponentResponse,
    CreateThemeBody, CreateThemeRequest, CreateThemeResponse, GetComponentRequest,
    GetComponentResponse, GetThemeRequest, GetThemeResponse, ListComponentsRequest,
    ListComponentsRequestQuery, ListComponentsResponse, ListComponentsResponseEnum,
    ListThemesRequest, ListThemesRequestQuery, ListThemesResponse, ListThemesResponseEnum,
    ThemeResponse, ThemeStyles, UpdateComponentBody, UpdateComponentRequest,
    UpdateComponentResponse, UpdateComponentResponseEnum, UpdateThemeBody, UpdateThemeRequest,
    UpdateThemeResponse, UpdateThemeResponseEnum,
};
use crate::routes::Route;

impl LoopsClient {
    /// List components.
    pub async fn list_components(&self, page: Page) -> Result<ListComponentsResponse> {
        const ACTION: &str = "list components";
        let query = ListComponentsRequestQuery {
            per_page: page.per_page_param(),
            cursor: page.cursor,
        };
        let raw = self
            .send(Route::ListComponents, &[], |r| r.query(&query))
            .await?;
        match raw
            .parse(ACTION, ListComponentsRequest::parse_response)
            .await?
        {
            ListComponentsResponseEnum::Ok(components) => Ok(components),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Get a component.
    pub async fn get_component(&self, component_id: &str) -> Result<ComponentResponse> {
        const ACTION: &str = "get component";
        let raw = self
            .send(Route::GetComponent, &[component_id], |r| r)
            .await?;
        match raw
            .parse(ACTION, GetComponentRequest::parse_response)
            .await?
        {
            GetComponentResponse::Ok(component) => Ok(component),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Create a component from an LMX body.
    pub async fn create_component(&self, name: &str, lmx: &str) -> Result<ComponentResponse> {
        const ACTION: &str = "create component";
        let body = CreateComponentBody {
            name: name.to_owned(),
            lmx: lmx.to_owned(),
        };
        check(&body)?;
        let raw = self
            .send(Route::CreateComponent, &[], |r| r.json(&body))
            .await?;
        match raw
            .parse(ACTION, CreateComponentRequest::parse_response)
            .await?
        {
            CreateComponentResponse::Created(component) => Ok(component),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Rename a component and/or replace its LMX body. Changing the body
    /// updates every email using the component; the response counts them in
    /// `affected_email_count`.
    pub async fn update_component(
        &self,
        component_id: &str,
        name: Option<&str>,
        lmx: Option<&str>,
    ) -> Result<UpdateComponentResponse> {
        const ACTION: &str = "update component";
        let body = UpdateComponentBody {
            name: name.map(str::to_owned),
            lmx: lmx.map(str::to_owned),
        };
        let raw = self
            .send(Route::UpdateComponent, &[component_id], |r| r.json(&body))
            .await?;
        match raw
            .parse(ACTION, UpdateComponentRequest::parse_response)
            .await?
        {
            UpdateComponentResponseEnum::Ok(component) => Ok(component),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// List themes.
    pub async fn list_themes(&self, page: Page) -> Result<ListThemesResponse> {
        const ACTION: &str = "list themes";
        let query = ListThemesRequestQuery {
            per_page: page.per_page_param(),
            cursor: page.cursor,
        };
        let raw = self
            .send(Route::ListThemes, &[], |r| r.query(&query))
            .await?;
        match raw.parse(ACTION, ListThemesRequest::parse_response).await? {
            ListThemesResponseEnum::Ok(themes) => Ok(themes),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Get a theme.
    pub async fn get_theme(&self, theme_id: &str) -> Result<ThemeResponse> {
        const ACTION: &str = "get theme";
        let raw = self.send(Route::GetTheme, &[theme_id], |r| r).await?;
        match raw.parse(ACTION, GetThemeRequest::parse_response).await? {
            GetThemeResponse::Ok(theme) => Ok(theme),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Create a theme. `styles` mirrors the attributes of the LMX `<Style />`
    /// tag; unset fields keep the Loops defaults.
    pub async fn create_theme(
        &self,
        name: &str,
        styles: Option<ThemeStyles>,
    ) -> Result<ThemeResponse> {
        const ACTION: &str = "create theme";
        let body = CreateThemeBody {
            name: name.to_owned(),
            styles,
        };
        check(&body)?;
        let raw = self
            .send(Route::CreateTheme, &[], |r| r.json(&body))
            .await?;
        match raw
            .parse(ACTION, CreateThemeRequest::parse_response)
            .await?
        {
            CreateThemeResponse::Created(theme) => Ok(theme),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Rename a theme and/or change its styles. Changing styles updates every
    /// email using the theme; the response counts them in
    /// `affected_email_count`.
    pub async fn update_theme(
        &self,
        theme_id: &str,
        name: Option<&str>,
        styles: Option<ThemeStyles>,
    ) -> Result<UpdateThemeResponse> {
        const ACTION: &str = "update theme";
        let body = UpdateThemeBody {
            name: name.map(str::to_owned),
            styles,
        };
        let raw = self
            .send(Route::UpdateTheme, &[theme_id], |r| r.json(&body))
            .await?;
        match raw
            .parse(ACTION, UpdateThemeRequest::parse_response)
            .await?
        {
            UpdateThemeResponseEnum::Ok(theme) => Ok(theme),
            _ => Err(raw.unexpected(ACTION)),
        }
    }
}
