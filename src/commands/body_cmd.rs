use crate::api::FsClient;
use crate::auth::device::today_days;
use crate::cli::{
    ExerciseAction, ExerciseArgs, OutputFormat, RdiAction, RdiArgs, WaterAction, WaterArgs,
    WeightAction, WeightArgs,
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

pub async fn run_rdi(client: &FsClient, format: OutputFormat, args: RdiArgs) -> Result<()> {
    match args.action {
        RdiAction::Show => {
            let v = client.rdi_show().await?;
            let (human, plain) = crate::output::render_scalars(&v, "rdi (see --format json)");
            emit(format, human.trim_end(), plain.trim_end(), &v)
        }
        RdiAction::Save {
            age,
            weight_kg,
            height_cm,
            sex,
            goal,
            activity,
            rdi,
        } => {
            let v = client
                .rdi_save(
                    age,
                    weight_kg,
                    height_cm,
                    rdi_sex(&sex)?,
                    rdi_goal(&goal)?,
                    rdi_activity(&activity)?,
                    rdi,
                )
                .await?;
            emit(format, "rdi saved", "OK", &v)
        }
    }
}

/// Server ordinals from `Sex` (Female=0, Male=1).
pub(crate) fn rdi_sex(v: &str) -> Result<i64> {
    match v.to_lowercase().as_str() {
        "female" => Ok(0),
        "male" => Ok(1),
        _ => Err(crate::error::AppError::Msg(format!(
            "bad --sex {v:?}, want female|male"
        ))),
    }
}

/// Server ordinals from `RDIGoal` (Steady=3, GainOnePoundAWeek=1, ...).
pub(crate) fn rdi_goal(v: &str) -> Result<i64> {
    match v.to_lowercase().as_str() {
        "maintain" => Ok(3),
        "gain" => Ok(1),
        "gain-slow" => Ok(2),
        "lose-slow" => Ok(4),
        "lose" => Ok(5),
        _ => Err(crate::error::AppError::Msg(format!(
            "bad --goal {v:?}, want maintain|gain|gain-slow|lose-slow|lose"
        ))),
    }
}

/// Server ordinals from `RDIActivityLevel` (Sedentary=1 ... High=4).
pub(crate) fn rdi_activity(v: &str) -> Result<i64> {
    match v.to_lowercase().as_str() {
        "sedentary" => Ok(1),
        "low" => Ok(2),
        "active" => Ok(3),
        "high" => Ok(4),
        _ => Err(crate::error::AppError::Msg(format!(
            "bad --activity {v:?}, want sedentary|low|active|high"
        ))),
    }
}

pub async fn run_exercise(
    client: &FsClient,
    format: OutputFormat,
    args: ExerciseArgs,
) -> Result<()> {
    match args.action {
        ExerciseAction::Day { date } => {
            let (_, recorded) = crate::commands::diary_cmd::recorded_date(&date)?;
            let v = client.exercise_day(recorded).await?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_rdi_ordinals() {
        assert_eq!(rdi_sex("female").unwrap(), 0);
        assert_eq!(rdi_sex("Male").unwrap(), 1);
        assert!(rdi_sex("x").is_err());
        assert_eq!(rdi_goal("maintain").unwrap(), 3);
        assert_eq!(rdi_goal("gain").unwrap(), 1);
        assert_eq!(rdi_goal("gain-slow").unwrap(), 2);
        assert_eq!(rdi_goal("lose-slow").unwrap(), 4);
        assert_eq!(rdi_goal("lose").unwrap(), 5);
        assert!(rdi_goal("bulk").is_err());
        assert_eq!(rdi_activity("sedentary").unwrap(), 1);
        assert_eq!(rdi_activity("low").unwrap(), 2);
        assert_eq!(rdi_activity("active").unwrap(), 3);
        assert_eq!(rdi_activity("high").unwrap(), 4);
        assert!(rdi_activity("extreme").is_err());
    }
}
