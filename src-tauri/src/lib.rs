//! OpenAudioPower – recording, editing and stem separation studio (Tauri shell).

mod commands;
mod engine;
mod plugins;
mod state;

use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let files: Vec<String> = std::env::args().skip(1).filter(|a| !a.starts_with('-')).collect();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .setup(move |app| {
            let data = app.path().app_local_data_dir().unwrap_or_else(|_| std::env::temp_dir().join("OpenAudioPower"));
            let config = app.path().app_config_dir().unwrap_or_else(|_| data.clone());
            // remembered output device
            let preferred = std::fs::read_to_string(config.join("settings.json"))
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .and_then(|v| v.get("audio").and_then(|a| a.get("output")).and_then(|o| o.as_str()).map(|s| s.to_string()))
                .filter(|s| !s.is_empty());
            let engine = engine::Engine::start(preferred);
            let st = state::App::new(engine, data.join("models"), config, files.clone());
            app.manage(Arc::new(st));
            plugins::install();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::system_language,
            commands::startup_files,
            commands::load_settings,
            commands::save_settings,
            commands::cancel_job,
            commands::import_files,
            commands::list_sources,
            commands::source_peaks,
            commands::source_samples,
            commands::source_bands,
            commands::remove_sources,
            commands::rename_source,
            commands::clear_sources,
            commands::set_project,
            commands::transport,
            commands::poll,
            commands::spectrum,
            commands::audio_devices,
            commands::set_audio_device,
            commands::input_monitor,
            commands::record_start,
            commands::record_stop,
            commands::apply_op,
            commands::bounce,
            commands::analyze,
            commands::spectrogram,
            commands::eq_response,
            commands::separation_models,
            commands::download_model,
            commands::delete_model,
            commands::separate_source,
            commands::export_mix,
            commands::export_tracks,
            commands::export_source,
            commands::list_audio_files,
            commands::batch_process,
            plugins::plugin_dirs,
            plugins::plugin_scan,
            plugins::plugin_create,
            plugins::plugin_restart,
            plugins::plugin_editor,
            plugins::plugin_states,
            plugins::plugin_status,
            plugins::plugin_retain,
            commands::save_project,
            commands::load_project,
        ])
        .run(tauri::generate_context!())
        .expect("error while running OpenAudioPower");
}
