use crate::api::{FsClient, parse_plan_day, parse_plan_entry};
use crate::cli::{MealPlansAction, MealPlansArgs, OutputFormat};
use crate::error::Result;
use crate::output::emit;

pub async fn run(client: &FsClient, format: OutputFormat, args: MealPlansArgs) -> Result<()> {
    match args.action {
        MealPlansAction::Save {
            plan_id,
            name,
            description,
            entries,
        } => {
            let mut parsed = Vec::with_capacity(entries.len());
            for e in &entries {
                parsed.push(parse_plan_entry(e)?);
            }
            let v = client
                .meal_plan_save(plan_id, &name, &description, &parsed)
                .await?;
            emit(format, "meal plan saved", "OK", &v)
        }
        MealPlansAction::Schedule { inserts, deletes } => {
            let mut ins = Vec::with_capacity(inserts.len());
            for s in &inserts {
                ins.push(parse_plan_day(s, "--insert")?);
            }
            let mut dels = Vec::with_capacity(deletes.len());
            for s in &deletes {
                dels.push(parse_plan_day(s, "--delete")?);
            }
            let v = client.meal_plan_schedule(&ins, &dels).await?;
            emit(format, "meal plan scheduled", "OK", &v)
        }
    }
}
