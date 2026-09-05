//! Output rendering: stdout carries data only.
//!
//! Shapes below are observed from the live modern API (2026-09-04):
//! search → `{currentpage, recipes[], resultsPerPage, totalresults}`,
//! item → `{id: number, title, energyPerPortion, defaultPortionDescription,
//! ...}`. `--format json` always passes the raw value through.

use std::io::{self, Write};

use serde_json::Value;

use crate::cli::OutputFormat;
use crate::error::Result;

pub fn emit(format: OutputFormat, human: &str, plain: &str, json: &Value) -> Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    match format {
        OutputFormat::Human => {
            writeln!(out, "{human}")?;
        }
        OutputFormat::Plain => {
            writeln!(out, "{plain}")?;
        }
        OutputFormat::Json => {
            serde_json::to_writer_pretty(&mut out, json)?;
            writeln!(out)?;
        }
    }
    Ok(())
}

/// First present key as display string (strings raw, numbers formatted,
/// quick-xml `{"$text": ...}` nodes unwrapped).
fn field(item: &Value, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|k| item.get(*k))
        .map(unwrap_text)
        .map(|v| match v {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            // Multi-part names arrive as string arrays (e.g. recipe titles).
            Value::Array(parts) => {
                let joined = parts
                    .iter()
                    .filter_map(scalar_opt)
                    .collect::<Vec<_>>()
                    .join(" ");
                if joined.is_empty() {
                    "?".to_string()
                } else {
                    joined
                }
            }
            _ => "?".to_string(),
        })
        .unwrap_or_else(|| "?".to_string())
}

fn unwrap_text(v: &Value) -> &Value {
    match v {
        Value::Object(map) if map.len() == 1 => map.get("$text").unwrap_or(v),
        _ => v,
    }
}

fn as_items(v: &Value) -> Vec<&Value> {
    match v {
        Value::Array(items) => items.iter().collect(),
        Value::Null => vec![],
        single => vec![single],
    }
}

const NAME_KEYS: &[&str] = &["title", "recipe_name", "food_name", "name"];
const ID_KEYS: &[&str] = &["id", "recipe_id", "food_id"];

/// Human + plain rendering of the modern food-search response.
pub fn render_foods_search(v: &Value) -> (String, String) {
    let total = v
        .get("totalresults")
        .map(|t| match t {
            Value::String(s) => s.clone(),
            _ => t.to_string(),
        })
        .unwrap_or_else(|| "?".to_string());
    let items = v
        .get("recipes")
        .or_else(|| v.get("foods"))
        .map(as_items)
        .unwrap_or_default();

    let mut human = format!("results: {} shown, {total} total\n", items.len());
    let mut plain = String::new();
    for item in items {
        let id = field(item, ID_KEYS);
        let name = field(item, NAME_KEYS);
        let kcal = field(item, &["energyPerPortion", "calories"]);
        let portion = field(item, &["defaultPortionDescription", "serving_description"]);
        let macros = nutrition_detail(
            item,
            &[
                ("P", "proteinPerPortion"),
                ("C", "carbohydratePerPortion"),
                ("F", "fatPerPortion"),
            ],
        );
        if macros == "?" {
            human.push_str(&format!("{id}  {name} — {kcal} energy / {portion}\n"));
        } else {
            human.push_str(&format!(
                "{id}  {name} — {kcal} energy / {portion} · {macros}\n"
            ));
        }
        plain.push_str(&format!("{id}\t{name}\t{kcal}\t{portion}\n"));
    }
    (human, plain)
}

/// Tolerant account-details rendering. Observed live shape (2026-09-04):
/// `{commencementWeightKg, dateCreatedUtc, gender, goalWeightKg,
/// marketingUserType, yearOfBirth}` — no identity keys at all.
pub fn render_account_show(v: &Value) -> (String, String) {
    const KEYS: &[&str] = &[
        "userName",
        "username",
        "memberName",
        "email",
        "gender",
        "yearOfBirth",
        "commencementWeightKg",
        "goalWeightKg",
        "marketingUserType",
        "dateCreatedUtc",
    ];
    let root = v.get("member").or(v.get("user")).unwrap_or(v);
    let mut human = String::new();
    let mut plain = String::new();
    if let Some(map) = root.as_object() {
        for key in KEYS {
            if let Some(val) = map.get(*key) {
                let s = scalar(val);
                human.push_str(&format!("{key}: {s}\n"));
                plain.push_str(&format!("{key}\t{s}\n"));
            }
        }
    }
    if human.is_empty() {
        human = "account details differ from the known shape (see --format json)".to_string();
    }
    (human, plain)
}

