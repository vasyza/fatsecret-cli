//! App-scheme REST client: JSON POSTs with the `Authorization: FatSecret`
//! header triple. No OAuth.

use reqwest::{Client, RequestBuilder};
use serde_json::Value;

use crate::auth::Triple;
use crate::config::AppConfig;
use crate::error::{AppError, Result};

#[derive(Debug, Clone)]
pub struct FsClient {
    http: Client,
    pub app: AppConfig,
    pub creds: Option<Triple>,
}

impl FsClient {
    pub fn new(app: AppConfig, creds: Option<Triple>) -> Result<Self> {
        // NOTE: legacy responses are sensitive to the TLS/ALPN fingerprint;
        // platform TLS with default negotiation (see WAL).
        let http = Client::builder()
            .user_agent(format!("fatsecret-cli/{}", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self { http, app, creds })
    }

    pub fn http(&self) -> &Client {
        &self.http
    }
    fn authed(&self, req: RequestBuilder) -> Result<RequestBuilder> {
        let Some(t) = &self.creds else {
            return Err(AppError::NotLoggedIn);
        };
        let mut req = req
            .header("Authorization", "FatSecret")
            .header("c_id", t.server_id.to_string())
            .header("c_s", &t.secret_key)
            .header("c_d", &t.device_key)
            .header("fs_device", "android")
            .header("fs_device_type", "android")
            .header("fs_app_version", &self.app.app_version)
            .header("app_version", &self.app.app_version)
            .header("device", "6")
            .header("unit", "kj")
            .header("Content-Type", "application/json");
        if let Some(id) = &self.app.device_id {
            req = req.header("c_desc", id);
        }
        Ok(req)
    }
    async fn post_json(&self, url: &str, body: &Value) -> Result<Value> {
        let req = self.authed(self.http.post(url))?.json(body);
        self.finish(req).await
    }

    /// Pre-auth POST: device headers, no triple (login/register/password flows).
    async fn post_preauth(
        &self,
        url: &str,
        body: &Value,
        device_id: Option<&str>,
    ) -> Result<Value> {
        let mut req = self
            .http
            .post(url)
            .header("Content-Type", "application/json")
            .header("fs_device", "android")
            .header("fs_device_type", "android")
            .header("fs_app_version", &self.app.app_version)
            .header("app_version", &self.app.app_version)
            .header("device", "6")
            .header("unit", "kj");
        if let Some(id) = device_id {
            req = req.header("c_d", id).header("c_desc", id);
        }
        self.finish(req.json(body)).await
    }

    /// Authed GET with the app query-string convention (`?k=v&...`,
    /// RFC-3986-encoded). Used by legacy `.aspx` pages.
    async fn get_query(&self, url: &str, query: &[(&str, &str)]) -> Result<Value> {
        let full = if query.is_empty() {
            url.to_string()
        } else {
            format!("{url}?{}", encode_query(query))
        };
        let req = self.authed(self.http.get(full))?;
        self.finish(req).await
    }

    async fn finish(&self, req: RequestBuilder) -> Result<Value> {
        let status = req.send().await?;
        let code = status.status();
        let text = status.text().await?;
        let value = parse_body(&text)?;
        // Legacy pages answer 200 with a bare failure string instead of an
        // error object; surface it instead of reporting success.
        if let Value::String(s) = &value
            && s.contains("Unable to save")
        {
            return Err(AppError::Api {
                code: code.to_string(),
                message: s.clone(),
            });
        }
        if !code.is_success() {
            let (mut ecode, message) = api_error(&value, &text);
            if ecode == "?" {
                ecode = code.to_string();
            }
            return Err(AppError::Api {
                code: ecode,
                message,
            });
        }
        Ok(value)
    }
    /// Modern food search: `POST {food_search_url}` with
    /// `{SearchExpression, PageNumber, PageSize}`.
    pub async fn foods_search(&self, query: &str, page: i64, size: i64) -> Result<Value> {
        let body = serde_json::json!({
            "SearchExpression": query,
            "PageNumber": page,
            "PageSize": size,
        });
        let url = self.app.food_search_url.clone();
        self.post_json(&url, &body).await
    }

    /// Account registration: `POST {register_url}` with the full
    /// `RegisterAccountDTO` key set (missing optionals are omitted).
    pub async fn register(&self, body: &Value, device_id: Option<&str>) -> Result<Value> {
        let url = self.app.register_url.clone();
        self.post_preauth(&url, body, device_id).await
    }

    /// Password-reset email: `POST {forgot_url}` with `{userIdentifier}`.
    pub async fn forgot_password(&self, email: &str, device_id: Option<&str>) -> Result<Value> {
        let body = serde_json::json!({ "userIdentifier": email });
        let url = self.app.forgot_url.clone();
        self.post_preauth(&url, &body, device_id).await
    }

    /// Password reset: `POST {reset_url}` with `{code, password}`.
    pub async fn reset_password(
        &self,
        code: &str,
        password: &str,
        device_id: Option<&str>,
    ) -> Result<Value> {
        let body = serde_json::json!({ "code": code, "password": password });
        let url = self.app.reset_url.clone();
        self.post_preauth(&url, &body, device_id).await
    }

    /// Account settings page: authed GET `{server_base}AccountSettingsAndroidPage.aspx?fl=3`.
    pub async fn account_settings(&self) -> Result<Value> {
        let url = format!("{}AccountSettingsAndroidPage.aspx", self.app.server_base);
        self.get_query(&url, &[("fl", "3")]).await
    }
    /// Change username: authed `POST {change_username_url}` with `{userName}`.
    pub async fn change_username(&self, name: &str) -> Result<Value> {
        let body = serde_json::json!({ "userName": name });
        let url = self.app.change_username_url.clone();
        self.post_json(&url, &body).await
    }

    /// Account details: authed GET `{user_details_url}`, no params.
    pub async fn user_details(&self) -> Result<Value> {
        let url = self.app.user_details_url.clone();
        self.get_query(&url, &[]).await
    }

    /// Food details: authed GET `RecipeAndroidPage.aspx?rid={id}&images=true`.
    pub async fn food_get(&self, id: i64) -> Result<Value> {
        let url = format!("{}RecipeAndroidPage.aspx", self.app.server_base);
        self.get_query(&url, &[("rid", &id.to_string()), ("images", "true")])
            .await
    }

    /// Diary day: authed GET `RecipeJournalDayAndroidPage.aspx?fl=7`.
    pub async fn diary_day(&self) -> Result<Value> {
        let url = format!("{}RecipeJournalDayAndroidPage.aspx", self.app.server_base);
        self.get_query(&url, &[("fl", "7")]).await
    }

    /// Dietary-preference vote: `POST {food_vote_url}` with
    /// `{recipeid, votes: [{typeId, value}]}`.
    pub async fn vote_preference(&self, recipe_id: i64, votes: &[(i64, String)]) -> Result<Value> {
        let body = vote_body(recipe_id, votes);
        let url = self.app.food_vote_url.clone();
        self.post_json(&url, &body).await
    }

    /// Barcode scan: `POST {scan_url}` with `{barcode, deviceCanPrompt}`.
    pub async fn barcode_scan(&self, gtin: &str) -> Result<Value> {
        let body = serde_json::json!({ "barcode": gtin, "deviceCanPrompt": false });
        let url = self.app.scan_url.clone();
        self.post_json(&url, &body).await
    }

    /// Journal bulk update: `POST {journal_url}` with
    /// `{recordedDate, recipes: [...], deletes: [...]}`.
    pub async fn journal_update(
        &self,
        recorded_date: i64,
        recipes: &[Value],
        deletes: &[Value],
    ) -> Result<Value> {
        let body = serde_json::json!({
            "recordedDate": recorded_date,
            "recipes": recipes,
            "deletes": deletes,
        });
        let url = self.app.journal_url.clone();
        self.post_json(&url, &body).await
    }

    /// Dietary-preference types: authed GET `{food_types_url}`, no params.
    pub async fn dietary_types(&self) -> Result<Value> {
        let url = self.app.food_types_url.clone();
        self.get_query(&url, &[]).await
    }

    /// Most popular serving sizes: `POST {food_popular_url}` with `{foodIds}`.
    pub async fn foods_popular(&self, ids: &[i64]) -> Result<Value> {
        let body = serde_json::json!({ "foodIds": ids });
        let url = self.app.food_popular_url.clone();
        self.post_json(&url, &body).await
    }

    /// Recipe search: authed GET `RecipeSearch.aspx` with `{fl:2, q, pg}`.
    pub async fn recipes_search(&self, query: &str, page: i64) -> Result<Value> {
        let page = page.to_string();
        let url = format!("{}RecipeSearch.aspx", self.app.server_base);
        self.get_query(&url, &[("fl", "2"), ("q", query), ("pg", &page)])
            .await
    }

    /// Recipe details: same page as food details (`RecipeAndroidPage.aspx`).
    pub async fn recipe_get(&self, id: i64) -> Result<Value> {
        self.food_get(id).await
    }

    /// Recipe categories: authed GET `RecipeTypeAndroidPage.aspx`, no params.
    pub async fn recipe_categories(&self) -> Result<Value> {
        let url = format!("{}RecipeTypeAndroidPage.aspx", self.app.server_base);
        self.get_query(&url, &[]).await
    }

    /// Cookbook search: authed POST `CookbookSearchJsonAndroidPage.aspx` with
    /// `{searchExpression, pageNumber, pageSize}`.
    pub async fn cookbook_search(&self, query: &str, page: i64, size: i64) -> Result<Value> {
        let body = serde_json::json!({
            "searchExpression": query,
            "pageNumber": page,
            "pageSize": size,
        });
        let url = format!("{}CookbookSearchJsonAndroidPage.aspx", self.app.server_base);
        self.post_json(&url, &body).await
    }

    /// Cookbook recipe count: authed GET `{recipe_count_url}?marketLocale=`.
    pub async fn cookbook_count(&self, market: &str) -> Result<Value> {
        let url = self.app.recipe_count_url.clone();
        self.get_query(&url, &[("marketLocale", market)]).await
    }

    /// Thin Wave-8 reads. Modern URLs are fixed (verified live, no per-user
    /// variance to configure); legacy pages build from `server_base`.
    /// User attributes: authed GET `get-user-attributes`, no params.
    pub async fn user_attributes(&self) -> Result<Value> {
        self.get_query(
            "https://app.ftscrt.com/api/user-attributes/v1/get-user-attributes",
            &[],
        )
        .await
    }

    /// Locale settings: authed GET `GetBestSettingsAndroidPage.aspx`.
    pub async fn settings_locale(&self) -> Result<Value> {
        let url = self.legacy("GetBestSettingsAndroidPage.aspx");
        self.get_query(&url, &[]).await
    }

    /// Notifications: authed GET `NotificationsAndroidPage.aspx`.
    pub async fn notifications(&self) -> Result<Value> {
        let url = self.legacy("NotificationsAndroidPage.aspx");
        self.get_query(&url, &[]).await
    }

    /// Onboarding config: authed GET `OnboardingConfigurationAndroidPage.aspx`.
    pub async fn onboarding_config(&self) -> Result<Value> {
        let url = self.legacy("OnboardingConfigurationAndroidPage.aspx");
        self.get_query(&url, &[]).await
    }

    /// Feed blocks: authed POST `get-user-blocks` with `{}`.
    pub async fn feed_blocks(&self) -> Result<Value> {
        self.post_json(
            "https://app.ftscrt.com/api/feed/v1/get-user-blocks",
            &serde_json::json!({}),
        )
        .await
    }

    /// Feed blocking users: authed POST `get-blocking-users` with `{}`.
    pub async fn feed_blocking(&self) -> Result<Value> {
        self.post_json(
            "https://app.ftscrt.com/api/feed/v1/get-blocking-users",
            &serde_json::json!({}),
        )
        .await
    }

    /// Learning progress: authed POST `user/get` with `{}`.
    pub async fn learning_progress(&self) -> Result<Value> {
        self.post_json(
            "https://app.ftscrt.com/api/user-learning/v1/user/get",
            &serde_json::json!({}),
        )
        .await
    }

    /// Learning content: authed POST `content/get-for-language` with `{}`.
    pub async fn learning_content(&self) -> Result<Value> {
        self.post_json(
            "https://app.ftscrt.com/api/user-learning/v1/content/get-for-language",
            &serde_json::json!({}),
        )
        .await
    }

    /// Static food-groups data: authed GET `{server_base}FoodGroupsData.json`.
    pub async fn food_groups(&self) -> Result<Value> {
        let url = self.legacy("FoodGroupsData.json");
        self.get_query(&url, &[]).await
    }

    fn legacy(&self, page: &str) -> String {
        format!("{}{page}", self.app.server_base)
    }
    /// Saved meals list: `SavedMealsAndroidPage.aspx?meal={ordinal}`.
    pub async fn meals_list(&self, meal_ordinal: i64) -> Result<Value> {
        let url = self.legacy("SavedMealsAndroidPage.aspx");
        self.get_query(&url, &[("meal", &meal_ordinal.to_string())])
            .await
    }

    /// Saved meal details: `SavedMealAndroidPage.aspx?mealid=&fl=4`.
    pub async fn meal_show(&self, meal_id: i64) -> Result<Value> {
        let url = self.legacy("SavedMealAndroidPage.aspx");
        self.get_query(&url, &[("mealid", &meal_id.to_string()), ("fl", "4")])
            .await
    }

    /// Saved meal create/edit: `SavedMealActionAndroidPage.aspx` with
    /// `{action:save, mealid, title, description, mealtypes}` (mealid 0 to create).
    pub async fn meal_save(
        &self,
        meal_id: i64,
        title: &str,
        description: &str,
        meal_types: &str,
    ) -> Result<Value> {
        let url = self.legacy("SavedMealActionAndroidPage.aspx");
        self.get_query(
            &url,
            &[
                ("action", "save"),
                ("mealid", &meal_id.to_string()),
                ("title", title),
                ("description", description),
                ("mealtypes", meal_types),
            ],
        )
        .await
    }

    /// Saved meal delete: `{action:delete, mealid}`.
    pub async fn meal_delete(&self, meal_id: i64) -> Result<Value> {
        let url = self.legacy("SavedMealActionAndroidPage.aspx");
        self.get_query(
            &url,
            &[("action", "delete"), ("mealid", &meal_id.to_string())],
        )
        .await
    }

    /// Log a saved meal into the diary: `{action:add, mealid, meal}`.
    pub async fn meal_log(&self, meal_id: i64, meal: i64) -> Result<Value> {
        let url = self.legacy("SavedMealActionAndroidPage.aspx");
        self.get_query(
            &url,
            &[
                ("action", "add"),
                ("mealid", &meal_id.to_string()),
                ("meal", &meal.to_string()),
            ],
        )
        .await
    }

    /// Meal item save: `SavedMealItemActionAndroidPage.aspx` with
    /// `{action:save, mealid, itemid, rid, entryname, portionid, portionamount}`.
    pub async fn meal_item_save(
        &self,
        meal_id: i64,
        item_id: i64,
        food_id: i64,
        name: &str,
        portion_id: i64,
        units: f64,
    ) -> Result<Value> {
        let url = self.legacy("SavedMealItemActionAndroidPage.aspx");
        self.get_query(
            &url,
            &[
                ("action", "save"),
                ("mealid", &meal_id.to_string()),
                ("itemid", &item_id.to_string()),
                ("rid", &food_id.to_string()),
                ("entryname", name),
                ("portionid", &portion_id.to_string()),
                ("portionamount", &units.to_string()),
            ],
        )
        .await
    }

    /// Meal item delete: `{action:delete, itemid}`.
    pub async fn meal_item_delete(&self, item_id: i64) -> Result<Value> {
        let url = self.legacy("SavedMealItemActionAndroidPage.aspx");
        self.get_query(
            &url,
            &[("action", "delete"), ("itemid", &item_id.to_string())],
        )
        .await
    }

    /// Quick picks: `QuickPicksAndroidPage.aspx`, no params.
    pub async fn quick_picks(&self) -> Result<Value> {
        let url = self.legacy("QuickPicksAndroidPage.aspx");
        self.get_query(&url, &[]).await
    }
    /// Weight save: authed GET `AccountActionAndroidPage.aspx` with
    /// `{action:save, currentWeightKg, goalWeightKg, journal, reset}`.
    pub async fn weight_log(&self, current_kg: f64, goal_kg: f64) -> Result<Value> {
        let url = format!("{}AccountActionAndroidPage.aspx", self.app.server_base);
        self.get_query(
            &url,
            &[
                ("action", "save"),
                ("currentWeightKg", &current_kg.to_string()),
                ("goalWeightKg", &goal_kg.to_string()),
                ("journal", ""),
                ("reset", "false"),
            ],
        )
        .await
    }

    /// Exercise day: authed GET `ActivityDayAndroidPage.aspx?fl=3`.
    pub async fn exercise_day(&self) -> Result<Value> {
        let url = format!("{}ActivityDayAndroidPage.aspx", self.app.server_base);
        self.get_query(&url, &[("fl", "3")]).await
    }

    /// Activity types: authed GET `ActivitiesAndroidPage.aspx`, no params
    /// (`fl=3` narrows to a placeholder on the wire; trust observed behavior).
    pub async fn exercise_types(&self) -> Result<Value> {
        let url = format!("{}ActivitiesAndroidPage.aspx", self.app.server_base);
        self.get_query(&url, &[]).await
    }

    /// Exercise log: authed GET `ActivityEntryActionAndroidPage.aspx` with
    /// `{action:increment, typeID, mins, [kCal], [description], fl:2}`.
    pub async fn exercise_log(
        &self,
        type_id: i64,
        mins: i64,
        kcal: Option<f64>,
        description: Option<&str>,
    ) -> Result<Value> {
        let type_id = type_id.to_string();
        let mins = mins.to_string();
        let mut query = vec![
            ("action", "increment"),
            ("typeID", type_id.as_str()),
            ("mins", mins.as_str()),
            ("fl", "2"),
        ];
        let kcal_str;
        if let Some(k) = kcal {
            kcal_str = k.to_string();
            query.push(("kCal", kcal_str.as_str()));
        }
        if let Some(d) = description {
            query.push(("description", d));
        }
        let url = format!(
            "{}ActivityEntryActionAndroidPage.aspx",
            self.app.server_base
        );
        self.get_query(&url, &query).await
    }

    /// Water entry: authed GET `WaterTrackingActionAndroidPage.aspx` with
    /// `{action:get, dateInt}`; `date_int` is days since epoch.
    pub async fn water_get(&self, date_int: i64) -> Result<Value> {
        let url = format!(
            "{}WaterTrackingActionAndroidPage.aspx",
            self.app.server_base
        );
        self.get_query(
            &url,
            &[("action", "get"), ("dateInt", &date_int.to_string())],
        )
        .await
    }

    /// Water log: `{action:savewater, consume: ml, type: 1 (delta),
    /// goal: daily ml, dateInt, fl: 2}`.
    pub async fn water_log(&self, ml: i64, goal_ml: i64, date_int: i64) -> Result<Value> {
        let url = format!(
            "{}WaterTrackingActionAndroidPage.aspx",
            self.app.server_base
        );
        self.get_query(
            &url,
            &[
                ("action", "savewater"),
                ("consume", &ml.to_string()),
                ("type", "1"),
                ("goal", &goal_ml.to_string()),
                ("dateInt", &date_int.to_string()),
                ("fl", "2"),
            ],
        )
        .await
    }
}

/// Diary entry object with the `DTORequestBulkUpdateRecipeJournalEntry` keys.
/// `meal` is the server meal id (1=breakfast, 2=lunch, 3=dinner, 4=snack,
/// 5=pre-breakfast, 6=second-breakfast); `id` is 0 for new entries.
pub fn journal_entry(
    id: i64,
    recipe_id: i64,
    name: &str,
    portion_id: i64,
    portion_amount: f64,
    meal: i64,
    serving_desc: &str,
) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("id".to_string(), id.into());
    // `reference` is REQUIRED (server 400s empty without it); the app sends
    // the string form of the local entry id (`"0"` for new entries).
    obj.insert("reference".to_string(), id.to_string().into());
    obj.insert("recipeid".to_string(), recipe_id.into());
    obj.insert("name".to_string(), name.into());
    obj.insert("recipeportionid".to_string(), portion_id.into());
    obj.insert(
        "portionamount".to_string(),
        serde_json::Number::from_f64(portion_amount).map_or(Value::Null, Value::Number),
    );
    obj.insert("meal".to_string(), meal.into());
    // The app only sends `servingDescription` for AI-assistant entries.
    if !serving_desc.is_empty() {
        obj.insert("servingDescription".to_string(), serving_desc.into());
    }
    Value::Object(obj)
}
/// Vote body with the exact `FoodDietaryPreferenceVoteRequestDTO` key set.
pub fn vote_body(recipe_id: i64, votes: &[(i64, String)]) -> Value {
    let votes: Vec<Value> = votes
        .iter()
        .map(|(t, v)| serde_json::json!({ "typeId": t, "value": v }))
        .collect();
    serde_json::json!({ "recipeid": recipe_id, "votes": votes })
}

