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
    tauri_build::build()
}
