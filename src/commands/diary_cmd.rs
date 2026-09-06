use crate::api::{FsClient, journal_entry};
use crate::auth::device::today_ymd;
use crate::cli::{DiaryAction, DiaryArgs, OutputFormat};
use crate::error::{AppError, Result};
use crate::output::{emit, render_diary_day};

/// Named meals to server ids (from the app's `*_ID` constants).
pub(crate) fn meal_id(name: &str, override_id: Option<i64>) -> Result<i64> {
    if let Some(id) = override_id {
        return Ok(id);
    }
    match name.to_lowercase().as_str() {
        "breakfast" => Ok(1),
        "lunch" => Ok(2),
        "dinner" => Ok(3),
        "snack" => Ok(4),
        "prebreakfast" | "pre-breakfast" => Ok(5),
        "secondbreakfast" | "second-breakfast" => Ok(6),
        "other" => Ok(0),
        _ => Err(AppError::Msg(format!(
            "unknown meal {name:?}; want breakfast|lunch|dinner|snack|other (or --meal-id INT)"
        ))),
    }
}

pub(crate) fn recorded_date(date: &Option<String>) -> Result<(String, i64)> {
    let day = date.clone().unwrap_or_else(today_ymd);
    Ok((day.clone(), date_to_days(&day)?))
}

pub(crate) fn date_to_days(day: &str) -> Result<i64> {
    let parts: Vec<&str> = day.split('-').collect();
    let (Some(y), Some(m), Some(d)) = (
        parts.first().and_then(|s| s.parse::<i64>().ok()),
        parts.get(1).and_then(|s| s.parse::<i64>().ok()),
        parts.get(2).and_then(|s| s.parse::<i64>().ok()),
    ) else {
        return Err(AppError::Msg(format!("bad date {day:?}, want YYYY-MM-DD")));
    };
    if !(1..=12).contains(&m) || d < 1 || d > days_in_month(y, m) {
        return Err(AppError::Msg(format!("bad date {day:?}, want YYYY-MM-DD")));
    }
    Ok(days_from_civil(y, m, d))
}

fn is_leap_year(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(y) => 29,
        2 => 28,
        _ => 0,
    }
}

pub(crate) fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (m + 9).rem_euclid(12);
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub async fn run(client: &FsClient, format: OutputFormat, args: DiaryArgs) -> Result<()> {
    match args.action {
        DiaryAction::Day { date } => {
            let (_, recorded) = recorded_date(&date)?;
            let v = client.diary_day(recorded).await?;
            let (human, plain) = render_diary_day(&v);
            emit(format, human.trim_end(), plain.trim_end(), &v)
        }
        DiaryAction::Cp { from, date } => {
            let (_, recorded_from) = recorded_date(&Some(from))?;
            let (_, recorded) = recorded_date(&date)?;
            let src = client.diary_day(recorded_from).await?;
            let entries = day_entries(&src);
            if entries.is_empty() {
                return Err(AppError::Msg(
                    "nothing to copy: source day has no entries".to_string(),
                ));
            }
            let mut copied = 0;
            for e in entries {
                let entry = journal_entry(
                    0,
                    entry_i64(e, "recipeid"),
                    &entry_str(e, "name"),
                    entry_i64(e, "recipeportionid"),
                    entry_f64(e, "portionamount"),
                    entry_i64(e, "meal"),
                    "",
                );
                let v = client.journal_update(recorded, &[entry], &[]).await?;
                if let Some(detail) = journal_failed_entries(&v) {
                    return Err(AppError::Api {
                        code: "200".to_string(),
                        message: detail,
                    });
                }
                copied += 1;
            }
            emit(
                format,
                &format!("copied {copied} entries"),
                "OK",
                &serde_json::json!({"copied": copied}),
            )
        }
        DiaryAction::Add {
            food_id,
            serving_id,
            meal,
            meal_id: meal_override,
            date,
            units,
        } => {
            let (_, recorded) = recorded_date(&date)?;
            // Name refresh from details for accuracy.
            let details = client.food_get(food_id).await?;
            let name = details.get("title").and_then(|t| as_text(t)).unwrap_or("?");
            let entry = journal_entry(
                0,
                food_id,
                name,
                serving_id,
                units,
                meal_id(&meal, meal_override)?,
                "",
            );
            let v = client.journal_update(recorded, &[entry], &[]).await?;
            if let Some(detail) = journal_failed_entries(&v) {
                return Err(AppError::Api {
                    code: "200".to_string(),
                    message: detail,
                });
            }
            emit(format, "entry logged", "OK", &v)
        }
        DiaryAction::Rm { entry_id, date } => {
            let (_, recorded) = recorded_date(&date)?;
            // The app serializes `deletes` as a bare id array, not objects.
            let del = serde_json::json!(entry_id);
            let v = client.journal_update(recorded, &[], &[del]).await?;
            emit(format, "entry deleted", "OK", &v)
        }
    }
}

fn as_text(v: &serde_json::Value) -> Option<&str> {
    match v {
        serde_json::Value::String(s) => Some(s),
        serde_json::Value::Object(map) if map.len() == 1 => {
            map.get("$text").and_then(|t| t.as_str())
        }
        _ => None,
    }
}

/// Entry rows of a day payload (`recipejournalentry` list or single object).
fn day_entries(v: &serde_json::Value) -> Vec<&serde_json::Value> {
    match v.get("recipejournalentry") {
        Some(serde_json::Value::Array(items)) => items.iter().collect(),
        Some(single) if single.is_object() => vec![single],
        _ => vec![],
    }
}

