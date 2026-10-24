use crate::providers::codex::catalog::ModelMetadata;
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::io::{self, IsTerminal, Write};

pub struct LaunchProfile {
    pub settings: Value,
    pub context_window: u64,
    pub auto_compact_tokens: u64,
    pub effort: Option<String>,
    pub capability_env: Vec<(String, String)>,
}

pub fn build_profile(
    models: &[ModelMetadata],
    selected: &str,
    effort: Option<&str>,
    only_model: bool,
) -> Result<LaunchProfile> {
    let selected = models.iter().find(|m| m.id == selected).ok_or_else(|| {
        anyhow::anyhow!("Selected model is not present in the authenticated compatible catalog")
    })?;
    if let Some(effort) = effort {
        ensure!(
            selected.supported_efforts.iter().any(|e| e == effort),
            "Selected model does not advertise the requested effort"
        );
    }
    let choices: Vec<_> = models
        .iter()
        .filter(|m| !only_model || m.id == selected.id)
        .collect();
    // Claude's context override is process-wide, not a per-picker-row setting.
    // A shared picker must remain safe when switching to any listed model.
    let context_window = choices.iter().map(|m| m.context_window).min().unwrap();
    let auto_compact_tokens = choices
        .iter()
        .map(|m| m.auto_compact_tokens)
        .min()
        .unwrap()
        .min(context_window);
    let options: Vec<_> = choices.iter().map(|m| json!({
        "model":m.id, "label":m.id,
        "description":format!("{}k context; default effort {}; supported: {}", m.context_window / 1000, m.default_effort.as_deref().unwrap_or("backend default"), m.supported_efforts.join(", "))
    })).collect();
    let mut capability_env = Vec::new();
    // Native Claude capability declarations have five named slots. Keep the
    // initial model in a slot and fill the remaining slots without fake IDs.
    let mut pinned = vec![selected];
    pinned.extend(choices.iter().copied().filter(|m| m.id != selected.id));
    for (slot, model) in ["OPUS", "SONNET", "HAIKU", "FABLE", "CUSTOM"]
        .iter()
        .zip(pinned)
    {
        let prefix = if *slot == "CUSTOM" {
            "ANTHROPIC_CUSTOM_MODEL_OPTION".to_owned()
        } else {
            format!("ANTHROPIC_DEFAULT_{slot}_MODEL")
        };
        let mut capabilities = Vec::new();
        if !model.supported_efforts.is_empty() {
            capabilities.push("effort");
        }
        if model.supported_efforts.iter().any(|e| e == "xhigh") {
            capabilities.push("xhigh_effort");
        }
        if model.supported_efforts.iter().any(|e| e == "max") {
            capabilities.push("max_effort");
        }
        capability_env.push((prefix.clone(), model.id.clone()));
        capability_env.push((format!("{prefix}_NAME"), model.id.clone()));
        capability_env.push((
            format!("{prefix}_SUPPORTED_CAPABILITIES"),
            capabilities.join(","),
        ));
    }
    Ok(LaunchProfile {
        settings: json!({"modelPicker":{"replaceBuiltInOptions":true,"options":options}, "availableModels":choices.iter().map(|m| &m.id).collect::<Vec<_>>()}),
        context_window,
        auto_compact_tokens,
        effort: effort
            .map(str::to_owned)
            .or_else(|| selected.default_effort.clone()),
        capability_env,
    })
}

struct SettingsFile(std::path::PathBuf);
impl Drop for SettingsFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn choose_model(
    models: &[ModelMetadata],
    input: &mut impl io::BufRead,
    output: &mut impl Write,
) -> Result<String> {
    writeln!(
        output,
        "Choose a default Claude model from your Codex catalog:"
    )?;
    for (index, model) in models.iter().enumerate() {
        writeln!(
            output,
            "  {}. {} ({}k context)",
            index + 1,
            model.id,
            model.context_window / 1000
        )?;
    }
    loop {
        write!(output, "Model number (1-{}): ", models.len())?;
        output.flush()?;
        let mut response = String::new();
        ensure!(
            input.read_line(&mut response)? != 0,
            "Model selection cancelled"
        );
        if let Ok(index) = response.trim().parse::<usize>()
            && let Some(model) = index.checked_sub(1).and_then(|index| models.get(index))
        {
            return Ok(model.id.clone());
        }
        writeln!(output, "Enter a number from 1 to {}.", models.len())?;
    }
}

