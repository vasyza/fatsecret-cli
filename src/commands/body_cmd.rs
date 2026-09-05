use crate::api::FsClient;
use crate::auth::device::today_days;
use crate::cli::{
    ExerciseAction, ExerciseArgs, OutputFormat, WaterAction, WaterArgs, WeightAction, WeightArgs,
};
use crate::error::Result;
use crate::output::{emit, render_exercise, render_water};

pub async fn run_weight(client: &FsClient, format: OutputFormat, args: WeightArgs) -> Result<()> {
    match args.action {
        WeightAction::Log { kg, goal_kg } => {
            let goal = match goal_kg {
                Some(g) => g,
                None => {
                    // Keep the current account goal: read-modify-write.
                    let details = client.user_details().await?;
                    details
                        .get("goalWeightKg")
                        .and_then(|g| g.as_f64())
                        .unwrap_or(kg)
                }
            };
            let v = client.weight_log(kg, goal).await?;
            emit(format, &format!("weight logged: {kg} kg"), "OK", &v)
        }
    }
}

pub async fn run_exercise(
    client: &FsClient,
    format: OutputFormat,
    args: ExerciseArgs,
) -> Result<()> {
    match args.action {
        ExerciseAction::Day => {
            let v = client.exercise_day().await?;
            let (human, plain) = render_exercise(&v);
            emit(format, human.trim_end(), plain.trim_end(), &v)
        }
        ExerciseAction::Types => {
            let v = client.exercise_types().await?;
            let (human, plain) = render_exercise(&v);
            emit(format, human.trim_end(), plain.trim_end(), &v)
        }
        ExerciseAction::Log {
            type_id,
            mins,
            kcal,
            description,
        } => {
            let v = client
                .exercise_log(type_id, mins, kcal, description.as_deref())
                .await?;
            emit(format, "exercise logged", "OK", &v)
        }
    }
}

pub async fn run_water(client: &FsClient, format: OutputFormat, args: WaterArgs) -> Result<()> {
    let today = today_days();
    match args.action {
        WaterAction::Day => {
            let v = client.water_get(today).await?;
            let (human, plain) = render_water(&v, "no water data (see --format json)");
            emit(format, human.trim_end(), plain.trim_end(), &v)
        }
        WaterAction::Log { ml, goal_ml } => {
            let v = client.water_log(ml, goal_ml, today).await?;
            emit(format, &format!("water logged: {ml} ml"), "OK", &v)
        }
    }
}
