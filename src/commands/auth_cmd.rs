use crate::api::register_body;
use crate::auth::device::{ensure_device_id, today_ymd};
use crate::auth::{FileStore, login, prompt_password};
use crate::cli::{AuthAction, AuthArgs};
use crate::commands::{app_client, http_client};
use crate::config::AppConfig;
use crate::error::{AppError, Result};

pub async fn run(app: &AppConfig, args: AuthArgs) -> Result<()> {
    let store = FileStore::platform()?;
    match args.action {
        AuthAction::Login { username } => {
            let http = http_client();
            // Per-install identity first: explicit config, stored fid,
            // otherwise mint + persist (network, once per install).
            let device_id = ensure_device_id(&http, app.device_id.as_deref()).await?;
            let password = prompt_password()?;
            let triple = login(
                &http,
                &app.auth_url,
                username.trim(),
                &password,
                &app.device_model,
                &app.app_version,
                Some(&device_id),
            )
            .await?;
            store.save(&triple)?;
            println!("logged in as {}", triple.username);
            Ok(())
        }
        AuthAction::Register {
            email,
            username,
            birth_date,
            gender,
            country,
            current_weight_kg,
            goal_weight_kg,
            height_cm,
            first_name,
        } => {
            let http = http_client();
            let device_id = ensure_device_id(&http, app.device_id.as_deref()).await?;
            let password = prompt_password()?;
            let body = register_body(
                email.trim(),
                username.trim(),
                &password,
                &gender,
                &birth_date,
                0,
                current_weight_kg,
                goal_weight_kg,
                0,
                height_cm,
                &today_ymd(),
                &country,
                None,
                None,
                None,
                &app.device_model,
                first_name.as_deref(),
            );
            let client = app_client(app)?;
            client.register(&body, Some(&device_id)).await?;
            println!("registered; check {email} for confirmation");
            Ok(())
        }
        AuthAction::ForgotPassword { email } => {
            let http = http_client();
            let device_id = ensure_device_id(&http, app.device_id.as_deref()).await?;
            let client = app_client(app)?;
            client
                .forgot_password(email.trim(), Some(&device_id))
                .await?;
            println!("reset email sent to {email}");
            Ok(())
        }
        AuthAction::ResetPassword => {
            let code = rpassword::prompt_password("Reset code: ").map_err(AppError::Io)?;
            let password = prompt_password()?;
            let http = http_client();
            let device_id = ensure_device_id(&http, app.device_id.as_deref()).await?;
            let client = app_client(app)?;
            client
                .reset_password(code.trim(), &password, Some(&device_id))
                .await?;
            println!("password reset; log in with the new password");
            Ok(())
        }
        AuthAction::Status => match store.load() {
            Ok(t) if t.username.is_empty() => {
                println!("logged in");
                Ok(())
            }
            Ok(t) => {
                println!("logged in as {}", t.username);
                Ok(())
            }
            Err(e) => {
                println!("logged out");
                Err(e)
            }
        },
        AuthAction::Logout => {
            store.delete()?;
            println!("logged out");
            Ok(())
        }
    }
}
