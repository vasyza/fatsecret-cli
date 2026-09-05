use crate::api::FsClient;
use crate::cli::{FoodsAction, FoodsArgs, OutputFormat};
use crate::error::{AppError, Result};
use crate::output::{emit, render_food_get, render_foods_search, render_types};

pub async fn run(client: &FsClient, format: OutputFormat, args: FoodsArgs) -> Result<()> {
    match args.action {
        FoodsAction::Search { query, page, size } => {
            let v = client.foods_search(&query, page, size).await?;
            let (human, plain) = render_foods_search(&v);
            emit(format, human.trim_end(), plain.trim_end(), &v)
        }
        FoodsAction::Get { id } => {
            let v = client.food_get(id).await?;
            let (human, plain) = render_food_get(&v);
            emit(format, human.trim_end(), plain.trim_end(), &v)
        }
        FoodsAction::Popular { ids } => {
            let v = client.foods_popular(&ids).await?;
            emit(format, "popular servings (see --format json)", "", &v)
        }
        FoodsAction::VotePreference { id, votes } => {
            let parsed = parse_votes(&votes)?;
            let v = client.vote_preference(id, &parsed).await?;
            emit(format, "vote recorded", "OK", &v)
        }
        FoodsAction::PreferenceTypes => {
            let v = client.dietary_types().await?;
            let (human, plain) = render_types(&v);
            emit(format, human.trim_end(), plain.trim_end(), &v)
        }
        FoodsAction::Barcode { gtin } => {
            let scanned = client.barcode_scan(gtin.trim()).await?;
            let Some(id) = find_food_id(&scanned) else {
                return emit(
                    format,
                    "no food found for this barcode",
                    "NOT_FOUND",
                    &scanned,
                );
            };
            let v = client.food_get(id).await?;
            let (human, plain) = render_food_get(&v);
            emit(format, human.trim_end(), plain.trim_end(), &v)
        }
    }
}

/// Extract a food id from a barcode-scan response, searching a few known
/// shapes; `None` means unrecognized (caller prints the raw response).
fn find_food_id(v: &serde_json::Value) -> Option<i64> {
    for key in ["foodId", "food_id", "recipeId", "recipeid", "id"] {
        if let Some(obj) = v.get(key).or_else(|| {
            v.as_object()
                .and_then(|m| m.values().find_map(|inner| inner.as_object()?.get(key)))
        }) {
            if let Some(n) = obj.as_i64() {
                return Some(n);
            }
            if let Some(s) = obj.as_str() {
                if let Ok(n) = s.parse::<i64>() {
                    return Some(n);
                }
            }
        }
    }
    None
}

fn parse_votes(raw: &[String]) -> Result<Vec<(i64, String)>> {
    raw.iter()
        .map(|s| {
            let (t, v) = s
                .split_once('=')
                .ok_or_else(|| AppError::Msg(format!("bad --vote {s:?}, want TYPE=VALUE")))?;
            let type_id: i64 = t
                .trim()
                .parse()
                .map_err(|_| AppError::Msg(format!("bad vote type {t:?}, want a number")))?;
            Ok((type_id, v.trim().to_string()))
        })
        .collect()
}
