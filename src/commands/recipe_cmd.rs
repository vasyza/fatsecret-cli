use crate::api::FsClient;
use crate::cli::{OutputFormat, RecipesAction, RecipesArgs};
use crate::error::Result;
use crate::output::{emit, render_food_get, render_recipes, render_scalars};

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
            let (human, plain) = render_scalars(&v, "cookbook count (see --format json)");
            emit(format, human.trim_end(), plain.trim_end(), &v)
        }
        RecipesAction::Create {
            title,
            description,
            portions,
            prep_time,
            cook_time,
        } => {
            let v = client
                .recipe_create(&title, &description, portions, prep_time, cook_time)
                .await?;
            emit(format, &recipe_created(&v), "OK", &v)
        }
        RecipesAction::Save {
            id,
            title,
            description,
            portions,
            prep_time,
            cook_time,
            share,
            steps,
            types,
        } => {
            let v = client
                .recipe_save(
                    id,
                    &title,
                    &description,
                    portions,
                    prep_time,
                    cook_time,
                    share,
                    &steps,
                    &types,
                )
                .await?;
            emit(format, "recipe saved", "OK", &v)
        }
        RecipesAction::Rm { id } => {
            let v = client.recipe_rm(id).await?;
            emit(format, "recipe deleted", "OK", &v)
        }
        RecipesAction::AddIngredient {
            recipe_id,
            food_id,
            name,
            portion_id,
            units,
            item_id,
        } => {
            let v = client
                .recipe_add_ingredient(recipe_id, item_id, food_id, &name, portion_id, units)
                .await?;
            emit(format, "ingredient saved", "OK", &v)
        }
        RecipesAction::RmIngredient { item_id, recipe_id } => {
            let v = client.recipe_rm_ingredient(item_id, recipe_id).await?;
            emit(format, "ingredient deleted", "OK", &v)
        }
    }
}

/// The create response is a bare `...:<newId>` string: surface the id,
/// or the raw body when the shape differs.
fn recipe_created(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => match s.rsplit(':').next() {
            Some(id) if !id.is_empty() && id != s => format!("recipe created: {id}"),
            _ => format!("recipe created: {s}"),
        },
        _ => "recipe created".to_string(),
    }
}