pub fn render_account_settings(v: &Value) -> (String, String) {
    render_scalars(
        v,
        "settings differ from the known shape (see --format json)",
    )
}

/// Generic scalar dump: descends into single-child wrappers, then lists
/// `key: value` pairs (quick-xml `$text` shown bare).
pub fn render_scalars(v: &Value, empty: &str) -> (String, String) {
    let mut current = v;
    loop {
        let descend = match current.as_object() {
            Some(map) if map.len() == 1 => match map.values().next() {
                Some(only) if only.is_object() => Some(only),
                _ => None,
            },
            _ => None,
        };
        match descend {
            Some(next) => current = next,
            None => break,
        }
    }
    let mut human = String::new();
    let mut plain = String::new();
    if let Some(map) = current.as_object() {
        for (k, val) in map {
            if let Some(s) = scalar_opt(val) {
                // quick-xml text nodes surface as `$text` — show the value bare.
                if k == "$text" {
                    human.push_str(&format!("{s}\n"));
                } else {
                    human.push_str(&format!("{k}: {s}\n"));
                }
                plain.push_str(&format!("{k}\t{s}\n"));
            }
        }
    }
    if human.is_empty() {
        human = empty.to_string();
    }
    (human, plain)
}
/// Food details: macro header per default portion plus portion rows with
/// gram weights. Portion objects carry no energy keys, so portion energy is
/// scaled linearly from the default portion by gram weight.
pub fn render_food_get(v: &Value) -> (String, String) {
    let root = v
        .get("recipe")
        .or(v.get("food"))
        .or(v.get("Recipe"))
        .unwrap_or(v);
    let name = field(root, &["title", "name", "recipe_name", "food_name"]);
    let mut human = if name == "?" {
        String::new()
    } else {
        format!("{name}\n")
    };
    let kcal = field(root, &["energyPerPortion", "calories", "energy"]);
    let macros = nutrition_detail(
        root,
        &[
            ("P", "proteinPerPortion"),
            ("C", "carbohydratePerPortion"),
            ("F", "fatPerPortion"),
            ("fiber", "fiberPerPortion"),
        ],
    );
    if kcal != "?" || macros != "?" {
        human.push_str(&format!("{kcal} energy / portion · {macros}\n"));
    }
    // Reference grams of the default portion for energy scaling.
    let default_id = field(root, &["defaultPortionID", "defaultPortionId"]);
    let base_kcal: f64 = kcal.parse().unwrap_or(f64::NAN);
    let mut plain = String::new();
    let mut rows = 0;
    for key in [
        "portions",
        "alternatePortions",
        "recipeportion",
        "servings",
        "serving",
        "Portions",
    ] {
        if let Some(list) = root.get(key) {
            let portion_items = as_items(list);
            let base_grams = portion_items
                .iter()
                .find(|p| field(p, &["id", "portionId", "portionid"]) == default_id)
                .map(|p| field(p, &["gramWeight", "grams", "gram_weight"]).parse::<f64>())
                .and_then(|r| r.ok())
                .unwrap_or(f64::NAN);
            for item in portion_items {
                let pid = field(item, &["id", "portionId", "portionid", "serving_id"]);
                let desc = field(
                    item,
                    &[
                        "description",
                        "servingDescription",
                        "serving_description",
                        "portionDescription",
                    ],
                );
                let grams = field(item, &["gramWeight", "grams", "gram_weight"])
                    .parse::<f64>()
                    .unwrap_or(f64::NAN);
                let scaled = if base_kcal.is_finite()
                    && base_grams.is_finite()
                    && grams.is_finite()
                    && base_grams > 0.0
                {
                    format!("{:.0}", base_kcal * grams / base_grams)
                } else {
                    "?".to_string()
                };
                human.push_str(&format!("  {pid}  {desc} ({grams} g): {scaled} energy\n"));
                plain.push_str(&format!("{pid}\t{desc}\t{scaled}\n"));
                rows += 1;
            }
            if rows > 0 {
                break;
            }
        }
    }
    if rows == 0 && name == "?" {
        human = "details differ from the known shape (see --format json)".to_string();
    }
    (human, plain)
}

