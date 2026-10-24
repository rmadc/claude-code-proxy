use crate::{config, paths};
use serde_json::Value;
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_LOG_BYTES: u64 = 20 * 1024 * 1024;

static STDERR_SUPPRESSION_DEPTH: AtomicUsize = AtomicUsize::new(0);

pub const REDACT_KEYS: [&str; 20] = [
    "accesstoken",
    "refreshtoken",
    "client_token",
    "clienttoken",
    "ccp_client_token",
    "authorization",
    "proxy-authorization",
    "access",
    "access_token",
    "refresh",
    "refresh_token",
    "id_token",
    "code",
    "code_verifier",
    "chatgpt-account-id",
    "cookie",
    "set-cookie",
    "x-api-key",
    "apikey",
    "api_key",
];

pub fn log_file() -> std::path::PathBuf {
    paths::log_file()
}

#[must_use]
pub struct StderrSuppressionGuard;

impl Drop for StderrSuppressionGuard {
    fn drop(&mut self) {
        STDERR_SUPPRESSION_DEPTH.fetch_sub(1, Ordering::Relaxed);
    }
}

pub fn suppress_stderr() -> StderrSuppressionGuard {
    STDERR_SUPPRESSION_DEPTH.fetch_add(1, Ordering::Relaxed);
    StderrSuppressionGuard
}

fn stderr_suppressed() -> bool {
    STDERR_SUPPRESSION_DEPTH.load(Ordering::Relaxed) > 0
}

fn should_mirror_to_stderr(level: &str) -> bool {
    !stderr_suppressed() && (matches!(level, "warn" | "error") || config::log_stderr())
}

#[derive(Clone)]
pub struct Logger {
    service: String,
    base: serde_json::Map<String, Value>,
}

impl Logger {
    pub fn child(&self, bindings: serde_json::Map<String, Value>) -> Logger {
        let mut merged = self.base.clone();
        merged.extend(bindings);
        Logger {
            service: self.service.clone(),
            base: merged,
        }
    }

    pub fn debug(&self, msg: &str, fields: Option<serde_json::Map<String, Value>>) {
        self.emit("debug", msg, fields)
    }

    pub fn info(&self, msg: &str, fields: Option<serde_json::Map<String, Value>>) {
        self.emit("info", msg, fields)
    }

    pub fn warn(&self, msg: &str, fields: Option<serde_json::Map<String, Value>>) {
        self.emit("warn", msg, fields)
    }

    pub fn error(&self, msg: &str, fields: Option<serde_json::Map<String, Value>>) {
        self.emit("error", msg, fields)
    }

    fn emit(&self, level: &str, msg: &str, fields: Option<serde_json::Map<String, Value>>) {
        let mut body = serde_json::Map::new();
        body.insert("t".into(), Value::String(now_iso8601()));
        body.insert("level".into(), Value::String(level.to_string()));
        body.insert("service".into(), Value::String(self.service.clone()));
        body.insert("msg".into(), Value::String(msg.to_string()));

        let mut merged = self.base.clone();
        if let Some(fields) = fields {
            merged.extend(fields);
        }
        // Upstream error prose and request bodies can echo arbitrary secrets.
        // Known-key redaction alone cannot make these safe for routine logs.
        omit_content(
            &mut merged,
            std::env::var("CCP_LOG_CONTENT").as_deref() == Ok("1"),
        );
        if !merged.is_empty() {
            body.insert("fields".into(), redact_value(Value::Object(merged)));
        }

        let line = Value::Object(body).to_string();

        let mirror_to_stderr = should_mirror_to_stderr(level);
        if mirror_to_stderr {
            let _ = writeln!(io::stderr(), "{line}");
        }

        if write_log_line(&line).is_err() && mirror_to_stderr {
            // swallow logging errors intentionally
        }
    }
}

fn omit_content(fields: &mut serde_json::Map<String, Value>, include_content: bool) {
    if include_content {
        return;
    }
    for key in [
        "message",
        "error",
        "body",
        "request",
        "response",
        "responseBody",
        "requestBody",
        "bodyReadError",
    ] {
        if fields.contains_key(key) {
            fields.insert(key.into(), Value::String("[content omitted]".into()));
        }
    }
}

pub fn create_logger(service: &str) -> Logger {
    Logger {
        service: service.to_string(),
        base: serde_json::Map::new(),
    }
}

fn write_log_line(line: &str) -> io::Result<()> {
    let file = log_file();
    if let Some(dir) = file.parent() {
        create_dir(dir, 0o700)?;
    }

    if fs::metadata(&file).is_ok_and(|meta| meta.len() > MAX_LOG_BYTES) {
        rotate_file(&file)?;
    }

    let mut out = OpenOptions::new().create(true).append(true).open(&file)?;
    set_mode(&file, 0o600);
    out.write_all(line.as_bytes())?;
    out.write_all(b"\n")?;
    Ok(())
}

fn rotate_file(path: &Path) -> io::Result<()> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let rotated = path.with_extension(format!("{ts}"));
    fs::rename(path, rotated)?;
    if let Some(dir) = path.parent() {
        prune_files(dir, "proxy.", "", 3);
    }
    Ok(())
}

/// Bound our own regular capture files; never follow directory symlinks.
pub(crate) fn prune_files(dir: &Path, prefix: &str, extension: &str, keep: usize) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if prefix == "proxy."
                && !name.strip_prefix(prefix).is_some_and(|suffix| {
                    !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit())
                })
            {
                return None;
            }
            if !entry.file_type().ok()?.is_file()
                || !name.starts_with(prefix)
                || (!extension.is_empty() && entry.path().extension()?.to_str()? != extension)
                || name == "proxy.log"
            {
                return None;
            }
            Some((entry.metadata().ok()?.modified().ok()?, entry.path()))
        })
        .collect();
    files.sort_by_key(|(modified, _)| *modified);
    let remove = files.len().saturating_sub(keep);
    for (_, path) in files.into_iter().take(remove) {
        let _ = fs::remove_file(path);
    }
}

