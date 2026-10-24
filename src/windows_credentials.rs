//! User-scoped DPAPI. No UI, machine scope, subprocesses, or plaintext fallback.
use anyhow::{Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{fs, path::Path, ptr};

static MIGRATION_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[repr(C)]
struct Blob {
    len: u32,
    data: *mut u8,
}

#[link(name = "crypt32")]
unsafe extern "system" {
    fn CryptProtectData(
        input: *const Blob,
        description: *const u16,
        entropy: *const Blob,
        reserved: *mut core::ffi::c_void,
        prompt: *mut core::ffi::c_void,
        flags: u32,
        output: *mut Blob,
    ) -> i32;
    fn CryptUnprotectData(
        input: *const Blob,
        description: *mut *mut u16,
        entropy: *const Blob,
        reserved: *mut core::ffi::c_void,
        prompt: *mut core::ffi::c_void,
        flags: u32,
        output: *mut Blob,
    ) -> i32;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn LocalFree(memory: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
}

fn transform(bytes: &[u8], protect: bool) -> Result<Vec<u8>> {
    let input = Blob {
        len: bytes.len().try_into()?,
        data: bytes.as_ptr().cast_mut(),
    };
    let mut output = Blob {
        len: 0,
        data: ptr::null_mut(),
    };
    // SAFETY: input lives through the call; output is owned by DPAPI and freed
    // with LocalFree after copying. CRYPTPROTECT_UI_FORBIDDEN = 1.
    unsafe {
        let ok = if protect {
            CryptProtectData(
                &input,
                ptr::null(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
                1,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                ptr::null_mut(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
                1,
                &mut output,
            )
        };
        if ok == 0 {
            bail!(
                "Windows credential protection failed: {}",
                std::io::Error::last_os_error()
            );
        }
        let result = std::slice::from_raw_parts(output.data, output.len as usize).to_vec();
        LocalFree(output.data.cast());
        Ok(result)
    }
}

pub fn protected_path(path: &str) -> String {
    format!("{path}.dpapi")
}

pub fn read(path: &str) -> Result<Option<Value>> {
    let bytes = match fs::read(protected_path(path)) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    let envelope: Value = serde_json::from_slice(&bytes)?;
    let encoded = envelope
        .get("dpapi_v1")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("Invalid Windows credential envelope"))?;
    let plaintext = transform(&STANDARD.decode(encoded)?, false)?;
    Ok(Some(serde_json::from_slice(&plaintext)?))
}

pub fn save<T: serde::Serialize>(path: &str, value: &T) -> Result<()> {
    let plaintext = serde_json::to_vec(value)?;
    let encrypted = transform(&plaintext, true)?;
    if transform(&encrypted, false)? != plaintext {
        bail!("Credential protection verification failed");
    }
    crate::auth::write_atomically(
        &protected_path(path),
        &json!({"dpapi_v1": STANDARD.encode(encrypted)}),
    )?;
    // Read back the durable copy before allowing any plaintext removal.
    if read(path)? != Some(serde_json::from_slice(&plaintext)?) {
        bail!("Stored credential verification failed");
    }
    Ok(())
}

pub fn load<T: serde::de::DeserializeOwned + serde::Serialize>(
    primary: &str,
    legacy: &str,
) -> Result<Option<T>> {
    let _migration = MIGRATION_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("Windows credential migration lock unavailable"))?;
    if let Some(value) = read(primary)? {
        return Ok(Some(serde_json::from_value(value)?));
    }
    for source in [primary, legacy] {
        if source != primary
            && let Some(value) = read(source)?
        {
            return Ok(Some(serde_json::from_value(value)?));
        }
        let raw = match fs::read(source) {
            Ok(raw) => raw,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(err.into()),
        };
        let value: T = serde_json::from_slice(&raw)?;
        save(primary, &value)?;
        // Failure leaves both copies intact and is reported; never fall back to
        // plaintext after an unreadable protected copy.
        if let Err(err) = fs::remove_file(Path::new(source))
            && err.kind() != std::io::ErrorKind::NotFound
        {
            return Err(err.into());
        }
        return Ok(Some(value));
    }
    // Another process may have retired plaintext after our first protected read.
    read(primary)?
        .map(serde_json::from_value)
        .transpose()
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn migrates_only_after_verified_protected_write() {
        let temp = tempdir().unwrap();
        let legacy = temp.path().join("legacy.json").to_str().unwrap().to_owned();
        let primary = temp.path().join("auth.json").to_str().unwrap().to_owned();
        let value = json!({"access":"synthetic-access", "refresh":"synthetic-refresh"});
        fs::write(&legacy, value.to_string()).unwrap();
        assert_eq!(
            load::<Value>(&primary, &legacy).unwrap(),
            Some(value.clone())
        );
        assert!(!Path::new(&legacy).exists());
        assert_eq!(read(&primary).unwrap(), Some(value));
        assert!(
            !fs::read_to_string(protected_path(&primary))
                .unwrap()
                .contains("synthetic-access")
        );
        let rotated = json!({"access":"synthetic-rotated", "refresh":"synthetic-rotated-refresh"});
        save(&primary, &rotated).unwrap();
        assert_eq!(load::<Value>(&primary, &legacy).unwrap(), Some(rotated));
    }

    #[test]
    fn failed_write_preserves_plaintext_and_corruption_fails_closed() {
        let temp = tempdir().unwrap();
        let legacy = temp.path().join("legacy.json").to_str().unwrap().to_owned();
        fs::write(&legacy, r#"{"access":"synthetic"}"#).unwrap();
        let primary = temp
            .path()
            .join("missing/auth.json")
            .to_str()
            .unwrap()
            .to_owned();
        fs::write(temp.path().join("missing"), "block directory creation").unwrap();
        assert!(save(&primary, &json!({"access":"synthetic"})).is_err());
        assert!(load::<Value>(&primary, &legacy).is_err());
        assert!(Path::new(&legacy).exists());
        let primary = temp.path().join("auth.json").to_str().unwrap().to_owned();
        fs::write(protected_path(&primary), r#"{"dpapi_v1":"AAAA"}"#).unwrap();
        assert!(load::<Value>(&primary, &legacy).is_err());
        assert!(Path::new(&legacy).exists());
    }

    #[test]
    fn concurrent_first_loads_all_receive_verified_credentials() {
        let temp = tempdir().unwrap();
        let primary = temp.path().join("auth.json").to_str().unwrap().to_owned();
        fs::write(&primary, r#"{"access":"synthetic-concurrent"}"#).unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(12));
        std::thread::scope(|scope| {
            for _ in 0..12 {
                let barrier = barrier.clone();
                let primary = &primary;
                scope.spawn(move || {
                    barrier.wait();
                    let value = load::<Value>(primary, primary).unwrap().unwrap();
                    assert_eq!(value["access"], "synthetic-concurrent");
                });
            }
        });
        assert!(!Path::new(&primary).exists());
        assert!(read(&primary).unwrap().is_some());
    }
}
