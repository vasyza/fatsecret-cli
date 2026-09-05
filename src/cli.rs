use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

/// Terminal client mirroring the FatSecret mobile app.
///
/// Same authorization, same requests: food search today; diary, recipes and
/// barcode return with their request-shape slices.
#[derive(Parser, Debug)]
#[command(
    name = "fatsecret-cli",
    version,
    about,
    long_about = None,
    propagate_version = true
)]
pub struct Cli {
    /// Increase log verbosity (-v, -vv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Config file (TOML): endpoints and device model
    #[arg(short, long, global = true, env = "FATSECRET_CONFIG")]
    pub config: Option<PathBuf>,

    /// Output format
    #[arg(long, value_enum, default_value_t = OutputFormat::Human, global = true)]
    pub format: OutputFormat,

    /// Controls log color on stderr; data output stays uncolored
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto, global = true)]
    pub color: ColorChoice,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Log in with your fatsecret.com account (password via TTY prompt)
    Auth(AuthArgs),
    /// Search the food database
    Foods(FoodsArgs),
    /// Recipes and cookbook
    Recipes(RecipesArgs),
    /// Food diary
    Diary(DiaryArgs),
    /// Body weight
    Weight(WeightArgs),
    /// Exercise diary
    Exercise(ExerciseArgs),
    /// Water tracker
    Water(WaterArgs),
    /// Saved meals
    Meals(MealArgs),
    /// Account details and settings
    Account(AccountArgs),
    /// User settings and attributes
    Settings(SettingsArgs),
    /// Notifications
    Notifications(NotificationsArgs),
    /// Social feed blocks
    Feed(FeedArgs),
    /// Learning content and progress
    Learning(LearningArgs),
    /// Static food-groups data
    FoodGroups(FoodGroupsArgs),
    /// Print shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Args, Debug)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub action: AuthAction,
}

#[derive(Subcommand, Debug)]
pub enum AuthAction {
    /// Log in as USERNAME (asks for the password interactively)
    Login {
        /// FatSecret username or email
        username: String,
    },
    /// Register a new account (asks for the password interactively)
    Register {
        /// Email address
        email: String,
        /// Desired member name
        username: String,
        /// Birth date YYYY-MM-DD
        #[arg(long)]
        birth_date: String,
        /// Gender as the app sends it (e.g. Male/Female)
        #[arg(long)]
        gender: String,
        /// Country code, e.g. US
        #[arg(long, default_value = "US")]
        country: String,
        /// Current weight in kg
        #[arg(long)]
        current_weight_kg: Option<f64>,
        /// Goal weight in kg
        #[arg(long)]
        goal_weight_kg: Option<f64>,
        /// Height in cm
        #[arg(long)]
        height_cm: Option<f64>,
        /// First name
        #[arg(long)]
        first_name: Option<String>,
    },
    /// Send a password-reset email
    ForgotPassword {
        /// Account email address
        email: String,
    },
    /// Complete a password reset (code + new password via prompts)
    ResetPassword,
    /// Show login state (username only, never secrets)
    Status,
    /// Delete stored credentials
    Logout,
}

#[derive(Args, Debug)]
pub struct AccountArgs {
    #[command(subcommand)]
    pub action: AccountAction,
}

#[derive(Subcommand, Debug)]
pub enum AccountAction {
    /// Show account details
    Show,
    /// Show account settings page
    Settings,
    /// Change member name (takes effect immediately)
    ChangeUsername {
        /// New member name
        name: String,
    },
}

#[derive(Args, Debug)]
pub struct DiaryArgs {
    #[command(subcommand)]
    pub action: DiaryAction,
}

