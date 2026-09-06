pub mod account_cmd;
pub mod auth_cmd;
pub mod body_cmd;
pub mod config_cmd;
pub mod diary_cmd;
pub mod food_cmd;
pub mod meal_cmd;
pub mod meal_plan_cmd;
pub mod misc_cmd;
pub mod recipe_cmd;

use reqwest::Client;

use crate::api::FsClient;
use crate::auth::FileStore;
use crate::cli::{Cli, Commands, OutputFormat};
use crate::config::AppConfig;
use crate::error::Result;
/// Build a client; attach stored (or env) credentials when present.
pub fn app_client(app: &AppConfig) -> Result<FsClient> {
    let store = FileStore::platform()?;
    let creds = store.load().ok();
    FsClient::new(app.clone(), creds)
}

pub fn http_client() -> Client {
    Client::new()
}

pub async fn dispatch(cli: Cli) -> Result<()> {
    let app = AppConfig::resolve(cli.config.as_deref())?;
    let format: OutputFormat = cli.format;
    match cli.command {
        Commands::Auth(a) => auth_cmd::run(&app, format, a).await,
        Commands::Foods(f) => food_cmd::run(&app_client(&app)?, format, f).await,
        Commands::Recipes(r) => recipe_cmd::run(&app_client(&app)?, format, r).await,
        Commands::Diary(d) => diary_cmd::run(&app_client(&app)?, format, d).await,
        Commands::Weight(w) => body_cmd::run_weight(&app_client(&app)?, format, w).await,
        Commands::Rdi(r) => body_cmd::run_rdi(&app_client(&app)?, format, r).await,
        Commands::Exercise(e) => body_cmd::run_exercise(&app_client(&app)?, format, e).await,
        Commands::Water(w) => body_cmd::run_water(&app_client(&app)?, format, w).await,
        Commands::Meals(m) => meal_cmd::run(&app_client(&app)?, format, m).await,
        Commands::MealPlans(m) => meal_plan_cmd::run(&app_client(&app)?, format, m).await,
        Commands::Settings(s) => misc_cmd::run_settings(&app_client(&app)?, format, s).await,
        Commands::Notifications(n) => {
            misc_cmd::run_notifications(&app_client(&app)?, format, n).await
        }
        Commands::Feed(f) => misc_cmd::run_feed(&app_client(&app)?, format, f).await,
        Commands::Learning(l) => misc_cmd::run_learning(&app_client(&app)?, format, l).await,
        Commands::FoodGroups(g) => misc_cmd::run_food_groups(&app_client(&app)?, format, g).await,
        Commands::Config(c) => config_cmd::run(&app, format, c).await,
        Commands::Account(a) => account_cmd::run(&app, a, format).await,
        Commands::Completions { .. } => Ok(()), // handled in main
    }
}
