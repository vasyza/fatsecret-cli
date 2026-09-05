//! App configuration: endpoint URLs and device fingerprint.
//!
//! Precedence: `FATSECRET_*` env vars > `--config` file (TOML) > built-in
//! defaults extracted from the mobile app. No secrets live here — only the
//! device model string. Credentials are handled by [`crate::auth`].

use std::path::{Path, PathBuf};

use directories::BaseDirs;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

pub const AUTH_URL_DEFAULT: &str = "https://app.ftscrt.com/api/authenticate/v1/fatsecret";
pub const FOOD_SEARCH_URL_DEFAULT: &str = "https://app.ftscrt.com/api/food/v1/search";
pub const JOURNAL_URL_DEFAULT: &str =
    "https://app.ftscrt.com/api/user-data/v1/update-journal-entries";
pub const FOOD_POPULAR_URL_DEFAULT: &str =
    "https://app.ftscrt.com/api/food/v1/most-popular-serving-size";
pub const FOOD_VOTE_URL_DEFAULT: &str =
    "https://app.ftscrt.com/api/food/v1/food-dietary-preference-vote";
pub const FOOD_TYPES_URL_DEFAULT: &str =
    "https://app.ftscrt.com/api/food/v1/food-dietary-preference-types";
pub const RECIPE_COUNT_URL_DEFAULT: &str =
    "https://app.ftscrt.com/api/cookbook-recipes/v1/count-per-market";
pub const SCAN_URL_DEFAULT: &str = "https://app.ftscrt.com/api/barcode-verification/v1/scan";
pub const REGISTER_URL_DEFAULT: &str = "https://app.ftscrt.com/api/account/v1/register";
pub const FORGOT_URL_DEFAULT: &str = "https://app.ftscrt.com/api/account/v1/forgot-password";
pub const RESET_URL_DEFAULT: &str = "https://app.ftscrt.com/api/account/v1/reset-password";
pub const USER_DETAILS_URL_DEFAULT: &str =
    "https://app.ftscrt.com/api/user/v3/user-account-details";
pub const CHANGE_USERNAME_URL_DEFAULT: &str =
    "https://app.ftscrt.com/api/account/v1/change-user-name";
pub const SERVER_BASE_DEFAULT: &str = "https://android.fatsecret.com/android/";
pub const APP_VERSION_DEFAULT: &str = "11.8.0.5";
pub const DEVICE_MODEL_DEFAULT: &str = "android";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_auth_url")]
    pub auth_url: String,
    #[serde(default = "default_food_search_url")]
    pub food_search_url: String,
    #[serde(default = "default_food_popular_url")]
    pub food_popular_url: String,
    #[serde(default = "default_food_vote_url")]
    pub food_vote_url: String,
    #[serde(default = "default_food_types_url")]
    pub food_types_url: String,
    #[serde(default = "default_recipe_count_url")]
    pub recipe_count_url: String,
    #[serde(default = "default_scan_url")]
    pub scan_url: String,
    #[serde(default = "default_journal_url")]
    pub journal_url: String,
    #[serde(default = "default_register_url")]
    pub register_url: String,
    #[serde(default = "default_forgot_url")]
    pub forgot_url: String,
    #[serde(default = "default_reset_url")]
    pub reset_url: String,
    #[serde(default = "default_user_details_url")]
    pub user_details_url: String,
    #[serde(default = "default_change_username_url")]
    pub change_username_url: String,
    #[serde(default = "default_server_base")]
    pub server_base: String,
    #[serde(default = "default_device_model")]
    pub device_model: String,
    #[serde(default = "default_app_version")]
    pub app_version: String,
    /// Stable device identity (Firebase Installation ID), sent as `c_d`/`c_desc`.
    /// Optional; mint via Firebase Installations API (see docs), or leave empty.
    #[serde(default)]
    pub device_id: Option<String>,
}
fn default_auth_url() -> String {
    AUTH_URL_DEFAULT.to_string()
}
fn default_food_search_url() -> String {
    FOOD_SEARCH_URL_DEFAULT.to_string()
}
fn default_journal_url() -> String {
    JOURNAL_URL_DEFAULT.to_string()
}
fn default_food_popular_url() -> String {
    FOOD_POPULAR_URL_DEFAULT.to_string()
}
fn default_food_vote_url() -> String {
    FOOD_VOTE_URL_DEFAULT.to_string()
}
fn default_food_types_url() -> String {
    FOOD_TYPES_URL_DEFAULT.to_string()
}
fn default_recipe_count_url() -> String {
    RECIPE_COUNT_URL_DEFAULT.to_string()
}
fn default_scan_url() -> String {
    SCAN_URL_DEFAULT.to_string()
}
fn default_register_url() -> String {
    REGISTER_URL_DEFAULT.to_string()
}
fn default_forgot_url() -> String {
    FORGOT_URL_DEFAULT.to_string()
}
fn default_reset_url() -> String {
    RESET_URL_DEFAULT.to_string()
}
fn default_user_details_url() -> String {
    USER_DETAILS_URL_DEFAULT.to_string()
}
fn default_change_username_url() -> String {
    CHANGE_USERNAME_URL_DEFAULT.to_string()
}
fn default_server_base() -> String {
    SERVER_BASE_DEFAULT.to_string()
}
fn default_device_model() -> String {
    DEVICE_MODEL_DEFAULT.to_string()
}
fn default_app_version() -> String {
    APP_VERSION_DEFAULT.to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            auth_url: default_auth_url(),
            food_search_url: default_food_search_url(),
            food_popular_url: default_food_popular_url(),
            food_vote_url: default_food_vote_url(),
            food_types_url: default_food_types_url(),
            recipe_count_url: default_recipe_count_url(),
            scan_url: default_scan_url(),
            journal_url: default_journal_url(),
            register_url: default_register_url(),
            forgot_url: default_forgot_url(),
            reset_url: default_reset_url(),
            user_details_url: default_user_details_url(),
            change_username_url: default_change_username_url(),
            server_base: default_server_base(),
            device_model: default_device_model(),
            app_version: default_app_version(),
            device_id: None,
        }
    }
}

