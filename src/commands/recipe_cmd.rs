use crate::api::FsClient;
use crate::cli::{OutputFormat, RecipesAction, RecipesArgs};
use crate::error::Result;
use crate::output::{emit, render_food_get, render_recipes};

pub async fn run(client: &FsClient, format: OutputFormat, args: RecipesArgs) -> Result<()> {
    match args.action {
        RecipesAction::Search { query, page } => {
            let v = client.recipes_search(&query, page).await?;
            let (human, plain) = render_recipes(&v);
            emit(format, human.trim_end(), plain.trim_end(), &v)
        }
        RecipesAction::Get { id } => {
            let v = client.recipe_get(id).await?;
            let (human, plain) = render_food_get(&v);
            emit(format, human.trim_end(), plain.trim_end(), &v)
        }
        RecipesAction::Categories => {
            let v = client.recipe_categories().await?;
            let (human, plain) = render_recipes(&v);
            emit(format, human.trim_end(), plain.trim_end(), &v)
        }
        RecipesAction::CookbookSearch { query, page, size } => {
            let v = client.cookbook_search(&query, page, size).await?;
            let (human, plain) = render_recipes(&v);
            emit(format, human.trim_end(), plain.trim_end(), &v)
        }
        RecipesAction::CookbookCount { market } => {
            let v = client.cookbook_count(&market).await?;
            emit(format, "cookbook count (see --format json)", "", &v)
        }
    }
}