#[derive(Subcommand, Debug)]
pub enum DiaryAction {
    /// Show the diary day (default today)
    Day {
        /// Date YYYY-MM-DD; must equal today. History is unsupported.
        #[arg(long)]
        date: Option<String>,
    },
    /// Log a food entry
    Add {
        /// Food id from search
        #[arg(long)]
        food_id: i64,
        /// Serving/portion id from food details
        #[arg(long)]
        serving_id: i64,
        /// Meal: breakfast|lunch|dinner|snack|other (or --meal-id INT)
        #[arg(long, default_value = "other")]
        meal: String,
        /// Numeric meal id override
        #[arg(long)]
        meal_id: Option<i64>,
        /// Date YYYY-MM-DD (default today)
        #[arg(long)]
        date: Option<String>,
        /// Number of servings
        #[arg(long, default_value_t = 1.0)]
        units: f64,
    },
    /// Delete an entry by id; keep the add response (day output does not reliably expose entry IDs)
    Rm {
        /// Entry id
        entry_id: i64,
        /// Entry recorded date YYYY-MM-DD (default today)
        #[arg(long)]
        date: Option<String>,
    },
}

#[derive(Args, Debug)]
pub struct WeightArgs {
    #[command(subcommand)]
    pub action: WeightAction,
}

#[derive(Subcommand, Debug)]
pub enum WeightAction {
    /// Log body weight in kg (goal defaults to current account goal)
    Log {
        /// Weight in kilograms
        kg: f64,
        /// Goal weight in kg (default: account goal)
        #[arg(long)]
        goal_kg: Option<f64>,
    },
}

#[derive(Args, Debug)]
pub struct ExerciseArgs {
    #[command(subcommand)]
    pub action: ExerciseAction,
}

#[derive(Subcommand, Debug)]
pub enum ExerciseAction {
    /// Show today's exercise entries
    Day,
    /// List activity types
    Types,
    /// Log an activity
    Log {
        /// Activity type id (see `exercise types`)
        #[arg(long)]
        type_id: i64,
        /// Minutes performed
        #[arg(long)]
        mins: i64,
        /// Calories expended (optional, server estimates otherwise)
        #[arg(long)]
        kcal: Option<f64>,
        /// Free-text note
        #[arg(long)]
        description: Option<String>,
    },
}

#[derive(Args, Debug)]
pub struct WaterArgs {
    #[command(subcommand)]
    pub action: WaterAction,
}

#[derive(Subcommand, Debug)]
pub enum WaterAction {
    /// Show today's water entry
    Day,
    /// Log water intake in ml
    Log {
        /// Milliliters consumed
        ml: i64,
        /// Daily goal in ml (default 2000)
        #[arg(long, default_value_t = 2000)]
        goal_ml: i64,
    },
}

#[derive(Args, Debug)]
pub struct MealArgs {
    #[command(subcommand)]
    pub action: MealAction,
}

#[derive(Subcommand, Debug)]
pub enum MealAction {
    /// List saved meals
    Ls {
        /// Meal-tab ordinal filter
        #[arg(long, default_value_t = 0)]
        meal: i64,
    },
    /// Show one saved meal with items
    Show {
        /// Saved meal id
        id: i64,
    },
    /// Create a saved meal (mealid 0)
    Create {
        /// Title
        #[arg(long)]
        title: String,
        /// Description
        #[arg(long, default_value = "")]
        description: String,
        /// Meal types mask
        #[arg(long, default_value = "1")]
        meal_types: String,
    },
    /// Edit a saved meal
    Save {
        /// Saved meal id
        id: i64,
        /// Title
        #[arg(long)]
        title: String,
        /// Description
        #[arg(long, default_value = "")]
        description: String,
        /// Meal types mask
        #[arg(long, default_value = "1")]
        meal_types: String,
    },
    /// Delete a saved meal
    Rm {
        /// Saved meal id
        id: i64,
    },
    /// Log a saved meal into the diary
    Log {
        /// Saved meal id
        id: i64,
        /// Meal: breakfast|lunch|dinner|snack|other (or --meal-id INT)
        #[arg(long, default_value = "other")]
        meal: String,
        /// Numeric meal id override
        #[arg(long)]
        meal_id: Option<i64>,
    },
    /// Add (or edit, with --item-id) a food item in a saved meal
    AddItem {
        /// Saved meal id
        #[arg(long)]
        meal_id: i64,
        /// Food id from search
        #[arg(long)]
        food_id: i64,
        /// Entry name
        #[arg(long)]
        name: String,
        /// Portion id from food details
        #[arg(long)]
        portion_id: i64,
        /// Number of servings
        #[arg(long, default_value_t = 1.0)]
        units: f64,
        /// Existing item id (edit instead of add)
        #[arg(long, default_value_t = 0)]
        item_id: i64,
    },
    /// Delete a food item from a saved meal
    RmItem {
        /// Item id
        item_id: i64,
    },
    /// Quick picks list
    Quickpicks,
}