fn resolve_model(
    models: &[ModelMetadata],
    override_model: Option<String>,
    saved_model: Option<String>,
    interactive: bool,
    input: &mut impl io::BufRead,
    output: &mut impl Write,
) -> Result<(String, bool)> {
    if let Some(model) = override_model {
        ensure!(
            models.iter().any(|available| available.id == model),
            "Selected model {model} is not available in the authenticated catalog"
        );
        return Ok((model, false));
    }
    if let Some(model) = saved_model {
        if models.iter().any(|available| available.id == model) {
            return Ok((model, false));
        }
        ensure!(
            interactive,
            "Saved default model {model} is unavailable; run interactively to choose another or pass --model"
        );
        writeln!(
            output,
            "Saved default model {model} is no longer available."
        )?;
    }
    ensure!(
        interactive,
        "No default Claude model configured; run interactively to choose one or pass --model"
    );
    Ok((choose_model(models, input, output)?, true))
}

pub async fn launch(
    model: Option<String>,
    effort: Option<String>,
    only_model: bool,
    print_config: bool,
    executable: String,
    args: Vec<String>,
) -> Result<()> {
    ensure!(
        crate::config::codex_model().is_none() && crate::config::codex_effort().is_none(),
        "Claude-controlled selection requires removing CCP_CODEX_MODEL / CCP_CODEX_EFFORT and codex.model / codex.effort overrides"
    );
    let models = crate::providers::codex::catalog::fetch_catalog().await?;
    let saved = if model.is_none() {
        crate::config::launcher_default_model()?
    } else {
        None
    };
    let (model, save_default) = resolve_model(
        &models,
        model,
        saved,
        io::stdin().is_terminal() && io::stderr().is_terminal(),
        &mut io::stdin().lock(),
        &mut io::stderr().lock(),
    )?;
    let profile = build_profile(&models, &model, effort.as_deref(), only_model)?;
    ensure!(
        profile.context_window <= 1_000_000 && profile.auto_compact_tokens >= 100_000,
        "Catalog context cannot be represented safely by this Claude Code launcher"
    );
    if save_default {
        crate::config::save_launcher_default_model(&model)?;
        eprintln!("Saved default Claude model: {model}");
    }
    if print_config {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &json!({"model":model, "effort":profile.effort, "context_window":profile.context_window, "auto_compact_tokens":profile.auto_compact_tokens, "models":models, "settings":profile.settings})
            )?
        );
        return Ok(());
    }
    use base64::Engine;
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let token = base64::engine::general_purpose::STANDARD.encode(bytes);
    let auth = crate::client_auth::ClientAuth::new(Some(&token))?;
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    let url = format!("http://{}", listener.local_addr()?);
    let monitor = crate::monitor::MonitorHandle::default();
    let app = crate::server::app_for_launcher(
        std::sync::Arc::new(crate::Registry::with_default_alias()),
        Some(monitor),
        auth,
    );
    let (shutdown, stopped) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown(async {
            let _ = stopped.await;
        })
        .await
    });
    let file = SettingsFile(
        std::env::temp_dir().join(format!("ccp-claude-{}.json", uuid::Uuid::new_v4())),
    );
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    options
        .open(&file.0)?
        .write_all(serde_json::to_string(&profile.settings)?.as_bytes())?;
    let mut command = std::process::Command::new(executable);
    command
        .args(["--settings"])
        .arg(&file.0)
        .args(["--model", &model]);
    if let Some(effort) = &profile.effort {
        command.args(["--effort", effort]);
    }
    for (key, _) in std::env::vars_os() {
        let key_str = key.to_string_lossy();
        if key_str.starts_with("ANTHROPIC_")
            || matches!(
                key_str.as_ref(),
                "CLAUDE_CODE_MAX_CONTEXT_TOKENS"
                    | "CLAUDE_CODE_AUTO_COMPACT_WINDOW"
                    | "CLAUDE_CODE_EFFORT_LEVEL"
                    | "DISABLE_COMPACT"
                    | "CLAUDE_CODE_DISABLE_1M_CONTEXT"
                    | "CLAUDE_CODE_DISABLE_UNKNOWN_MODEL_WINDOW_ENFORCEMENT"
                    | "CLAUDE_CODE_USE_BEDROCK"
                    | "CLAUDE_CODE_USE_VERTEX"
                    | "CLAUDE_CODE_USE_FOUNDRY"
                    | "CLAUDE_CODE_USE_ANTHROPIC_AWS"
            )
        {
            command.env_remove(key);
        }
    }
    command
        .env("ANTHROPIC_BASE_URL", &url)
        .env("ANTHROPIC_AUTH_TOKEN", token)
        .env("ANTHROPIC_MODEL", &model)
        .env(
            "CLAUDE_CODE_MAX_CONTEXT_TOKENS",
            profile.context_window.to_string(),
        )
        .env(
            "CLAUDE_CODE_AUTO_COMPACT_WINDOW",
            profile.auto_compact_tokens.to_string(),
        )
        .env("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1");
    for (key, value) in &profile.capability_env {
        command.env(key, value);
    }
    command.args(args);
    eprintln!(
        "Proxy {url}; model {model}; effort {}; shared context {} (compact at {}).",
        profile.effort.as_deref().unwrap_or("backend default"),
        profile.context_window,
        profile.auto_compact_tokens
    );
    if !only_model {
        eprintln!(
            "/model lists {} compatible account models. Shared context uses the smallest listed window; --only-model uses the selected model's full default window.",
            models.len()
        );
    }
    let result = tokio::task::spawn_blocking(move || command.status()).await?;
    let _ = shutdown.send(());
    let _ = server.await;
    let status = result?;
    ensure!(status.success(), "Claude Code exited with {status}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn model(id: &str, window: u64) -> ModelMetadata {
        ModelMetadata {
            id: id.into(),
            context_window: window,
            max_context_window: Some(872000),
            default_effort: Some("medium".into()),
            supported_efforts: vec!["low".into(), "medium".into(), "high".into()],
            auto_compact_tokens: window * 95 / 100,
        }
    }
    #[test]
    fn model_selection_uses_override_then_saved_default_then_prompt() {
        let models = [model("gpt-6-sol", 272000), model("gpt-6.1-sol", 500000)];
        let mut input = io::Cursor::new("2\n1\n");
        let mut output = Vec::new();
        assert_eq!(
            resolve_model(
                &models,
                Some("gpt-6.1-sol".into()),
                Some("gpt-6-sol".into()),
                false,
                &mut input,
                &mut output
            )
            .unwrap(),
            ("gpt-6.1-sol".into(), false)
        );
        assert_eq!(
            resolve_model(
                &models,
                None,
                Some("gpt-6-sol".into()),
                false,
                &mut input,
                &mut output
            )
            .unwrap(),
            ("gpt-6-sol".into(), false)
        );
        assert_eq!(
            resolve_model(&models, None, None, true, &mut input, &mut output).unwrap(),
            ("gpt-6.1-sol".into(), true)
        );
        assert!(resolve_model(&models, None, None, false, &mut input, &mut output).is_err());
        assert_eq!(
            resolve_model(
                &models,
                None,
                Some("unavailable".into()),
                true,
                &mut input,
                &mut output
            )
            .unwrap(),
            ("gpt-6-sol".into(), true)
        );
        assert!(
            resolve_model(
                &models,
                None,
                Some("unavailable".into()),
                false,
                &mut input,
                &mut output
            )
            .is_err()
        );
    }

    #[test]
    fn model_prompt_retries_invalid_numbers_and_cancels_on_eof() {
        let models = [model("gpt-6-sol", 272000)];
        let mut input = io::Cursor::new("0\ninvalid\n1\n");
        let mut output = Vec::new();
        assert_eq!(
            choose_model(&models, &mut input, &mut output).unwrap(),
            "gpt-6-sol"
        );
        assert!(
            String::from_utf8(output)
                .unwrap()
                .contains("Enter a number from 1 to 1")
        );
        assert!(choose_model(&models, &mut io::Cursor::new(""), &mut Vec::new()).is_err());
    }

    #[test]
    fn switching_never_uses_a_larger_window_than_a_selectable_model() {
        let models = [model("gpt-6-sol", 272000), model("gpt-6.1-sol", 500000)];
        let p = build_profile(&models, "gpt-6.1-sol", Some("high"), false).unwrap();
        assert_eq!(p.context_window, 272000);
        assert_eq!(p.auto_compact_tokens, 258400);
        assert_eq!(
            p.settings["modelPicker"]["options"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(p.effort.as_deref(), Some("high"));
        let p = build_profile(&models, "gpt-6.1-sol", None, true).unwrap();
        assert_eq!(p.context_window, 500000);
        assert_eq!(p.auto_compact_tokens, 475000);
        assert!(build_profile(&models, "gpt-6-sol", Some("ultra"), false).is_err());
        assert!(build_profile(&models, "unlisted", None, false).is_err());
    }
}