/// Dietary-preference types: `{preferences: [{typeId, name, type, ordinal}]}`.
pub fn render_types(v: &Value) -> (String, String) {
    let items = v.get("preferences").map(as_items).unwrap_or_default();
    let mut human = String::new();
    let mut plain = String::new();
    for item in items {
        let id = field(item, &["typeId", "id"]);
        let name = field(item, &["name", "title"]);
        let kind = field(item, &["type"]);
        human.push_str(&format!("{id}  {name} ({kind})\n"));
        plain.push_str(&format!("{id}\t{name}\t{kind}\n"));
    }
    if human.is_empty() {
        human = "no types returned (see --format json)".to_string();
    }
    (human, plain)
}

/// Tolerant diary-day rendering: entries hide under several candidate keys;
/// grouped by meal with kcal subtotals like the app.
pub fn render_diary_day(v: &Value) -> (String, String) {
    let root = v
        .get("recipeJournalDay")
        .or(v.get("day"))
        .or(v.get("RecipeJournalDay"))
        .unwrap_or(v);
    let mut items: Vec<&Value> = Vec::new();
    if let Some(map) = root.as_object() {
        for key in [
            "recipejournalentry",
            "entries",
            "entry",
            "recipes",
            "recipe",
            "items",
            "foodEntries",
            "foodEntry",
        ] {
            if let Some(list) = map.get(key) {
                items = as_items(list);
                if !items.is_empty() {
                    break;
                }
            }
        }
    }
    if items.is_empty() {
        return (
            "no entries logged (see --format json)".to_string(),
            String::new(),
        );
    }
    // Group by meal, preserving first-seen order.
    let mut order: Vec<String> = Vec::new();
    let mut groups: std::collections::HashMap<String, Vec<&Value>> = Default::default();
    for item in items {
        let meal = meal_name(&field(item, &["meal", "mealName", "mealType"]));
        groups.entry(meal.clone()).or_default().push(item);
        if !order.contains(&meal) {
            order.push(meal);
        }
    }
    let mut human = day_totals(root);
    let mut plain = String::new();
    for meal in order {
        let mut subtotal = 0.0;
        let mut lines = String::new();
        let mut plains = String::new();
        for item in &groups[&meal] {
            let id = field(item, &["id", "entryId", "food_entry_id"]);
            let name = field(item, &["name", "title", "food_name"]);
            let kcal = field(
                item,
                &["energyPerEntry", "energyPerPortion", "calories", "energy"],
            );
            let kcal_num: f64 = kcal.parse().unwrap_or(0.0);
            subtotal += kcal_num;
            lines.push_str(&format!("  {id}  {name} — {kcal} energy\n"));
            let detail = nutrition_detail(
                item,
                &[
                    ("P", "proteinPerEntry"),
                    ("C", "carbohydratePerEntry"),
                    ("F", "fatPerEntry"),
                    ("fiber", "fiberPerEntry"),
                ],
            );
            let serving = field(
                item,
                &[
                    "servingDescription",
                    "fullDescription",
                    "portionDescription",
                ],
            );
            if detail != "?" || serving != "?" {
                lines.push_str(&format!("      {serving} · {detail}\n"));
            }
            plains.push_str(&format!("{id}\t{meal}\t{name}\t{kcal}\n"));
        }
        human.push_str(&format!("[{meal}] subtotal {subtotal:.0} energy\n{lines}"));
        plain.push_str(&plains);
    }
    (human, plain)
}

/// Day totals header (`energyPerDay/proteinPerDay/...`); empty when absent.
fn day_totals(root: &Value) -> String {
    let energy = field(root, &["energyPerDay"]);
    if energy == "?" {
        return String::new();
    }
    let detail = nutrition_detail(
        root,
        &[
            ("P", "proteinPerDay"),
            ("C", "carbohydratePerDay"),
            ("F", "fatPerDay"),
        ],
    );
    format!("day total {energy} energy · {detail}\n")
}

