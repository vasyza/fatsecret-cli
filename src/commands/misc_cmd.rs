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
    match args.action {
        FeedAction::Blocks => {
            let v = client.feed_blocks().await?;
            let (human, plain) = render_feed(&v);
            emit(format, human.trim_end(), plain.trim_end(), &v)
        }
        FeedAction::Blocking => {
            let v = client.feed_blocking().await?;
            let (human, plain) = render_feed(&v);
            emit(format, human.trim_end(), plain.trim_end(), &v)
        }
        FeedAction::Block { user_id } => {
            let v = client.feed_block_add(user_id).await?;
            emit(format, "user blocked", "OK", &v)
        }
    }
}

pub async fn run_learning(
    client: &FsClient,
    format: OutputFormat,
    args: LearningArgs,
) -> Result<()> {
    match args.action {
        LearningAction::Progress => {
            let v = client.learning_progress().await?;
            emit(format, "learning data (see --format json)", "", &v)
        }
        LearningAction::Content => {
            let v = client.learning_content().await?;
            emit(format, "learning data (see --format json)", "", &v)
        }
        LearningAction::BookmarkCourse { id } => {
            let v = client.learning_course_bookmark(id, true).await?;
            emit(format, "course bookmarked", "OK", &v)
        }
        LearningAction::UnbookmarkCourse { id } => {
            let v = client.learning_course_bookmark(id, false).await?;
            emit(format, "course unbookmarked", "OK", &v)
        }
        LearningAction::BookmarkLesson { id } => {
            let v = client.learning_lesson_bookmark(id, true).await?;
            emit(format, "lesson bookmarked", "OK", &v)
        }
        LearningAction::UnbookmarkLesson { id } => {
            let v = client.learning_lesson_bookmark(id, false).await?;
            emit(format, "lesson unbookmarked", "OK", &v)
        }
        LearningAction::CompleteLesson { id } => {
            let v = client.learning_lesson_progress(id).await?;
            emit(format, "lesson completed", "OK", &v)
        }
    }
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
