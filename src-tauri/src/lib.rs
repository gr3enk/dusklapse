pub mod camera;
mod commands;
mod ramp;
mod session;
mod settings;

use tauri_plugin_log::{Target, TargetKind};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            // Without this, Rust-side logs are lost on iOS - there is no stdout to
            // attach to. The plugin routes them into the platform logger, so they turn
            // up in the Xcode console and in logcat.
            //
            // `targets` replaces the default list; `target` would have appended to it.
            // The defaults are Stdout *and* LogDir, so appending left the app also
            // writing a rotating log file inside its container - never asked for, and
            // the source of a startup crash after the app was renamed.
            tauri_plugin_log::Builder::new()
                .targets([Target::new(TargetKind::Stdout)])
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        // Position for the daylight curve. Registered on every platform because the plugin
        // compiles everywhere, but only CoreLocation and Android answer honestly - see
        // `platform_has_geolocation`, which keeps the desktop stub out of the UI.
        .plugin(tauri_plugin_geolocation::init())
        .manage(session::CameraSession::default())
        .manage(commands::PreviewCache::default())
        .manage(ramp::RampState::default())
        .manage(settings::SettingsState::default())
        .invoke_handler(tauri::generate_handler![
            commands::camera_connect,
            commands::camera_reconnect,
            commands::camera_disconnect,
            commands::camera_status,
            commands::camera_capabilities,
            commands::camera_exposure,
            commands::camera_set_exposure,
            commands::camera_shoot,
            commands::camera_battery,
            commands::camera_preview,
            commands::camera_preview_image,
            commands::camera_vendors,
            commands::ramp_settings,
            commands::ramp_configure,
            commands::ramp_reference_from_latest_frame,
            commands::ramp_prime_reference,
            commands::ramp_apply,
            commands::ramp_sky,
            commands::platform_has_geolocation,
            commands::settings_get,
            commands::settings_set,
            commands::camera_default_port,
        ])
        .run(tauri::generate_context!())
        .expect("error while running dusklapse");
}