/// Compact `P x · C y · F z` macro line; `?` when no key hits.
fn nutrition_detail(item: &Value, keys: &[(&str, &str)]) -> String {
    let parts: Vec<String> = keys
        .iter()
        .map(|(label, key)| format!("{label} {}", field(item, &[key])))
        .collect();
    if parts.iter().all(|p| p.ends_with(" ?")) {
        "?".to_string()
    } else {
        parts.join(" · ")
    }
}

/// Server meal ids to CLI meal names (mirrors `meal_id` in `diary_cmd`).
fn meal_name(raw: &str) -> String {
    match raw {
        "1" => "breakfast",
        "2" => "lunch",
        "3" => "dinner",
        "4" => "snack",
        "5" => "pre-breakfast",
        "6" => "second-breakfast",
        "0" => "other",
        _ => raw,
    }
    .to_string()
}

/// Tolerant recipe-list rendering: legacy pages nest lists under several
/// keys (`recipes`, `recipetypes`, `items`); items carry `id` + `title`.
pub fn render_recipes(v: &Value) -> (String, String) {
    let mut current = v;
    loop {
        match current.as_object() {
            Some(map) if map.len() == 1 => match map.values().next() {
                Some(only) if only.is_object() => {
                    current = only;
                    continue;
                }
                _ => break,
            },
            _ => break,
        }
    }
    let mut items: Vec<&Value> = Vec::new();
    if let Some(map) = current.as_object() {
        for key in [
            "recipes",
            "recipe",
            "recipetypes",
            "recipetype",
            "foodGroups",
            "foodGroup",
            "items",
            "results",
        ] {
            if let Some(list) = map.get(key) {
                items = as_items(list);
                if !items.is_empty() {
                    break;
                }
            }
        }
        // Single-object responses (e.g. one `recipetype`) render as one row.
        if items.is_empty() && (map.contains_key("id") || map.contains_key("title")) {
            items = vec![current];
        }
    }
    let mut human = String::new();
    let mut plain = String::new();
    // Flatten nested group lists (e.g. food-groups markets): a node without
    // its own id/name but with a nested list contributes its children.
    let mut worklist: Vec<&Value> = items;
    while let Some(item) = worklist.pop() {
        let nested: Vec<&Value> = ["foodGroups", "foodGroup", "items", "item"]
            .iter()
            .filter_map(|k| item.get(*k))
            .flat_map(as_items)
            .collect();
        let id = field(item, &["id", "recipe_id", "recipeid"]);
        let name = field(item, &["title", "name", "recipe_name", "code"]);
        if !nested.is_empty() && (id == "?" || name == "?") {
            worklist.extend(nested);
            continue;
        }
        let kcal = field(
            item,
            &[
                "energyPerPortion",
                "defaultEnergyPerPortion",
                "calories",
                "energy",
            ],
        );
        let portion = field(item, &["defaultPortionDescription", "serving_description"]);
        if kcal == "?" && portion == "?" {
            human.push_str(&format!("{id}  {name}\n"));
        } else {
            human.push_str(&format!("{id}  {name} — {kcal} energy / {portion}\n"));
        }
        plain.push_str(&format!("{id}\t{name}\n"));
    }
    if human.is_empty() {
        human = "no recipes returned (see --format json)".to_string();
    }
    (human, plain)
}
fn scalar(v: &Value) -> String {
    scalar_opt(v).unwrap_or_else(|| "?".to_string())
}

/// Tolerant exercise rendering: day entries and type lists both surface as
/// small id/name rows; kcal shown when present.
pub fn render_exercise(v: &Value) -> (String, String) {
    let mut current = v;
    loop {
        match current.as_object() {
            Some(map) if map.len() == 1 => match map.values().next() {
                Some(only) if only.is_object() => {
                    current = only;
                    continue;
                }
                _ => break,
            },
            _ => break,
        }
    }
    let mut items: Vec<&Value> = Vec::new();
    if let Some(map) = current.as_object() {
        for key in [
            "entries",
            "entry",
            "activities",
            "activity",
            "activityentry",
            "activitytype",
            "items",
            "types",
        ] {
            if let Some(list) = map.get(key) {
                items = as_items(list);
                if !items.is_empty() {
                    break;
                }
            }
        }
        if items.is_empty() && (map.contains_key("id") || map.contains_key("title")) {
            items = vec![current];
        }
    }
    let mut human = String::new();
    let mut plain = String::new();
    for item in items {
        let id = field(item, &["id", "typeID", "typeId", "activityId"]);
        let name = field(item, &["title", "name", "description", "activityName"]);
        human.push_str(&format!("{id}  {name} ({})\n", exercise_extra(item)));
        plain.push_str(&format!("{id}\t{name}\t{}\n", exercise_extra(item)));
    }
    if human.is_empty() {
        human = "no exercise data (see --format json)".to_string();
    }
    (human, plain)
}

