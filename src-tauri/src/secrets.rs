use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

fn secrets_path() -> Result<PathBuf, String> {
    crate::config::app_config_dir().map(|d| d.join("secrets.json"))
}

fn use_file_store() -> bool {
    std::env::var("AGENT_IDE_SECRET_STORE")
        .map(|v| v == "file")
        .unwrap_or(false)
}

fn read_file_secrets() -> Result<HashMap<String, String>, String> {
    let path = secrets_path()?;
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read secrets file: {}", e))?;
    if content.trim().is_empty() {
        return Ok(HashMap::new());
    }
    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse secrets file: {}", e))
}

fn write_file_secrets(secrets: &HashMap<String, String>) -> Result<(), String> {
    let path = secrets_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config directory: {}", e))?;
    }
    let json = serde_json::to_string_pretty(secrets)
        .map_err(|e| format!("Failed to serialize secrets: {}", e))?;
    let mut file = std::fs::File::create(&path)
        .map_err(|e| format!("Failed to create secrets file: {}", e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = file
            .metadata()
            .map_err(|e| format!("Failed to read file metadata: {}", e))?
            .permissions();
        perms.set_mode(0o600);
        file.set_permissions(perms)
            .map_err(|e| format!("Failed to set secrets file permissions: {}", e))?;
    }

    file.write_all(json.as_bytes())
        .map_err(|e| format!("Failed to write secrets file: {}", e))?;
    file.flush()
        .map_err(|e| format!("Failed to flush secrets file: {}", e))?;
    Ok(())
}

pub fn get_secret(key: &str) -> Result<Option<String>, String> {
    if !use_file_store() {
        let entry = keyring::Entry::new("agent-ide", key)
            .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
        match entry.get_password() {
            Ok(value) => return Ok(Some(value)),
            Err(keyring::Error::NoEntry) => return Ok(None),
            Err(e) if !use_file_store() => {
                tracing::warn!("keyring get failed for {}: {}; falling back to file", key, e);
            }
            _ => {}
        }
    }
    let secrets = read_file_secrets()?;
    Ok(secrets.get(key).cloned())
}

pub fn set_secret(key: &str, value: &str) -> Result<(), String> {
    if !use_file_store() {
        let entry = keyring::Entry::new("agent-ide", key)
            .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
        match entry.set_password(value) {
            Ok(()) => return Ok(()),
            Err(e) => {
                tracing::warn!("keyring set failed for {}: {}; falling back to file", key, e);
            }
        }
    }
    let mut secrets = read_file_secrets()?;
    secrets.insert(key.to_string(), value.to_string());
    write_file_secrets(&secrets)
}

pub fn delete_secret(key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new("agent-ide", key)
        .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
    let keyring_deleted = match entry.delete_credential() {
        Ok(()) => true,
        Err(keyring::Error::NoEntry) => true,
        Err(e) => {
            tracing::warn!("keyring delete failed for {}: {}; updating file", key, e);
            false
        }
    };
    if keyring_deleted && !use_file_store() {
        return Ok(());
    }
    let mut secrets = read_file_secrets()?;
    secrets.remove(key);
    write_file_secrets(&secrets)
}
