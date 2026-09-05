use crate::api::FsClient;
use crate::cli::{
    FeedAction, FeedArgs, FoodGroupsArgs, LearningAction, LearningArgs, NotificationsAction,
    NotificationsArgs, OutputFormat, SettingsAction, SettingsArgs,
};
use crate::error::Result;
use crate::output::{emit, render_feed, render_recipes, render_water};

pub async fn run_settings(
    client: &FsClient,
    format: OutputFormat,
    args: SettingsArgs,
) -> Result<()> {
    let v = match args.action {
        SettingsAction::Attributes => client.user_attributes().await?,
        SettingsAction::Locale => client.settings_locale().await?,
    };
    let (human, plain) = render_water(&v, "no settings data (see --format json)");
    emit(format, human.trim_end(), plain.trim_end(), &v)
}

pub async fn run_notifications(
    client: &FsClient,
    format: OutputFormat,
    args: NotificationsArgs,
) -> Result<()> {
    match args.action {
        NotificationsAction::Ls => {
            let v = client.notifications().await?;
            let (human, plain) = render_water(&v, "no notifications");
            emit(format, human.trim_end(), plain.trim_end(), &v)
        }
    }
}

pub async fn run_feed(client: &FsClient, format: OutputFormat, args: FeedArgs) -> Result<()> {
    let v = match args.action {
        FeedAction::Blocks => client.feed_blocks().await?,
        FeedAction::Blocking => client.feed_blocking().await?,
    };
    let (human, plain) = render_feed(&v);
    emit(format, human.trim_end(), plain.trim_end(), &v)
}

pub async fn run_learning(
    client: &FsClient,
    format: OutputFormat,
    args: LearningArgs,
) -> Result<()> {
    let v = match args.action {
        LearningAction::Progress => client.learning_progress().await?,
        LearningAction::Content => client.learning_content().await?,
    };
    emit(format, "learning data (see --format json)", "", &v)
}

pub async fn run_food_groups(
    client: &FsClient,
    format: OutputFormat,
    _args: FoodGroupsArgs,
) -> Result<()> {
    let v = client.food_groups().await?;
    let (human, plain) = render_recipes(&v);
    emit(format, human.trim_end(), plain.trim_end(), &v)
}
