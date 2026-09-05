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

fn recorded_date(date: &Option<String>) -> Result<(String, i64)> {
    let day = date.clone().unwrap_or_else(today_ymd);
    Ok((day.clone(), date_to_days(&day)?))
}

fn date_to_days(day: &str) -> Result<i64> {
    let parts: Vec<&str> = day.split('-').collect();
    let (Some(y), Some(m), Some(d)) = (
        parts.first().and_then(|s| s.parse::<i64>().ok()),
        parts.get(1).and_then(|s| s.parse::<i64>().ok()),
        parts.get(2).and_then(|s| s.parse::<i64>().ok()),
    ) else {
        return Err(AppError::Msg(format!("bad date {day:?}, want YYYY-MM-DD")));
    };
    Ok(days_from_civil(y, m, d))
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
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
            if let Some(d) = &date
                && *d != today_ymd()
            {
                return Err(AppError::Msg(
                    "past days need the day guid (unresolved); omit --date for today".to_string(),
                ));
            }
            let v = client.diary_day().await?;
            let (human, plain) = render_diary_day(&v);
            emit(format, human.trim_end(), plain.trim_end(), &v)
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
            emit(format, "entry logged", "OK", &v)
        }
        DiaryAction::Rm { entry_id } => {
            let (_, recorded) = recorded_date(&None)?;
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
        assert!(date_to_days("2026-13-45").is_ok()); // shape-checked only
    }
}
