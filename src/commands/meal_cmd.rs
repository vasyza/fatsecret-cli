use crate::api::FsClient;
use crate::cli::{MealAction, MealArgs, OutputFormat};
use crate::error::Result;
use crate::output::{emit, render_meals};

pub async fn run(client: &FsClient, format: OutputFormat, args: MealArgs) -> Result<()> {
    match args.action {
        MealAction::Ls { meal } => {
            let v = client.meals_list(meal).await?;
            let (human, plain) = render_meals(&v);
            emit(format, human.trim_end(), plain.trim_end(), &v)
        }
        MealAction::Show { id } => {
            let v = client.meal_show(id).await?;
            let (human, plain) = render_meals(&v);
            emit(format, human.trim_end(), plain.trim_end(), &v)
        }
        MealAction::Create {
            title,
            description,
            meal_types,
        } => {
            let v = client
                .meal_save(0, &title, &description, &meal_types)
                .await?;
            emit(format, "meal created", "OK", &v)
        }
        MealAction::Save {
            id,
            title,
            description,
            meal_types,
        } => {
            let v = client
                .meal_save(id, &title, &description, &meal_types)
                .await?;
            emit(format, "meal saved", "OK", &v)
        }
        MealAction::Rm { id } => {
            let v = client.meal_delete(id).await?;
            emit(format, "meal deleted", "OK", &v)
        }
        MealAction::Log { id, meal, meal_id } => {
            let m = crate::commands::diary_cmd::meal_id(&meal, meal_id)?;
            let v = client.meal_log(id, m).await?;
            emit(format, "meal logged", "OK", &v)
        }
        MealAction::AddItem {
            meal_id,
            food_id,
            name,
            portion_id,
            units,
            item_id,
        } => {
            let v = client
                .meal_item_save(meal_id, item_id, food_id, &name, portion_id, units)
                .await?;
            emit(format, "meal item saved", "OK", &v)
        }
        MealAction::RmItem { item_id } => {
            let v = client.meal_item_delete(item_id).await?;
            emit(format, "meal item deleted", "OK", &v)
        }
        MealAction::Quickpicks => {
            let v = client.quick_picks().await?;
            let (human, plain) = render_meals(&v);
            emit(format, human.trim_end(), plain.trim_end(), &v)
        }
    }
}