fn create_dir(path: &Path, mode: u32) -> io::Result<()> {
    fs::create_dir_all(path)?;
    set_mode(path, mode);
    Ok(())
}

fn set_mode(path: &Path, mode: u32) {
    #[cfg(not(unix))]
    let _ = (path, mode);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(path) {
            let mut perm = meta.permissions();
            perm.set_mode(mode);
            let _ = fs::set_permissions(path, perm);
        }
    }
}

fn now_iso8601() -> String {
    let now = time::OffsetDateTime::now_utc();
    let format = time::format_description::parse_borrowed::<3>(
        "[year]-[month]-[day]T[hour]:[minute]:[second]Z",
    )
    .unwrap();
    now.format(&format).unwrap_or_else(|_| String::new())
}

pub fn redact_value(value: Value) -> Value {
    redact_with_depth(value, 0)
}

fn redact_with_depth(value: Value, depth: u8) -> Value {
    if depth > 6 {
        return Value::String("[depth-limit]".into());
    }

    match value {
        Value::String(s) => {
            if config::log_verbose() {
                Value::String(s)
            } else if s.len() > 4000 {
                let mut end = 4000;
                while !s.is_char_boundary(end) {
                    end -= 1;
                }
                Value::String(format!("{}…[{} more]", &s[..end], s.len() - end))
            } else {
                Value::String(s)
            }
        }
        Value::Array(values) => Value::Array(
            values
                .into_iter()
                .map(|v| redact_with_depth(v, depth + 1))
                .collect(),
        ),
        Value::Object(fields) => {
            let mut out = serde_json::Map::new();
            for (key, value) in fields {
                if REDACT_KEYS.contains(&key.to_lowercase().as_str()) {
                    out.insert(key, redact_key_redaction(value));
                } else {
                    out.insert(key, redact_with_depth(value, depth + 1));
                }
            }
            Value::Object(out)
        }
        value => value,
    }
}

fn redact_key_redaction(value: Value) -> Value {
    match value {
        Value::String(s) => Value::String(format!("[redacted len={}]", s.len())),
        _ => Value::String("[redacted]".to_string()),
    }
}

pub fn redacted_keys() -> HashSet<&'static str> {
    REDACT_KEYS.iter().copied().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static STDERR_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn sensitive_error_prose_is_opt_in_and_retention_is_bounded() {
        let mut fields = serde_json::Map::from_iter([
            ("status".into(), serde_json::json!(401)),
            (
                "message".into(),
                serde_json::json!("echoed synthetic credential"),
            ),
            ("error".into(), serde_json::json!("echoed synthetic prompt")),
        ]);
        omit_content(&mut fields, false);
        assert!(
            !serde_json::to_string(&fields)
                .unwrap()
                .contains("synthetic")
        );
        assert_eq!(fields["status"], 401);
        let temp = tempfile::tempdir().unwrap();
        for i in 0..8 {
            fs::write(temp.path().join(format!("proxy.{i}")), "test").unwrap();
        }
        fs::write(temp.path().join("proxy.log"), "active").unwrap();
        fs::write(temp.path().join("unrelated"), "keep").unwrap();
        fs::write(temp.path().join("proxy.config"), "keep").unwrap();
        prune_files(temp.path(), "proxy.", "", 3);
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 6);
        assert!(temp.path().join("proxy.log").is_file());
        assert!(temp.path().join("unrelated").is_file());
        assert!(temp.path().join("proxy.config").is_file());
    }

    #[test]
    fn camel_case_tokens_and_unicode_are_safe() {
        let value = redact_value(
            serde_json::json!({"accessToken":"synthetic", "refreshToken":"synthetic", "text":"a".repeat(3999) + "🦀🦀"}),
        );
        assert!(!value["accessToken"].as_str().unwrap().contains("synthetic"));
        assert!(
            !value["refreshToken"]
                .as_str()
                .unwrap()
                .contains("synthetic")
        );
        assert!(value["text"].as_str().unwrap().is_char_boundary(3999));
    }

    #[test]
    fn stderr_suppression_disables_level_mirroring() {
        let _lock = STDERR_TEST_LOCK.lock().unwrap();
        assert!(should_mirror_to_stderr("warn"));

        {
            let _guard = suppress_stderr();
            assert!(!should_mirror_to_stderr("warn"));
            assert!(!should_mirror_to_stderr("error"));
        }

        assert!(should_mirror_to_stderr("warn"));
    }

    #[test]
    fn stderr_suppression_supports_nested_guards() {
        let _lock = STDERR_TEST_LOCK.lock().unwrap();
        let outer = suppress_stderr();
        let inner = suppress_stderr();
        assert!(!should_mirror_to_stderr("warn"));

        drop(inner);
        assert!(!should_mirror_to_stderr("warn"));

        drop(outer);
        assert!(should_mirror_to_stderr("warn"));
    }

    #[test]
    fn redacts_proxy_authorization_case_insensitively() {
        let redacted = redact_value(serde_json::json!({
            "Proxy-Authorization": "Basic dXNlcjpwYXNz",
            "safe": "kept"
        }));

        assert_eq!(redacted["safe"], "kept");
        assert_eq!(redacted["Proxy-Authorization"], "[redacted len=18]");
    }
}
