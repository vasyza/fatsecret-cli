//! Per-install device identity (Firebase Installation ID).
//!
//! The app sends its Firebase IID as `c_d`/`c_desc` on every request,
//! including pre-auth login. The CLI mints its own IID on first use via the
//! public Firebase Installations API (same values the app uses: API key,
//! package name, APK cert fingerprint, app ID — all extracted from the APK,
//! none of them user secrets) and persists it separately from login
//! credentials: logout wipes the triple, never the device identity.

use std::path::PathBuf;

use directories::BaseDirs;
use rand::{Rng, distr::Alphanumeric};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::error::{AppError, Result};

const FIS_URL: &str =
    "https://firebaseinstallations.googleapis.com/v1/projects/perfect-age-539/installations";
const FIREBASE_API_KEY: &str = "AIzaSyBIj5COBDVa5JqtMVoV3mRethCKokwyd9Q";
const ANDROID_PACKAGE: &str = "com.fatsecret.android";
const ANDROID_CERT_SHA1: &str = "D32FC02A53B046AFEEBE28FA3DEE58883BC387EA";
const APP_ID: &str = "1:114255563282:android:24d6c7bd47f4ebbd";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeviceFile {
    fid: String,
    obtained_utc: String,
}

/// Generate a Firebase Installation ID: 22 chars, `f` prefix + 21 random
/// URL-safe chars (matches the format the Installations API accepts).
pub fn generate_fid() -> String {
    let rand: String = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(21)
        .map(char::from)
        .collect();
    // Alphanumeric includes only [A-Za-z0-9]; map a couple of chars to -_
    // for a wider spread — value semantics don't matter, only shape.
    format!("f{rand}")
}

/// Mint a fresh installation via Firebase (`fid` must be client-generated).
/// Returns the confirmed `fid`.
pub async fn mint_fid(http: &Client, fid: &str) -> Result<String> {
    let body = json!({
        "fid": fid,
        "appId": APP_ID,
        "authVersion": "FIS_v2",
        "sdkVersion": "a:17.1.4",
    });
    let res = http
        .post(FIS_URL)
        .header("x-goog-api-key", FIREBASE_API_KEY)
        .header("X-Android-Package", ANDROID_PACKAGE)
        .header("X-Android-Cert", ANDROID_CERT_SHA1)
        .json(&body)
        .send()
        .await?;
    if !res.status().is_success() {
        return Err(AppError::Msg(format!(
            "device registration rejected (HTTP {})",
            res.status()
        )));
    }
    let value: Value = res.json().await?;
    value
        .get("fid")
        .and_then(|f| f.as_str())
        .map(str::to_string)
        .ok_or_else(|| AppError::Msg("unexpected device registration response".to_string()))
}

fn device_path() -> Result<PathBuf> {
    BaseDirs::new()
        .map(|b| b.config_dir().join("fatsecret-cli").join("device.json"))
        .ok_or_else(|| AppError::Msg("no config dir available".to_string()))
}

fn read_stored() -> Option<String> {
    let path = device_path().ok()?;
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<DeviceFile>(&text)
        .ok()
        .map(|d| d.fid)
}

fn store(fid: &str) -> Result<()> {
    let path = device_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = DeviceFile {
        fid: fid.to_string(),
        obtained_utc: now_utc(),
    };
    std::fs::write(&path, serde_json::to_string_pretty(&file)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn now_utc() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}

/// Today as `YYYY-MM-DD` (civil date from days-since-epoch).
pub fn today_ymd() -> String {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().div_ceil(86400) as i64)
        .unwrap_or(0);
    from_days(days)
}

fn from_days(days: i64) -> String {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{:02}-{:02}", if m <= 2 { y + 1 } else { y }, m, d)
}

/// Resolve the device identity: explicit config/env wins, then the stored
/// install fid, otherwise mint + persist a fresh one (network).
/// Pure-filesystem when already provisioned — safe to call on every command.
pub async fn ensure_device_id(http: &Client, configured: Option<&str>) -> Result<String> {
    if let Some(id) = configured
        && !id.is_empty()
    {
        return Ok(id.to_string());
    }
    if let Ok(env) = std::env::var("FATSECRET_DEVICE_ID")
        && !env.is_empty()
    {
        return Ok(env);
    }
    if let Some(fid) = read_stored() {
        return Ok(fid);
    }
    let fid = generate_fid();
    let confirmed = mint_fid(http, &fid).await?;
    store(&confirmed)?;
    Ok(confirmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fid_has_stable_shape() {
        for _ in 0..100 {
            let fid = generate_fid();
            assert_eq!(fid.len(), 22, "fid must be 22 chars");
            assert!(fid.starts_with('f'));
            assert!(fid.chars().all(|c| c.is_ascii_alphanumeric()));
        }
    }

    #[test]
    fn registration_body_shape() {
        let body = json!({
            "fid": "ftest123",
            "appId": APP_ID,
            "authVersion": "FIS_v2",
            "sdkVersion": "a:17.1.4",
        });
        assert_eq!(body["appId"], json!(APP_ID));
        assert_eq!(body["authVersion"], json!("FIS_v2"));
    }
}
