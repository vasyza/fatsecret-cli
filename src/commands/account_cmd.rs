use crate::cli::{AccountAction, AccountArgs};
use crate::commands::app_client;
use crate::config::AppConfig;
use crate::error::Result;
use crate::output::{emit, render_account_settings, render_account_show};

pub async fn run(
    app: &AppConfig,
    args: AccountArgs,
    format: crate::cli::OutputFormat,
) -> Result<()> {
    let client = app_client(app)?;
    match args.action {
        AccountAction::Show => {
            let v = client.user_details().await?;
            let (human, plain) = render_account_show(&v);
            emit(format, human.trim_end(), plain.trim_end(), &v)
        }
        AccountAction::Settings => {
            let v = client.account_settings().await?;
            let (human, plain) = render_account_settings(&v);
            emit(format, human.trim_end(), plain.trim_end(), &v)
        }
        AccountAction::ChangeUsername { name } => {
            let v = client.change_username(name.trim()).await?;
            emit(format, "username changed", "OK", &v)
        }
    }
}
