use crate::cli::{ConfigAction, ConfigArgs, OutputFormat};
use crate::config::{AppConfig, redacted, save_kv};
use crate::error::Result;
use crate::output::emit;

pub async fn run(app: &AppConfig, format: OutputFormat, args: ConfigArgs) -> Result<()> {
    match &args.action {
        ConfigAction::Show => {
            let v = redacted(app);
            let lines: Vec<String> = v
                .as_object()
                .map(|m| m.iter().map(|(k, val)| format!("{k}: {val}")).collect())
                .unwrap_or_default();
            let text = lines.join("\n");
            let plain = text.replace(": ", "\t");
            emit(format, &text, &plain, &v)
        }
        ConfigAction::Set { key, value } => {
            let path = save_kv(None, key, Some(value))?;
            emit(
                format,
                &format!("set {key} ({})", path.display()),
                "OK",
                &serde_json::json!({ "key": key, "path": path }),
            )
        }
        ConfigAction::Unset { key } => {
            let path = save_kv(None, key, None)?;
            emit(
                format,
                &format!("unset {key} ({})", path.display()),
                "OK",
                &serde_json::json!({ "key": key, "path": path }),
            )
        }
        ConfigAction::Path => {
            let path = crate::config::default_config_path()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "<unavailable>".to_string());
            emit(format, &path, &path, &serde_json::json!({ "path": path }))
        }
    }
}
