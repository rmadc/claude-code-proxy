use claude_code_proxy::paths::{self, DirResolverEnv};
use std::collections::HashMap;

#[test]
fn native_os_names_and_existing_configuration_are_preserved() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().to_string_lossy().to_string();
    let appdata = temp.path().join("roaming");
    let local = temp.path().join("local");
    let mut deps = DirResolverEnv {
        platform: "windows".into(),
        home: home.clone(),
        env: HashMap::from([
            ("APPDATA".into(), appdata.to_string_lossy().to_string()),
            ("LOCALAPPDATA".into(), local.to_string_lossy().to_string()),
        ]),
    };
    assert_eq!(
        paths::resolve_config_dir(&deps),
        appdata.join("claude-code-proxy")
    );
    assert_eq!(
        paths::resolve_state_dir(&deps),
        local.join("claude-code-proxy")
    );
    let old = temp.path().join(".config/claude-code-proxy");
    std::fs::create_dir_all(&old).unwrap();
    std::fs::write(old.join("config.json"), r#"{"port":12345}"#).unwrap();
    assert_eq!(paths::resolve_config_dir(&deps), old);
    let xdg = temp.path().join("xdg");
    std::fs::create_dir_all(xdg.join("claude-code-proxy")).unwrap();
    deps.env
        .insert("XDG_CONFIG_HOME".into(), xdg.to_string_lossy().to_string());
    assert_eq!(
        paths::resolve_config_dir(&deps),
        xdg.join("claude-code-proxy")
    );
    let explicit = temp.path().join("explicit");
    deps.env.insert(
        "CCP_CONFIG_DIR".into(),
        explicit.to_string_lossy().to_string(),
    );
    assert_eq!(paths::resolve_config_dir(&deps), explicit);
    deps.platform = "macos".into();
    deps.env.remove("CCP_CONFIG_DIR");
    assert_eq!(
        paths::resolve_config_dir(&deps),
        xdg.join("claude-code-proxy")
    );
    deps.env.clear();
    assert_eq!(paths::resolve_config_dir(&deps), old);
}