/// Numeric entry field tolerating string-or-number wire values.
fn entry_i64(e: &serde_json::Value, key: &str) -> i64 {
    e.get(key)
        .and_then(|v| {
            v.as_i64().or_else(|| {
                v.as_str()
                    .and_then(|s| s.parse::<f64>().ok())
                    .map(|n| n as i64)
            })
        })
        .unwrap_or(0)
}

/// Float entry field tolerating string-or-number wire values.
fn entry_f64(e: &serde_json::Value, key: &str) -> f64 {
    e.get(key)
        .and_then(|v| {
            v.as_f64()
                .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
        })
        .unwrap_or(1.0)
}

/// Text entry field with `?` fallback (matches `diary add` behavior).
fn entry_str(e: &serde_json::Value, key: &str) -> String {
    e.get(key)
        .and_then(|v| as_text(v).map(|s| s.to_string()))
        .unwrap_or("?".to_string())
}

/// Server-side per-entry failures from `update-journal-entries`.
/// Live responses carry HTTP 200 with `{failedEntries: [{errorCode, references}]}`;
/// a non-empty array means the entry was rejected despite the 200.
pub(crate) fn journal_failed_entries(v: &serde_json::Value) -> Option<String> {
    let entries = v.get("failedEntries")?.as_array()?;
    let first = entries.first()?;
    let code = first
        .get("errorCode")
        .map(|c| c.to_string())
        .unwrap_or("?".to_string());
    let refs = first
        .get("references")
        .map(|r| r.to_string())
        .unwrap_or("[]".to_string());
    Some(format!(
        "failedEntries[0]: errorCode={code} references={refs}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_meal_names() {
        assert_eq!(meal_id("breakfast", None).unwrap(), 1);
        assert_eq!(meal_id("LUNCH", None).unwrap(), 2);
        assert_eq!(meal_id("dinner", None).unwrap(), 3);
        assert_eq!(meal_id("snack", None).unwrap(), 4);
        assert_eq!(meal_id("other", None).unwrap(), 0);
        assert_eq!(meal_id("brunch", Some(9)).unwrap(), 9);
        assert!(meal_id("brunch", None).is_err());
    }

    #[test]
    fn converts_dates_to_days() {
        // 1970-01-01 is day 0; 2026-09-05 is a known Friday check below.
        assert_eq!(date_to_days("1970-01-01").unwrap(), 0);
        assert_eq!(date_to_days("1970-01-02").unwrap(), 1);
        assert!(date_to_days("not-a-date").is_err());
        assert!(date_to_days("2026-13-45").is_err());
        assert!(date_to_days("2026-00-10").is_err());
        assert!(date_to_days("2026-01-00").is_err());
        assert!(date_to_days("2026-04-31").is_err());
        assert!(date_to_days("2023-02-29").is_err());
        assert!(date_to_days("2024-02-29").is_ok());
        assert!(date_to_days("2000-02-29").is_ok());
        assert!(date_to_days("1900-02-29").is_err());
        assert_eq!(
            crate::auth::device::from_days(date_to_days("2026-09-05").unwrap()),
            "2026-09-05"
        );
    }

    #[test]
    fn recorded_date_uses_explicit_day() {
        let (day, recorded) = recorded_date(&Some("2026-09-05".to_string())).unwrap();
        assert_eq!(day, "2026-09-05");
        assert_eq!(recorded, date_to_days("2026-09-05").unwrap());
        let err = recorded_date(&Some("not-a-date".to_string())).unwrap_err();
        assert!(err.to_string().contains("bad date"));
        assert!(err.to_string().contains("want YYYY-MM-DD"));
    }

    #[test]
    fn surfaces_failed_entries() {
        let v = serde_json::json!({"failedEntries": [{"errorCode": 200, "references": ["0"]}]});
        let detail = journal_failed_entries(&v).expect("must surface rejection");
        assert!(detail.contains("200"), "code: {detail}");
        assert!(journal_failed_entries(&serde_json::json!({"failedEntries": []})).is_none());
        assert!(journal_failed_entries(&serde_json::json!({})).is_none());
    }

    #[test]
    fn history_dates_resolve_to_days() {
        // Past dates are valid day selectors (same page + dt on the wire).
        assert!(date_to_days("2000-01-01").unwrap() < date_to_days("2026-09-05").unwrap());
        let (_, recorded) = recorded_date(&Some("2026-09-04".to_string())).unwrap();
        assert_eq!(recorded, date_to_days("2026-09-05").unwrap() - 1);
    }

    #[test]
    fn extracts_copyable_entries() {
        let v = serde_json::json!({
            "dateint": "20700",
            "recipejournalentry": [
                {"id": "1", "recipeid": "39715", "name": "Oats",
                 "recipeportionid": 62446, "portionamount": "3.25", "meal": "1"},
            ],
        });
        let entries = day_entries(&v);
        assert_eq!(entries.len(), 1);
        let e = entries[0];
        assert_eq!(entry_i64(e, "recipeid"), 39715);
        assert_eq!(entry_i64(e, "recipeportionid"), 62446);
        assert_eq!(entry_i64(e, "meal"), 1);
        assert!((entry_f64(e, "portionamount") - 3.25).abs() < 1e-9);
        assert_eq!(entry_str(e, "name"), "Oats");
        assert!(day_entries(&serde_json::json!({"dateint": "20701"})).is_empty());
    }
}
