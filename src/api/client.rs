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

    /// Clone with overridden market/language locales (per-search override).
    pub fn with_locales(&self, market: Option<&str>, language: Option<&str>) -> Self {
        let mut app = self.app.clone();
        if let Some(m) = market {
            app.market_locale = m.to_string();
        }
        if let Some(l) = language {
            app.language_locale = l.to_string();
        }
        Self {
            http: self.http.clone(),
            app,
            creds: self.creds.clone(),
        }
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
            .header("unit", "kj");
        // No Content-Type here: reqwest `header()` appends while `.json()` /
        // `.form()` only fill an absent value, so each body setter must own it.
        if let Some(id) = &self.app.device_id {
            req = req.header("c_desc", id);
        }
        Ok(req)
    }
    async fn post_json(&self, url: &str, body: &Value) -> Result<Value> {
        let mut req = self.authed(self.http.post(url))?;
        // Market + language scope modern indexes (e.g. food search);
        // the app sends both on every modern call.
        if !self.app.market_locale.is_empty() {
            req = req.header("fs_market_locale", &self.app.market_locale);
        }
        if !self.app.language_locale.is_empty() {
            req = req.header("fs_language_locale", &self.app.language_locale);
        }
        self.finish(req.json(body)).await
    }

    /// Legacy action-page write: the app POSTs `application/x-www-form-urlencoded`
    /// bodies to `.aspx` pages (params in the body, not the query string).
    /// Several action pages reject the same params over GET with `Unable to save`.
    /// The body carries the app-equivalent prefix (`c_id/c_fl/c_s/c_d`, `dt`,
    /// `app_version`, `unit`) ahead of the caller params.
    async fn post_form(&self, url: &str, params: &[(&str, &str)]) -> Result<Value> {
        self.post_form_dated(url, crate::auth::device::today_days(), params)
            .await
    }

    /// `post_form` with an explicit day number (diary history reads).
    async fn post_form_dated(
        &self,
        url: &str,
        days: i64,
        params: &[(&str, &str)],
    ) -> Result<Value> {
        let Some(t) = &self.creds else {
            return Err(AppError::NotLoggedIn);
        };
        let mut body: Vec<(String, String)> = vec![
            ("c_id".to_string(), t.server_id.to_string()),
            ("c_fl".to_string(), "1".to_string()),
            ("c_s".to_string(), t.secret_key.clone()),
            ("c_d".to_string(), t.device_key.clone()),
            ("dt".to_string(), days.to_string()),
            ("app_version".to_string(), self.app.app_version.clone()),
            ("unit".to_string(), "kj".to_string()),
        ];
        for (k, v) in params {
            body.push((k.to_string(), v.to_string()));
        }
        let req = self.authed(self.http.post(url))?.form(&body);
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
    /// RFC-3986-encoded). Legacy `.aspx` reads; writes go via `post_form`.
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
        if let Some(msg) = unable_to_save_message(&value) {
            return Err(AppError::Api {
                code: code.to_string(),
                message: msg.to_string(),
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

    /// Diary day: POST `RecipeJournalDayAndroidPage.aspx` with `{fl: 7}`.
    /// The app sends legacy reads as form POSTs with body credentials;
    /// the same page over GET returns only the `{dateint, guid}` shell.
    /// `days` selects the civil day (history included: same page + `dt`).
    pub async fn diary_day(&self, days: i64) -> Result<Value> {
        let url = format!("{}RecipeJournalDayAndroidPage.aspx", self.app.server_base);
        self.post_form_dated(&url, days, &[("fl", "7")]).await
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

    /// Custom food create: POST `RecipeCustomEntryActionAndroidPage.aspx`
    /// with `action=saveregional` plus nutrition/meta pairs (no photo upload;
    /// that is a separate image path).
    pub async fn food_create(&self, food: &CustomFood) -> Result<Value> {
        let url = self.legacy("RecipeCustomEntryActionAndroidPage.aspx");
        let params = custom_food_params(food);
        let refs: Vec<(&str, &str)> = params
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        self.post_form(&url, &refs).await
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

    /// Recipe create: POST `RecipeActionAndroidPage.aspx` with
    /// `{action:recipeinitialsave, prid:0, ...}` plus `fl=7`. The bare
    /// response carries the new id after a colon.
    pub async fn recipe_create(
        &self,
        title: &str,
        description: &str,
        portions: f64,
        prep_mins: i64,
        cook_mins: i64,
    ) -> Result<Value> {
        let url = self.legacy("RecipeActionAndroidPage.aspx");
        self.post_form(
            &url,
            &[
                ("action", "recipeinitialsave"),
                ("prid", "0"),
                ("title", title),
                ("description", description),
                ("portions", &portions.to_string()),
                ("preptime", &prep_mins.to_string()),
                ("cooktime", &cook_mins.to_string()),
                ("fl", "7"),
            ],
        )
        .await
    }

    /// Recipe edit: POST `RecipeActionAndroidPage.aspx` with
    /// `{action:recipesave, prid, ..., {typeId}_type, step{N}}` plus `fl=7`.
    #[allow(clippy::too_many_arguments)]
    pub async fn recipe_save(
        &self,
        id: i64,
        title: &str,
        description: &str,
        portions: f64,
        prep_mins: i64,
        cook_mins: i64,
        share: bool,
        steps: &[String],
        types: &[i64],
    ) -> Result<Value> {
        let url = self.legacy("RecipeActionAndroidPage.aspx");
        let mut params: Vec<(String, String)> = vec![
            ("action".to_string(), "recipesave".to_string()),
            ("prid".to_string(), id.to_string()),
            ("title".to_string(), title.to_string()),
            ("description".to_string(), description.to_string()),
            ("portions".to_string(), portions.to_string()),
            ("preptime".to_string(), prep_mins.to_string()),
            ("cooktime".to_string(), cook_mins.to_string()),
            ("osharing".to_string(), share.to_string()),
        ];
        for t in types {
            params.push((format!("{t}_type"), t.to_string()));
        }
        for (i, step) in steps.iter().enumerate() {
            params.push((format!("step{}", i + 1), step.clone()));
        }
        params.push(("fl".to_string(), "7".to_string()));
        let refs: Vec<(&str, &str)> = params
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        self.post_form(&url, &refs).await
    }

    /// Recipe delete: POST `RecipeActionAndroidPage.aspx` with
    /// `{action:recipedelete, rid, fl:5}` (note `rid`, not `prid`).
    pub async fn recipe_rm(&self, id: i64) -> Result<Value> {
        let url = self.legacy("RecipeActionAndroidPage.aspx");
        self.post_form(
            &url,
            &[
                ("action", "recipedelete"),
                ("rid", &id.to_string()),
                ("fl", "5"),
            ],
        )
        .await
    }

    /// Recipe ingredient save: POST `RecipeActionAndroidPage.aspx` with
    /// `{action:ingredientsave, fl:5, prid, rid, iid, entryname, portionid,
    /// portionamount}`.
    pub async fn recipe_add_ingredient(
        &self,
        recipe_id: i64,
        item_id: i64,
        food_id: i64,
        name: &str,
        portion_id: i64,
        units: f64,
    ) -> Result<Value> {
        let url = self.legacy("RecipeActionAndroidPage.aspx");
        self.post_form(
            &url,
            &[
                ("action", "ingredientsave"),
                ("fl", "5"),
                ("prid", &recipe_id.to_string()),
                ("rid", &food_id.to_string()),
                ("iid", &item_id.to_string()),
                ("entryname", name),
                ("portionid", &portion_id.to_string()),
                ("portionamount", &units.to_string()),
            ],
        )
        .await
    }

    /// Recipe ingredient delete: POST `RecipeActionAndroidPage.aspx` with
    /// `{action:ingredientdelete, fl:5, iid, prid}`.
    pub async fn recipe_rm_ingredient(&self, item_id: i64, recipe_id: i64) -> Result<Value> {
        let url = self.legacy("RecipeActionAndroidPage.aspx");
        self.post_form(
            &url,
            &[
                ("action", "ingredientdelete"),
                ("fl", "5"),
                ("iid", &item_id.to_string()),
                ("prid", &recipe_id.to_string()),
            ],
        )
        .await
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

    /// Block a user: authed POST `add-user-block` with `{blockUserId}`.
    /// No unblock call: `remove-user-block` exists in resources but nothing
    /// wires it in this APK build, and probing would mutate live block state.
    pub async fn feed_block_add(&self, user_id: i64) -> Result<Value> {
        let body = serde_json::json!({ "blockUserId": user_id });
        self.post_json(&self.app.feed_add_block_url, &body).await
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

    /// Course bookmark save/delete: `{guidedCourseContentId: id}`.
    pub async fn learning_course_bookmark(&self, id: i64, save: bool) -> Result<Value> {
        let url = if save {
            &self.app.learning_course_bookmark_save_url
        } else {
            &self.app.learning_course_bookmark_delete_url
        };
        self.post_json(url, &learning_bookmark_body(true, id)).await
    }

    /// Lesson bookmark save/delete: `{lessonContentId: id}`.
    pub async fn learning_lesson_bookmark(&self, id: i64, save: bool) -> Result<Value> {
        let url = if save {
            &self.app.learning_lesson_bookmark_save_url
        } else {
            &self.app.learning_lesson_bookmark_delete_url
        };
        self.post_json(url, &learning_bookmark_body(false, id))
            .await
    }

    /// Lesson progress (mark complete): `{lessonContentId: id}`.
    pub async fn learning_lesson_progress(&self, id: i64) -> Result<Value> {
        let url = &self.app.learning_lesson_progress_save_url;
        self.post_json(url, &learning_bookmark_body(false, id))
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

    /// Saved meal create/edit: POST `SavedMealActionAndroidPage.aspx` with
    /// `{action:save, mealid, title, description, mealtypes}` (mealid 0 to create).
    pub async fn meal_save(
        &self,
        meal_id: i64,
        title: &str,
        description: &str,
        meal_types: &str,
    ) -> Result<Value> {
        let url = self.legacy("SavedMealActionAndroidPage.aspx");
        self.post_form(
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

    /// Saved meal delete: POST `{action:delete, mealid}`.
    pub async fn meal_delete(&self, meal_id: i64) -> Result<Value> {
        let url = self.legacy("SavedMealActionAndroidPage.aspx");
        self.post_form(
            &url,
            &[("action", "delete"), ("mealid", &meal_id.to_string())],
        )
        .await
    }

    /// Saved meal duplicate: POST `{action:duplicate, meal, title,
    /// description, mealtypes}` (source server id in `meal`; no `mealid`).
    pub async fn meal_duplicate(
        &self,
        id: i64,
        title: &str,
        description: &str,
        meal_types: &str,
    ) -> Result<Value> {
        let url = self.legacy("SavedMealActionAndroidPage.aspx");
        self.post_form(
            &url,
            &[
                ("action", "duplicate"),
                ("meal", &id.to_string()),
                ("title", title),
                ("description", description),
                ("mealtypes", meal_types),
            ],
        )
        .await
    }

    /// Log a saved meal into the diary: POST `{action:add, mealid, meal}`.
    pub async fn meal_log(&self, meal_id: i64, meal: i64) -> Result<Value> {
        let url = self.legacy("SavedMealActionAndroidPage.aspx");
        self.post_form(
            &url,
            &[
                ("action", "add"),
                ("mealid", &meal_id.to_string()),
                ("meal", &meal.to_string()),
            ],
        )
        .await
    }

    /// Meal item save: POST `SavedMealItemActionAndroidPage.aspx` with
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
        self.post_form(
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

    /// Meal item delete: POST `{action:delete, itemid}`.
    pub async fn meal_item_delete(&self, item_id: i64) -> Result<Value> {
        let url = self.legacy("SavedMealItemActionAndroidPage.aspx");
        self.post_form(
            &url,
            &[("action", "delete"), ("itemid", &item_id.to_string())],
        )
        .await
    }

    /// Meal plan save: POST `MealPlanActionAndroidPage.aspx?action=save&fl=2`
    /// with a JSON body (hybrid transport: form query + JSON, like the app).
    pub async fn meal_plan_save(
        &self,
        plan_id: i64,
        name: &str,
        description: &str,
        entries: &[PlanEntry],
    ) -> Result<Value> {
        let url = format!(
            "{}MealPlanActionAndroidPage.aspx?action=save&fl=2",
            self.app.server_base
        );
        let body = meal_plan_body(plan_id, name, description, entries);
        self.post_json(&url, &body).await
    }

    /// Meal plan schedule: POST `MealPlanActionAndroidPage.aspx` with
    /// `action=schedule&fl=2` plus `{inserts, deletes}` of plan date ints.
    pub async fn meal_plan_schedule(
        &self,
        inserts: &[(i64, i64)],
        deletes: &[(i64, i64)],
    ) -> Result<Value> {
        let url = format!(
            "{}MealPlanActionAndroidPage.aspx?action=schedule&fl=2",
            self.app.server_base
        );
        let body = meal_plan_schedule_body(inserts, deletes);
        self.post_json(&url, &body).await
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

    /// RDI read: POST `RecommendedDailyIntakeAndroidPage.aspx`, no app params.
    /// Response keys are lowercase scalars (`weightkg`, `rdi`, ...).
    pub async fn rdi_show(&self) -> Result<Value> {
        let url = self.legacy("RecommendedDailyIntakeAndroidPage.aspx");
        self.post_form(&url, &[]).await
    }

    /// RDI save: POST `RecommendedDailyIntakeActionAndroidPage.aspx` with
    /// `{todaydt, action:save, ageInYears, weightKg, heightCm, sex, goal,
    /// activityLevel, rdi}`; sex/goal/activity are server ordinals.
    #[allow(clippy::too_many_arguments)]
    pub async fn rdi_save(
        &self,
        age: i64,
        weight_kg: f64,
        height_cm: f64,
        sex: i64,
        goal: i64,
        activity: i64,
        rdi: i64,
    ) -> Result<Value> {
        let url = self.legacy("RecommendedDailyIntakeActionAndroidPage.aspx");
        self.post_form(
            &url,
            &[
                ("todaydt", &crate::auth::device::today_days().to_string()),
                ("action", "save"),
                ("ageInYears", &age.to_string()),
                ("weightKg", &weight_kg.to_string()),
                ("heightCm", &height_cm.to_string()),
                ("sex", &sex.to_string()),
                ("goal", &goal.to_string()),
                ("activityLevel", &activity.to_string()),
                ("rdi", &rdi.to_string()),
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

    /// Exercise log: POST `ActivityEntryActionAndroidPage.aspx` with
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
        self.post_form(&url, &query).await
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

    /// Water log: POST `{action:savewater, consume: ml, type: 1 (delta),
    /// goal: daily ml, dateInt, fl: 2}`.
    pub async fn water_log(&self, ml: i64, goal_ml: i64, date_int: i64) -> Result<Value> {
        let url = format!(
            "{}WaterTrackingActionAndroidPage.aspx",
            self.app.server_base
        );
        self.post_form(
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

/// One meal-plan day entry: day index + food reference.
pub struct PlanEntry {
    pub day: i64,
    pub recipe_id: i64,
    pub portion_id: i64,
    pub amount: f64,
    pub meal: i64,
    pub name: String,
}

/// Parse `--entry DAY:RECIPE:PORTION:AMOUNT:MEAL:NAME` (colon split in six so
/// names may contain colons).
pub fn parse_plan_entry(s: &str) -> crate::error::Result<PlanEntry> {
    let mut parts = s.splitn(6, ':');
    let num = |p: Option<&str>, what: &str| -> crate::error::Result<f64> {
        p.unwrap_or("").trim().parse().map_err(|_| {
            crate::error::AppError::Msg(format!(
                "bad --entry {s:?}, want DAY:RECIPE:PORTION:AMOUNT:MEAL:NAME ({what})"
            ))
        })
    };
    let day = num(parts.next(), "day")? as i64;
    let recipe_id = num(parts.next(), "recipe")? as i64;
    let portion_id = num(parts.next(), "portion")? as i64;
    let amount = num(parts.next(), "amount")?;
    let meal = num(parts.next(), "meal")? as i64;
    let name = parts.next().unwrap_or("").trim().to_string();
    if name.is_empty() {
        return Err(crate::error::AppError::Msg(format!(
            "bad --entry {s:?}, want DAY:RECIPE:PORTION:AMOUNT:MEAL:NAME"
        )));
    }
    Ok(PlanEntry {
        day,
        recipe_id,
        portion_id,
        amount,
        meal,
        name,
    })
}

/// Parse `--insert/--delete PLAN:DAYINT` pairs.
pub fn parse_plan_day(s: &str, flag: &str) -> crate::error::Result<(i64, i64)> {
    let (p, d) = s.split_once(':').ok_or_else(|| {
        crate::error::AppError::Msg(format!("bad {flag} {s:?}, want PLAN:DAYINT"))
    })?;
    let plan = p
        .trim()
        .parse::<i64>()
        .map_err(|_| crate::error::AppError::Msg(format!("bad {flag} {s:?}, want PLAN:DAYINT")))?;
    let day = d
        .trim()
        .parse::<i64>()
        .map_err(|_| crate::error::AppError::Msg(format!("bad {flag} {s:?}, want PLAN:DAYINT")))?;
    Ok((plan, day))
}

/// Meal-plan save body: `mealPlanId`/`name`/`description` only when set,
/// entries grouped by day in flag order (`meal` omitted when 0).
pub fn meal_plan_body(plan_id: i64, name: &str, description: &str, entries: &[PlanEntry]) -> Value {
    let mut obj = serde_json::Map::new();
    if plan_id > 0 {
        obj.insert("mealPlanId".to_string(), plan_id.into());
    }
    if !name.is_empty() {
        obj.insert("name".to_string(), name.into());
    }
    if !description.is_empty() {
        obj.insert("description".to_string(), description.into());
    }
    let mut days: Vec<i64> = vec![];
    for e in entries {
        if !days.contains(&e.day) {
            days.push(e.day);
        }
    }
    let day_objs: Vec<Value> = days
        .iter()
        .map(|day| {
            let rows: Vec<Value> = entries
                .iter()
                .filter(|e| e.day == *day)
                .map(|e| {
                    let mut row = serde_json::Map::new();
                    row.insert("recipeId".to_string(), e.recipe_id.into());
                    if e.meal != 0 {
                        row.insert("meal".to_string(), e.meal.into());
                    }
                    row.insert("name".to_string(), e.name.clone().into());
                    row.insert("recipeportionId".to_string(), e.portion_id.into());
                    row.insert(
                        "portionAmount".to_string(),
                        serde_json::Number::from_f64(e.amount).map_or(Value::Null, Value::Number),
                    );
                    Value::Object(row)
                })
                .collect();
            serde_json::json!({ "day": day, "dailyRecipeEntries": rows })
        })
        .collect();
    obj.insert("days".to_string(), day_objs.into());
    Value::Object(obj)
}

/// Meal-plan schedule body: inserts/deletes grouped by plan, date ints as
/// strings, `mealPlanId` only when greater than 0.
pub fn meal_plan_schedule_body(inserts: &[(i64, i64)], deletes: &[(i64, i64)]) -> Value {
    fn group(pairs: &[(i64, i64)]) -> Vec<Value> {
        let mut plans: Vec<i64> = vec![];
        for (plan, _) in pairs {
            if !plans.contains(plan) {
                plans.push(*plan);
            }
        }
        plans
            .iter()
            .map(|plan| {
                let weeks: Vec<Value> = pairs
                    .iter()
                    .filter(|(p, _)| p == plan)
                    .map(|(_, d)| serde_json::json!(d.to_string()))
                    .collect();
                if *plan > 0 {
                    serde_json::json!({ "mealPlanId": plan, "weekNumber": weeks })
                } else {
                    serde_json::json!({ "weekNumber": weeks })
                }
            })
            .collect()
    }
    serde_json::json!({ "inserts": group(inserts), "deletes": group(deletes) })
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

/// Custom food fields for `action=saveregional`. Nutrition values are
/// pre-rendered strings (number or `""` when unset — the app always sends
/// every key). Calcium/iron/vitamin-A/C use the non-Mg names; the Mg/Mcg
/// variants apply per market locale (unverified which market gates them).
pub struct CustomFood {
    pub serving_type: String,
    pub serving_size: String,
    pub calories: String,
    pub total_fat: String,
    pub saturated_fat: String,
    pub cholesterol: String,
    pub sodium: String,
    pub potassium: String,
    pub carbohydrate: String,
    pub fiber: String,
    pub sugar: String,
    pub protein: String,
    pub metric_serving_size: String,
    pub calcium: String,
    pub iron: String,
    pub vitamin_a: String,
    pub vitamin_c: String,
    pub manufacturer_type: i64,
    pub manufacturer_name: String,
    pub product_name: String,
    pub tags: String,
    pub is_salt: bool,
    pub barcode: String,
    pub barcode_type: String,
}

/// App-ordered param grid: nutrition pairs first, then meta. Every nutrition
/// key is always present (empty when unset); barcode pairs only with barcode.
pub fn custom_food_params(food: &CustomFood) -> Vec<(String, String)> {
    let mut params = vec![
        ("servingType".to_string(), food.serving_type.clone()),
        ("servingSize".to_string(), food.serving_size.clone()),
        ("calories".to_string(), food.calories.clone()),
        ("totalFat".to_string(), food.total_fat.clone()),
        ("saturatedFat".to_string(), food.saturated_fat.clone()),
        ("cholesterol".to_string(), food.cholesterol.clone()),
        ("sodium".to_string(), food.sodium.clone()),
        ("potassium".to_string(), food.potassium.clone()),
        ("carbohydrate".to_string(), food.carbohydrate.clone()),
        ("fiber".to_string(), food.fiber.clone()),
        ("sugar".to_string(), food.sugar.clone()),
        ("protein".to_string(), food.protein.clone()),
        (
            "metricServingSize".to_string(),
            food.metric_serving_size.clone(),
        ),
        ("calcium".to_string(), food.calcium.clone()),
        ("iron".to_string(), food.iron.clone()),
        ("vitaminA".to_string(), food.vitamin_a.clone()),
        ("vitaminC".to_string(), food.vitamin_c.clone()),
        ("action".to_string(), "saveregional".to_string()),
        (
            "manufacturerType".to_string(),
            food.manufacturer_type.to_string(),
        ),
        (
            "manufacturerName".to_string(),
            food.manufacturer_name.clone(),
        ),
        ("productName".to_string(), food.product_name.clone()),
        ("tags".to_string(), food.tags.clone()),
        ("isSalt".to_string(), food.is_salt.to_string()),
    ];
    if !food.barcode.is_empty() {
        params.push(("barcode".to_string(), food.barcode.clone()));
        params.push(("barcodeType".to_string(), food.barcode_type.clone()));
    }
    params
}
/// Vote body with the exact `FoodDietaryPreferenceVoteRequestDTO` key set.
pub fn vote_body(recipe_id: i64, votes: &[(i64, String)]) -> Value {
    let votes: Vec<Value> = votes
        .iter()
        .map(|(t, v)| serde_json::json!({ "typeId": t, "value": v }))
        .collect();
    serde_json::json!({ "recipeid": recipe_id, "votes": votes })
}

/// Learning save body: `guidedCourseContentId` for courses,
/// `lessonContentId` for lessons/progress (single-id DTO).
pub fn learning_bookmark_body(course: bool, id: i64) -> Value {
    if course {
        serde_json::json!({ "guidedCourseContentId": id })
    } else {
        serde_json::json!({ "lessonContentId": id })
    }
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

/// Legacy bare-string failure marker: some `.aspx` action pages answer
/// HTTP 200 with plain text containing `Unable to save`. Returns the
/// message when present so `finish` can surface it as an error.
fn unable_to_save_message(value: &Value) -> Option<&str> {
    match value {
        Value::String(s) if s.contains("Unable to save") => Some(s.as_str()),
        _ => None,
    }
}

fn trim_xml_prolog(s: &str) -> &str {
    let t = s.trim_start();
    if let Some(rest) = t.strip_prefix("<?")
        && let Some(end) = rest.find("?>")
    {
        return rest[end + 2..].trim_start();
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

    #[test]
    fn detects_unable_to_save_string_body() {
        let v = parse_body("Unable to save").unwrap();
        assert_eq!(unable_to_save_message(&v), Some("Unable to save"));
        let v = parse_body(r#"{"ok":true}"#).unwrap();
        assert_eq!(unable_to_save_message(&v), None);
        assert_eq!(unable_to_save_message(&Value::Null), None);
    }

    #[test]
    fn learning_bodies_use_exact_keys() {
        assert_eq!(
            learning_bookmark_body(true, 7),
            serde_json::json!({ "guidedCourseContentId": 7 })
        );
        assert_eq!(
            learning_bookmark_body(false, 9),
            serde_json::json!({ "lessonContentId": 9 })
        );
    }

    #[test]
    fn custom_food_params_keep_app_order() {
        let food = CustomFood {
            serving_type: "".to_string(),
            serving_size: "1 cup".to_string(),
            calories: "100".to_string(),
            total_fat: "".to_string(),
            saturated_fat: "".to_string(),
            cholesterol: "".to_string(),
            sodium: "".to_string(),
            potassium: "".to_string(),
            carbohydrate: "".to_string(),
            fiber: "".to_string(),
            sugar: "".to_string(),
            protein: "5".to_string(),
            metric_serving_size: "100g".to_string(),
            calcium: "".to_string(),
            iron: "".to_string(),
            vitamin_a: "".to_string(),
            vitamin_c: "".to_string(),
            manufacturer_type: 0,
            manufacturer_name: "".to_string(),
            product_name: "Probe".to_string(),
            tags: "".to_string(),
            is_salt: false,
            barcode: "".to_string(),
            barcode_type: "EAN_13".to_string(),
        };
        let params = custom_food_params(&food);
        // Nutrition first, then meta; unset keys present-but-empty.
        assert_eq!(params[0], ("servingType".to_string(), "".to_string()));
        assert_eq!(params[1], ("servingSize".to_string(), "1 cup".to_string()));
        assert!(params.iter().any(|(k, v)| k == "protein" && v == "5"));
        let action = params.iter().find(|(k, _)| k == "action").unwrap();
        assert_eq!(action.1, "saveregional");
        assert!(!params.iter().any(|(k, _)| k == "barcode"));
    }

    #[test]
    fn parses_plan_entries_and_days() {
        let e = parse_plan_entry("1:39715:62446:2.0:1:Oats: hot").unwrap();
        assert_eq!(
            (e.day, e.recipe_id, e.portion_id, e.meal),
            (1, 39715, 62446, 1)
        );
        assert!((e.amount - 2.0).abs() < 1e-9);
        assert_eq!(e.name, "Oats: hot");
        assert!(parse_plan_entry("garbage").is_err());
        assert_eq!(parse_plan_day("5:20701", "--insert").unwrap(), (5, 20701));
        assert!(parse_plan_day("nope", "--insert").is_err());
    }

    #[test]
    fn meal_plan_bodies_omit_empties() {
        let entries = vec![
            PlanEntry {
                day: 1,
                recipe_id: 10,
                portion_id: 20,
                amount: 1.0,
                meal: 1,
                name: "A".to_string(),
            },
            PlanEntry {
                day: 1,
                recipe_id: 11,
                portion_id: 21,
                amount: 2.0,
                meal: 0,
                name: "B".to_string(),
            },
        ];
        let v = meal_plan_body(0, "Week", "", &entries);
        assert!(v.get("mealPlanId").is_none());
        assert_eq!(v["name"], serde_json::json!("Week"));
        assert!(v.get("description").is_none());
        assert_eq!(v["days"][0]["dailyRecipeEntries"][1].get("meal"), None);
        let v = meal_plan_schedule_body(&[(5, 20701)], &[]);
        assert_eq!(v["inserts"][0]["mealPlanId"], serde_json::json!(5));
        assert_eq!(v["inserts"][0]["weekNumber"], serde_json::json!(["20701"]));
        assert_eq!(v["deletes"].as_array().unwrap().len(), 0);
    }
}