#[derive(Args, Debug)]
pub struct SettingsArgs {
    #[command(subcommand)]
    pub action: SettingsAction,
}

#[derive(Subcommand, Debug)]
pub enum SettingsAction {
    /// User attributes (birth date, gender, names)
    Attributes,
    /// Locale configuration (language id)
    Locale,
}

#[derive(Args, Debug)]
pub struct NotificationsArgs {
    #[command(subcommand)]
    pub action: NotificationsAction,
}

#[derive(Subcommand, Debug)]
pub enum NotificationsAction {
    /// List notifications
    Ls,
}

#[derive(Args, Debug)]
pub struct FeedArgs {
    #[command(subcommand)]
    pub action: FeedAction,
}

#[derive(Subcommand, Debug)]
pub enum FeedAction {
    /// Users blocking you
    Blocks,
    /// Users you block
    Blocking,
}

#[derive(Args, Debug)]
pub struct LearningArgs {
    #[command(subcommand)]
    pub action: LearningAction,
}

#[derive(Subcommand, Debug)]
pub enum LearningAction {
    /// Learning progress and bookmarks
    Progress,
    /// Learning content for your language
    Content,
}

#[derive(Args, Debug)]
pub struct FoodGroupsArgs {}

#[derive(Args, Debug)]
pub struct FoodsArgs {
    #[command(subcommand)]
    pub action: FoodsAction,
}

#[derive(Args, Debug)]
pub struct RecipesArgs {
    #[command(subcommand)]
    pub action: RecipesAction,
}

#[derive(Subcommand, Debug)]
pub enum RecipesAction {
    /// Search recipes (legacy cookbook index)
    Search {
        /// Search expression
        query: String,
        /// Zero-based page number
        #[arg(long, default_value_t = 0)]
        page: i64,
    },
    /// Full details for a recipe id (same page as food details)
    Get {
        /// Recipe id
        id: i64,
    },
    /// List recipe categories
    Categories,
    /// Search the cookbook index
    CookbookSearch {
        /// Search expression
        query: String,
        /// Zero-based page number
        #[arg(long, default_value_t = 0)]
        page: i64,
        /// Results per page
        #[arg(long, default_value_t = 20)]
        size: i64,
    },
    /// Cookbook recipe count per market
    CookbookCount {
        /// Market locale, e.g. US
        #[arg(long, default_value = "US")]
        market: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum FoodsAction {
    /// Search the food database
    Search {
        /// Search expression, e.g. "oatmeal"
        query: String,
        /// Zero-based page number
        #[arg(long, default_value_t = 0)]
        page: i64,
        /// Results per page
        #[arg(long, default_value_t = 20)]
        size: i64,
    },
    /// Full details for a food id (portions included when served)
    Get {
        /// Food id from search
        id: i64,
    },
    /// Most popular serving sizes for one or more food ids
    Popular {
        /// Food ids
        #[arg(required = true)]
        ids: Vec<i64>,
    },
    /// Vote on a dietary preference: repeat --vote TYPE=VALUE
    VotePreference {
        /// Food id
        id: i64,
        /// Vote as TYPE=VALUE, repeatable
        #[arg(long = "vote", value_name = "TYPE=VALUE")]
        votes: Vec<String>,
    },
    /// Scan a barcode (GTIN) and show the food
    Barcode {
        /// GTIN digits
        gtin: String,
    },
    /// List dietary-preference/allergen types for votes
    PreferenceTypes,
}

#[derive(ValueEnum, Clone, Copy, Debug, Default)]
pub enum OutputFormat {
    /// Pretty tables for humans
    #[default]
    Human,
    /// JSON to stdout (pipe-safe)
    Json,
    /// Minimal tab-separated output for scripts
    Plain,
}

#[derive(ValueEnum, Clone, Copy, Debug, Default)]
pub enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}