/// Response bodies are JSON on the modern generation and XML on legacy
/// `.aspx` pages (both parsed by the app). JSON first, XML fallback.
/// Empty 2xx bodies (fire-and-forget endpoints) become `Value::Null`.
fn parse_body(text: &str) -> Result<Value> {
    let trimmed = text.trim_start();
    if trimmed.is_empty() {
        return Ok(Value::Null);
    }
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return serde_json::from_str(trimmed).map_err(AppError::Json);
    }
    // quick-xml's own `Value` support collapses repeated siblings to one
    // item (silent list-data loss); strip the prolog it also chokes on, then
    // build with the repeat-preserving reader below.
    let body = trim_xml_prolog(trimmed);
    xml_to_value(body).map_err(|e| AppError::Msg(format!("unparseable response body: {e}")))
}

fn trim_xml_prolog(s: &str) -> &str {
    let t = s.trim_start();
    if let Some(rest) = t.strip_prefix("<?") {
        if let Some(end) = rest.find("?>") {
            return rest[end + 2..].trim_start();
        }
    }
    t
}

/// Minimal XML→JSON mapping compatible with the renderers: the root tag is
/// dropped (its children form the top object), text becomes `{"$text": s}`,
/// and repeated sibling tags collect into arrays. Attributes are ignored
/// (the observed API surface carries data in elements only).
fn xml_to_value(text: &str) -> std::result::Result<Value, String> {
    use quick_xml::{Reader, events::Event};
    use serde_json::Map;

    fn insert(map: &mut Map<String, Value>, key: String, val: Value) {
        match map.remove(&key) {
            None => {
                map.insert(key, val);
            }
            Some(Value::Array(mut arr)) => {
                arr.push(val);
                map.insert(key, Value::Array(arr));
            }
            Some(prev) => {
                map.insert(key, Value::Array(vec![prev, val]));
            }
        }
    }

    #[derive(Debug)]
    enum Frame {
        Map {
            tag: String,
            map: Map<String, Value>,
        },
    }

    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(true);
    let mut stack: Vec<Frame> = Vec::new();
    let mut root: Option<Value> = None;
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Err(e) => return Err(e.to_string()),
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                stack.push(Frame::Map {
                    tag,
                    map: Map::new(),
                });
            }
            Ok(Event::Empty(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                match stack.last_mut() {
                    Some(Frame::Map { map, .. }) => insert(map, tag, Value::Null),
                    None => root = Some(Value::Null),
                }
            }
            Ok(Event::Text(e)) => {
                let text = e.xml_content().map_err(|e| e.to_string())?.into_owned();
                if text.is_empty() {
                    continue;
                }
                match stack.last_mut() {
                    Some(Frame::Map { map, .. }) => {
                        insert(map, "$text".to_string(), Value::String(text))
                    }
                    None => root = Some(Value::String(text)),
                }
            }
            Ok(Event::End(_)) => {
                let Some(Frame::Map { tag, map }) = stack.pop() else {
                    continue;
                };
                let inner = match map.len() {
                    1 if map.contains_key("$text") => map
                        .into_iter()
                        .next()
                        .map(|(_, v)| v)
                        .unwrap_or(Value::Null),
                    _ => Value::Object(map),
                };
                if stack.is_empty() {
                    root = Some(inner);
                } else if let Some(Frame::Map { map, .. }) = stack.last_mut() {
                    insert(map, tag, inner);
                }
            }
            _ => {}
        }
        buf.clear();
    }
    Ok(root.unwrap_or(Value::Null))
}
/// Registration body with the exact `RegisterAccountDTO` key set.
/// `None` optionals are omitted from the JSON object.
#[allow(clippy::too_many_arguments)]
pub fn register_body(
    email: &str,
    member_name: &str,
    password: &str,
    gender: &str,
    birth_date: &str,
    weight_measure: i64,
    current_weight_kg: Option<f64>,
    goal_weight_kg: Option<f64>,
    height_measure: i64,
    height_cm: Option<f64>,
    current_date: &str,
    country_code: &str,
    age_in_years: Option<i64>,
    goal: Option<&str>,
    activity_level: Option<&str>,
    device_identifier: &str,
    first_name: Option<&str>,
) -> Value {
    let mut obj = serde_json::Map::new();
    let mut put = |k: &str, v: Value| {
        obj.insert(k.to_string(), v);
    };
    put("email", Value::String(email.to_string()));
    put("memberName", Value::String(member_name.to_string()));
    put("password", Value::String(password.to_string()));
    put("gender", Value::String(gender.to_string()));
    put("birthDate", Value::String(birth_date.to_string()));
    put("weightMeasure", Value::Number(weight_measure.into()));
    put("heightMeasure", Value::Number(height_measure.into()));
    put("currentDate", Value::String(current_date.to_string()));
    put("countryCode", Value::String(country_code.to_string()));
    put(
        "deviceIdentifier",
        Value::String(device_identifier.to_string()),
    );
    if let Some(v) = current_weight_kg {
        put(
            "currentWeightKg",
            serde_json::Number::from_f64(v)
                .map(Value::Number)
                .unwrap_or(Value::Null),
        );
    }
    if let Some(v) = goal_weight_kg {
        put(
            "goalWeightKg",
            serde_json::Number::from_f64(v)
                .map(Value::Number)
                .unwrap_or(Value::Null),
        );
    }
    if let Some(v) = height_cm {
        put(
            "heightCm",
            serde_json::Number::from_f64(v)
                .map(Value::Number)
                .unwrap_or(Value::Null),
        );
    }
    if let Some(v) = age_in_years {
        put("ageInYears", Value::Number(v.into()));
    }
    if let Some(v) = goal {
        put("goal", Value::String(v.to_string()));
    }
    if let Some(v) = activity_level {
        put("activityLevel", Value::String(v.to_string()));
    }
    if let Some(v) = first_name {
        put("firstName", Value::String(v.to_string()));
    }
    Value::Object(obj)
}

