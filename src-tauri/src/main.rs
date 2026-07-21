// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--pty-daemon") {
        if let Err(e) = agent_ide_lib::run_pty_daemon() {
            eprintln!("pty daemon failed: {}", e);
            std::process::exit(1);
        }
    } else {
        agent_ide_lib::run();
    }
}