impl AppConfig {
    /// Resolve the config: env overrides, then TOML file, then defaults.
    /// Missing files are fine — defaults apply.
    pub fn resolve(explicit: Option<&Path>) -> Result<Self> {
        let mut cfg = match explicit.map(PathBuf::from).or_else(default_config_path) {
            Some(path) if path.exists() => {
                let text = std::fs::read_to_string(&path)?;
                toml::from_str(&text)
                    .map_err(|e| AppError::Msg(format!("bad config {path:?}: {e}")))?
            }
            Some(path) if explicit.is_some() => {
                return Err(AppError::Msg(format!("config file not found: {path:?}")));
            }
            _ => Self::default(),
        };
        if let Ok(v) = std::env::var("FATSECRET_FOOD_POPULAR_URL") {
            cfg.food_popular_url = v;
        }
        if let Ok(v) = std::env::var("FATSECRET_FOOD_VOTE_URL") {
            cfg.food_vote_url = v;
        }
        if let Ok(v) = std::env::var("FATSECRET_FOOD_TYPES_URL") {
            cfg.food_types_url = v;
        }
        if let Ok(v) = std::env::var("FATSECRET_RECIPE_COUNT_URL") {
            cfg.recipe_count_url = v;
        }
        if let Ok(v) = std::env::var("FATSECRET_SCAN_URL") {
            cfg.scan_url = v;
        }
        if let Ok(v) = std::env::var("FATSECRET_AUTH_URL") {
            cfg.auth_url = v;
        }
        if let Ok(v) = std::env::var("FATSECRET_FOOD_SEARCH_URL") {
            cfg.food_search_url = v;
        }
        if let Ok(v) = std::env::var("FATSECRET_JOURNAL_URL") {
            cfg.journal_url = v;
        }
        if let Ok(v) = std::env::var("FATSECRET_REGISTER_URL") {
            cfg.register_url = v;
        }
        if let Ok(v) = std::env::var("FATSECRET_FORGOT_URL") {
            cfg.forgot_url = v;
        }
        if let Ok(v) = std::env::var("FATSECRET_RESET_URL") {
            cfg.reset_url = v;
        }
        if let Ok(v) = std::env::var("FATSECRET_USER_DETAILS_URL") {
            cfg.user_details_url = v;
        }
        if let Ok(v) = std::env::var("FATSECRET_CHANGE_USERNAME_URL") {
            cfg.change_username_url = v;
        }
        if let Ok(v) = std::env::var("FATSECRET_SERVER_BASE") {
            cfg.server_base = v;
        }
        if let Ok(v) = std::env::var("FATSECRET_DEVICE_MODEL") {
            cfg.device_model = v;
        }
        if let Ok(v) = std::env::var("FATSECRET_APP_VERSION") {
            cfg.app_version = v;
        }
        if let Ok(v) = std::env::var("FATSECRET_DEVICE_ID") {
            cfg.device_id = Some(v);
        }
        if cfg.device_model.is_empty() {
            return Err(AppError::NoDevice);
        }
        Ok(cfg)
    }
}

fn default_config_path() -> Option<PathBuf> {
    BaseDirs::new().map(|b| b.config_dir().join("fatsecret-cli").join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn defaults_apply_without_file_or_env() {
        let _guard = ENV_LOCK.lock();
        for key in [
            "FATSECRET_AUTH_URL",
            "FATSECRET_FOOD_SEARCH_URL",
            "FATSECRET_JOURNAL_URL",
            "FATSECRET_DEVICE_MODEL",
            "FATSECRET_APP_VERSION",
        ] {
            // SAFETY: test-scoped env mutation under a process-wide lock.
            unsafe { std::env::remove_var(key) };
        }
        let cfg = AppConfig::resolve(Some(Path::new("/nonexistent.toml"))).unwrap_err();
        assert!(matches!(cfg, AppError::Msg(_)));
        let cfg = AppConfig::resolve(None).unwrap_or_default();
        assert_eq!(cfg.auth_url, AUTH_URL_DEFAULT);
    }

    #[test]
    fn env_overrides_defaults() {
        let _guard = ENV_LOCK.lock();
        // SAFETY: test-scoped env mutation under a process-wide lock.
        unsafe { std::env::set_var("FATSECRET_DEVICE_MODEL", "Pixel") };
        let cfg = AppConfig::resolve(None).unwrap_or_default();
        assert_eq!(cfg.device_model, "Pixel");
        unsafe { std::env::remove_var("FATSECRET_DEVICE_MODEL") };
    }
}