/// Exercise row detail: minutes for day entries, kcal when really set, mets
/// for activity types. The day page uses -2147483648 for unknown energy —
/// never display that sentinel.
fn exercise_extra(item: &Value) -> String {
    let mins = field(item, &["minutes", "mins"]);
    if mins != "?" {
        return format!("{mins} min");
    }
    let kcal = field(item, &["kCal", "calories"]);
    if let Ok(n) = kcal.parse::<f64>()
        && n.is_finite()
        && n >= 0.0
    {
        return format!("{kcal} kcal");
    }
    let mets = field(item, &["mets"]);
    if mets != "?" {
        return format!("mets {mets}");
    }
    "?".to_string()
}

/// Tolerant water rendering: `{..., consume/total/goal}` scalars listed.
pub fn render_water(v: &Value, empty: &str) -> (String, String) {
    let mut current = v;
    loop {
        match current.as_object() {
            Some(map) if map.len() == 1 => match map.values().next() {
                Some(only) if only.is_object() => {
                    current = only;
                    continue;
                }
                _ => break,
            },
            _ => break,
        }
    }
    let mut human = String::new();
    let mut plain = String::new();
    if let Some(map) = current.as_object() {
        for (k, val) in map {
            if let Some(s) = scalar_opt(val) {
                if k == "$text" {
                    human.push_str(&format!("{s}\n"));
                } else {
                    human.push_str(&format!("{k}: {s}\n"));
                }
                plain.push_str(&format!("{k}\t{s}\n"));
            }
        }
    }
    if human.is_empty() {
        human = empty.to_string();
    }
    (human, plain)
}

/// Tolerant saved-meals rendering: meal lists and single meals with items.
pub fn render_meals(v: &Value) -> (String, String) {
    let mut current = v;
    loop {
        match current.as_object() {
            Some(map) if map.len() == 1 => match map.values().next() {
                Some(only) if only.is_object() => {
                    current = only;
                    continue;
                }
                _ => break,
            },
            _ => break,
        }
    }
    let mut items: Vec<&Value> = Vec::new();
    if let Some(map) = current.as_object() {
        for key in ["meals", "meal", "items", "item", "results"] {
            if let Some(list) = map.get(key) {
                items = as_items(list);
                if !items.is_empty() {
                    break;
                }
            }
        }
        if items.is_empty() && (map.contains_key("id") || map.contains_key("title")) {
            items = vec![current];
        }
    }
    let mut human = String::new();
    let mut plain = String::new();
    walk_meals(&items, String::new(), &mut human, &mut plain);
    if human.is_empty() {
        human = "no meals returned (see --format json)".to_string();
    }
    (human, plain)
}

/// Recursive walk for nested `item` groups (quick picks): leaves print with
/// their group path, groups print as headers when they carry no food id.
fn walk_meals(items: &[&Value], prefix: String, human: &mut String, plain: &mut String) {
    for item in items {
        let Some(map) = item.as_object() else {
            continue;
        };
        let name = field(item, &["title", "name", "entryname", "entryName"]);
        let id = field(item, &["id", "mealid", "mealId", "itemid", "itemId"]);
        let children: Vec<&Value> = ["item", "items", "meals", "meal"]
            .iter()
            .filter_map(|k| map.get(*k))
            .flat_map(as_items)
            .collect();
        if children.is_empty() {
            let label = join_path(&prefix, &name);
            // Quick-pick foods carry names but no ids: print the path bare
            // instead of a bogus `?` id.
            if id == "?" {
                human.push_str(&format!("{label}\n"));
                plain.push_str(&format!("{label}\n"));
            } else {
                human.push_str(&format!("{id}  {label}\n"));
                plain.push_str(&format!("{id}\t{label}\n"));
            }
        } else {
            let group = join_path(&prefix, &name);
            if !group.is_empty() && group != "?" {
                human.push_str(&format!("= {group} =\n"));
            }
            walk_meals(&children, group, human, plain);
        }
    }
}
fn join_path(prefix: &str, name: &str) -> String {
    match (prefix.is_empty() || prefix == "?", name) {
        (_, "?") => prefix.to_string(),
        (true, _) => name.to_string(),
        _ => format!("{prefix} / {name}"),
    }
}