fn api_error(value: &Value, raw: &str) -> (String, String) {
    let err = value.get("error");
    let code = err
        .and_then(|e| e.get("typeId").or_else(|| e.get("code")))
        .map(|c| c.to_string())
        .unwrap_or_else(|| "?".to_string());
    let message = err
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| truncate_raw(raw));
    (code, message)
}

/// Truncated raw body for unstructured error responses (bounded so error
/// output stays one screen; whitespace collapsed).
fn truncate_raw(raw: &str) -> String {
    const LIMIT: usize = 300;
    let flat: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.len() <= LIMIT {
        return flat;
    }
    format!(
        "{}…",
        flat.char_indices()
            .take_while(|(i, _)| *i < LIMIT)
            .map(|(_, c)| c)
            .collect::<String>()
    )
}

/// RFC-3986 query encoder matching the app's `e0` builder: `k=v&k=v` with
/// unreserved bytes raw, everything else `%HH` uppercase.
fn encode_query(query: &[(&str, &str)]) -> String {
    query
        .iter()
        .map(|(k, v)| format!("{}={}", encode_part(k), encode_part(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn encode_part(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push(
                    char::from_digit(u32::from(b >> 4), 16)
                        .unwrap_or('0')
                        .to_ascii_uppercase(),
                );
                out.push(
                    char::from_digit(u32::from(b & 0x0F), 16)
                        .unwrap_or('0')
                        .to_ascii_uppercase(),
                );
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_query_pairs() {
        assert_eq!(encode_query(&[]), "");
        assert_eq!(
            encode_query(&[("q", "oats"), ("pg", "0"), ("fl", "2")]),
            "q=oats&pg=0&fl=2"
        );
        assert_eq!(encode_query(&[("q", "a b+c")]), "q=a%20b%2Bc");
    }

    #[test]
    fn builds_register_body_keys() {
        let v = register_body(
            "a@b.c",
            "user",
            "pw",
            "Male",
            "1990-01-01",
            0,
            Some(80.0),
            Some(75.0),
            0,
            Some(180.0),
            "2026-09-05",
            "US",
            Some(36),
            Some("lose"),
            Some("active"),
            "android",
            Some("Al"),
        );
        for key in [
            "email",
            "memberName",
            "password",
            "gender",
            "birthDate",
            "weightMeasure",
            "currentWeightKg",
            "goalWeightKg",
            "heightMeasure",
            "heightCm",
            "currentDate",
            "countryCode",
            "ageInYears",
            "goal",
            "activityLevel",
            "deviceIdentifier",
            "firstName",
        ] {
            assert!(v.get(key).is_some(), "missing key {key}");
        }
        assert_eq!(v["memberName"], serde_json::json!("user"));
        let minimal = register_body(
            "a@b.c",
            "user",
            "pw",
            "Male",
            "1990-01-01",
            0,
            None,
            None,
            0,
            None,
            "2026-09-05",
            "US",
            None,
            None,
            None,
            "android",
            None,
        );
        assert!(minimal.get("firstName").is_none());
        assert!(minimal.get("email").is_some());
    }

    #[test]
    fn builds_vote_body_keys() {
        let v = vote_body(39715, &[(3, "1".to_string()), (5, "0".to_string())]);
        assert_eq!(v["recipeid"], serde_json::json!(39715));
        assert_eq!(v["votes"][0]["typeId"], serde_json::json!(3));
        assert_eq!(v["votes"][1]["value"], serde_json::json!("0"));
    }

    #[test]
    fn builds_journal_entry_keys() {
        let v = journal_entry(0, 39715, "Oats", 62446, 1.0, 1, "g");
        for key in [
            "id",
            "reference",
            "recipeid",
            "name",
            "recipeportionid",
            "portionamount",
            "meal",
            "servingDescription",
        ] {
            assert!(v.get(key).is_some(), "missing key {key}");
        }
        assert_eq!(v["meal"], serde_json::json!(1));
        // `reference` is required by the server: string form of the entry id.
        assert_eq!(v["reference"], serde_json::json!("0"));
        // Empty serving descriptions are omitted (app behavior).
        assert!(
            journal_entry(0, 39715, "Oats", 62446, 1.0, 1, "")
                .get("servingDescription")
                .is_none()
        );
    }

    #[test]
    fn preserves_repeated_siblings() {
        let v = xml_to_value("<a><b><id>1</id><t>x</t></b><b><id>2</id><t>y</t></b></a>").unwrap();
        let items = v["b"].as_array().expect("repeats must collect");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["id"], serde_json::json!("1"));
        assert_eq!(items[1]["t"], serde_json::json!("y"));
    }

    #[test]
    fn parses_legacy_page_shape() {
        let xml = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n\r\n<activitytypecollection>\r\n<activitytype>\r\n<id>77</id>\r\n<name>Abs</name>\r\n</activitytype>\r\n<activitytype>\r\n<id>78</id>\r\n<name>Other</name>\r\n</activitytype>\r\n</activitytypecollection>";
        let v = parse_body(xml).unwrap();
        let items = v["activitytype"].as_array().expect("list preserved");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["name"], serde_json::json!("Abs"));
    }
}
