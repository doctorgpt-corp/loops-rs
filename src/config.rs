use std::sync::{PoisonError, RwLock};

/// The base URL of the Loops API. The `/v1` version segment is part of each
/// endpoint path, not of the base URL.
pub const DEFAULT_BASE_URL: &str = "https://app.loops.so/api";

/// The environment variable read when no API key was configured.
pub const API_KEY_ENV_VAR: &str = "LOOPS_API_KEY";

/// The process-wide defaults [`LoopsClient`](crate::LoopsClient) falls back to.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LoopsConfig {
    /// API key from [`configure`], else the `LOOPS_API_KEY` environment variable.
    pub api_key: Option<String>,
    /// Base URL of the Loops API.
    pub base_url: String,
    /// When `true`, only addresses ending in one of
    /// [`safe_mode_allowed_domains`](Self::safe_mode_allowed_domains) are accepted.
    pub safe_mode: bool,
    /// Domains allowed in safe mode, each starting with `@` (e.g. `"@test.com"`).
    pub safe_mode_allowed_domains: Vec<String>,
}

/// The settings to change in a [`configure`] call. Unset fields keep their
/// current value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ConfigureOptions {
    /// Loops API key.
    pub api_key: Option<String>,
    /// Base URL of the Loops API (default [`DEFAULT_BASE_URL`]).
    pub base_url: Option<String>,
    /// Turn safe mode on or off.
    pub safe_mode: Option<bool>,
    /// Domains allowed in safe mode, each starting with `@`.
    pub safe_mode_allowed_domains: Option<Vec<String>>,
}

impl ConfigureOptions {
    /// An empty set of changes.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the API key.
    #[must_use]
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Set the base URL. Do not include the `/v1` version segment; a trailing
    /// `/v1` is stripped.
    #[must_use]
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// Turn safe mode on or off. Useful in local development to avoid
    /// emailing real users by accident.
    #[must_use]
    pub fn safe_mode(mut self, safe_mode: bool) -> Self {
        self.safe_mode = Some(safe_mode);
        self
    }

    /// Set the domains allowed in safe mode, each starting with `@`.
    #[must_use]
    pub fn safe_mode_allowed_domains<I, S>(mut self, domains: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.safe_mode_allowed_domains = Some(domains.into_iter().map(Into::into).collect());
        self
    }
}

#[derive(Debug, Clone)]
struct Stored {
    api_key: Option<String>,
    base_url: String,
    safe_mode: bool,
    safe_mode_allowed_domains: Vec<String>,
}

static STORED: RwLock<Stored> = RwLock::new(Stored {
    api_key: None,
    base_url: String::new(),
    safe_mode: false,
    safe_mode_allowed_domains: Vec::new(),
});

/// Change the process-wide defaults. Only the fields set on `options` change.
///
/// Call [`reset_client`](crate::reset_client) afterwards if a client was
/// already created through [`get_client`](crate::get_client).
pub fn configure(options: ConfigureOptions) {
    let mut stored = STORED.write().unwrap_or_else(PoisonError::into_inner);
    if let Some(api_key) = options.api_key {
        stored.api_key = Some(api_key);
    }
    if let Some(base_url) = options.base_url {
        stored.base_url = base_url;
    }
    if let Some(safe_mode) = options.safe_mode {
        stored.safe_mode = safe_mode;
    }
    if let Some(domains) = options.safe_mode_allowed_domains {
        stored.safe_mode_allowed_domains = domains;
    }
}

/// The current process-wide defaults, with the API key falling back to the
/// `LOOPS_API_KEY` environment variable.
pub fn get_config() -> LoopsConfig {
    let stored = STORED
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    LoopsConfig {
        api_key: stored.api_key.filter(|key| !key.is_empty()).or_else(|| {
            std::env::var(API_KEY_ENV_VAR)
                .ok()
                .filter(|key| !key.is_empty())
        }),
        base_url: if stored.base_url.is_empty() {
            DEFAULT_BASE_URL.to_owned()
        } else {
            stored.base_url
        },
        safe_mode: stored.safe_mode,
        safe_mode_allowed_domains: stored.safe_mode_allowed_domains,
    }
}

/// Snapshot and restore the stored defaults, so the `testing` mock can put
/// back what the test found.
#[cfg(feature = "testing")]
pub(crate) fn snapshot() -> impl FnOnce() + Send + 'static {
    let saved = STORED
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    move || *STORED.write().unwrap_or_else(PoisonError::into_inner) = saved
}
