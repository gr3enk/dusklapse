pub mod camera;
mod commands;
mod session;

use tauri_plugin_log::{Target, TargetKind};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            // Without this, Rust-side logs are lost on iOS - there is no stdout
            // to attach to. The plugin routes them into the platform logger, so
            // they turn up in the Xcode console and in logcat.
            tauri_plugin_log::Builder::new()
                .target(Target::new(TargetKind::Stdout))
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .manage(session::CameraSession::default())
        .invoke_handler(tauri::generate_handler![
            commands::camera_connect,
            commands::camera_disconnect,
            commands::camera_status,
            commands::camera_capabilities,
            commands::camera_exposure,
            commands::camera_set_exposure,
            commands::camera_shoot,
            commands::camera_battery,
            commands::camera_preview,
            commands::camera_default_port,
        ])
        .run(tauri::generate_context!())
        .expect("error while running dusklapse");
}
