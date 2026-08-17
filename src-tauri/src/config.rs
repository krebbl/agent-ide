use std::path::PathBuf;

pub fn app_config_dir() -> Result<PathBuf, String> {
    std::env::var("AGENT_IDE_CONFIG_DIR")
        .map(PathBuf::from)
        .ok()
        .or_else(|| dirs::config_dir().map(|d| d.join("agent-ide")))
        .ok_or_else(|| "Could not determine config directory".to_string())
}
