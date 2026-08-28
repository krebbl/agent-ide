use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rerun-if-changed=objc/notification.m");
        println!("cargo:rerun-if-changed=objc/notification.h");
        cc::Build::new()
            .file("objc/notification.m")
            .flag("-fmodules")
            .flag("-fobjc-arc")
            .compile("agent_ide_notification");
        println!("cargo:rustc-link-lib=framework=UserNotifications");
    }
    // Refresh AGENT_IDE_DAEMON_TOKEN whenever Rust code changes, so the
    // running app replaces stale PTY daemons built from older sources.
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=Cargo.toml");
    let token = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string();
    println!("cargo:rustc-env=AGENT_IDE_DAEMON_TOKEN={}", token);
    tauri_build::build()
}