/// Feed block lists: `{blockedUsers|blockingUsers: [...]}` (empty arrays
/// are the common case — no one blocked).
pub fn render_feed(v: &Value) -> (String, String) {
    let mut items: Vec<&Value> = Vec::new();
    if let Some(map) = v.as_object() {
        for key in ["blockedUsers", "blockingUsers", "users", "user"] {
            if let Some(list) = map.get(key) {
                items = as_items(list);
                if !items.is_empty() {
                    break;
                }
            }
        }
    }
    let mut human = String::new();
    let mut plain = String::new();
    for item in items {
        let id = field(item, &["id", "userId", "memberId"]);
        let name = field(item, &["userName", "username", "name", "title"]);
        plain.push_str(&format!("{id}\t{name}\n"));
    }
    if human.is_empty() {
        human = "no blocked users".to_string();
    }
    (human, plain)
}

fn scalar_opt(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn renders_observed_search_shape() {
        // Shape captured from the live API 2026-09-04 (numeric ids!).
        let v = json!({
            "currentpage": 0,
            "resultsPerPage": 20,
            "totalresults": 611,
            "recipes": [
                {"id": 39715, "title": "Oats", "energyPerPortion": 389.0,
                 "proteinPerPortion": 16.89, "defaultPortionDescription": "g",
                 "pathName": "usda/oats", "source": "SingleFood",
                 "status": "Published", "shortDescription": null},
                {"id": 4481, "title": "Oatmeal", "energyPerPortion": 172.515,
                 "defaultPortionDescription": "cup, cooked"},
            ],
        });
        let (human, plain) = render_foods_search(&v);
        assert!(human.contains("611 total"), "total: {human}");
        assert!(human.contains("39715  Oats"), "row: {human}");
        assert!(human.contains("389.0 energy"), "energy: {human}");
        assert!(plain.contains("39715\tOats\t389"), "plain: {plain}");
    }

    #[test]
    fn renders_account_shapes_tolerantly() {
        let v = json!({"member": {"userName": "vasyz-", "email": "a@b.c"}});
        let (human, plain) = render_account_show(&v);
        assert!(human.contains("userName: vasyz-"), "human: {human}");
        assert!(plain.contains("email\ta@b.c"), "plain: {plain}");
        let live = json!({
            "commencementWeightKg": 67.0, "gender": 1,
            "goalWeightKg": 80.0, "marketingUserType": "registered non-premium",
        });
        let (human, _) = render_account_show(&live);
        assert!(human.contains("goalWeightKg: 80"), "live: {human}");
        let wrapped = json!({"settings": {"activitysource": "FatSecret"}});
        let (human, _) = render_account_settings(&wrapped);
        assert!(
            human.contains("activitysource: FatSecret"),
            "wrapped: {human}"
        );
    }

    #[test]
    fn renders_empty_response() {
        let v = json!({"totalresults": 0});
        let (human, plain) = render_foods_search(&v);
        assert!(human.contains("0 shown"));
        assert!(plain.is_empty());
    }

    #[test]
    fn renders_recipe_lists_tolerantly() {
        let v = json!({"recipetypes": {"recipetype": [
            {"id": {"$text": "1"}, "title": {"$text": "Appetizers"}},
            {"id": {"$text": "2"}, "title": {"$text": "Soups"}},
        ]}});
        let (human, plain) = render_recipes(&v);
        assert!(human.contains("1  Appetizers"), "human: {human}");
        assert!(plain.contains("2\tSoups"), "plain: {plain}");
    }

    #[test]
    fn renders_journal_entry_shape() {
        // Shape captured live 2026-09-05: `recipejournalentry` rows with
        // `energyPerEntry` and numeric meal ids.
        let v = json!({
            "dateint": "20701",
            "guid": "83229321-cf22-4d5a-9dab-4680bd07fecb",
            "energyPerDay": "945.000",
            "recipejournalentry": [
                {"id": "24725902617", "meal": "1", "name": "Milk",
                 "energyPerEntry": "173.000"},
                {"id": "24726537311", "meal": "2", "name": "Drink",
                 "energyPerEntry": "197.000"},
            ],
        });
        let (human, plain) = render_diary_day(&v);
        assert!(human.contains("[breakfast]"), "meal: {human}");
        assert!(human.contains("24725902617  Milk"), "row: {human}");
        assert!(human.contains("173.000 energy"), "kcal: {human}");
        assert!(
            human.contains("[lunch] subtotal 197 energy"),
            "sub: {human}"
        );
        assert!(
            plain.contains("24726537311\tlunch\tDrink\t197.000"),
            "plain: {plain}"
        );
    }

    #[test]
    fn renders_diary_totals_and_macros() {
        let v = json!({
            "energyPerDay": "1253.000", "proteinPerDay": "95.660",
            "carbohydratePerDay": "129.710", "fatPerDay": "37.320",
            "recipejournalentry": [
                {"id": "1", "meal": "1", "name": "Oats",
                 "energyPerEntry": "389.000", "proteinPerEntry": "16.89",
                 "carbohydratePerEntry": "66.27", "fatPerEntry": "6.9",
                 "fiberPerEntry": "10.6", "servingDescription": "100 g"},
            ],
        });
        let (human, _) = render_diary_day(&v);
        assert!(
            human.contains("day total 1253.000 energy"),
            "total: {human}"
        );
        assert!(human.contains("P 95.660"), "day protein: {human}");
        assert!(
            human.contains("P 16.89 · C 66.27 · F 6.9 · fiber 10.6"),
            "macros: {human}"
        );
        assert!(human.contains("100 g ·"), "serving: {human}");
    }

    #[test]
    fn renders_food_macros_and_scaled_portions() {
        let v = json!({
            "title": "Oats", "energyPerPortion": "389.000",
            "proteinPerPortion": "16.89", "carbohydratePerPortion": "66.27",
            "fatPerPortion": "6.9", "fiberPerPortion": "10.6",
            "defaultPortionID": "62446",
            "recipeportion": [
                {"id": "62446", "description": "g", "gramWeight": "100.000"},
                {"id": "40321", "description": "cup", "gramWeight": "156.000"},
            ],
        });
        let (human, plain) = render_food_get(&v);
        assert!(
            human.contains("389.000 energy / portion"),
            "header: {human}"
        );
        assert!(human.contains("P 16.89 · C 66.27"), "macros: {human}");
        assert!(
            human.contains("40321  cup (156 g): 607 energy"),
            "scaled: {human}"
        );
        assert!(plain.contains("40321\tcup\t607"), "plain: {plain}");
    }

    #[test]
    fn joins_multipart_titles_and_hides_sentinel_kcal() {
        let v = json!({"recipe": [
            {"id": "48901", "title": ["Morning Minis - Cinnamon Sugar", "Oats"],
             "energyPerPortion": "130.000",
             "defaultPortionDescription": "1 pouch"},
        ]});
        let (human, _) = render_recipes(&v);
        assert!(
            human.contains("48901  Morning Minis - Cinnamon Sugar Oats"),
            "title: {human}"
        );
        assert!(human.contains("130.000 energy / 1 pouch"), "kcal: {human}");
        let v = json!({"activityentry": [
            {"typeID": "2", "name": "Resting", "minutes": "960",
             "kCal": "-2147483648"},
        ]});
        let (human, _) = render_exercise(&v);
        assert!(human.contains("2  Resting (960 min)"), "mins: {human}");
        assert!(!human.contains("-2147483648"), "sentinel leaked: {human}");
        let v = json!({"activitytype": [
            {"id": "77", "name": "Abdominal (Sit Ups)", "mets": "8.0"},
        ]});
        let (human, _) = render_exercise(&v);
        assert!(human.contains("(mets 8.0)"), "mets: {human}");
    }
}
