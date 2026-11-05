mod commands;
mod job;

use job::JobState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(JobState::new())
        .invoke_handler(tauri::generate_handler![
            commands::review_export_plan,
            commands::execute_export,
            commands::cancel_background_work,
            commands::validate_export_form,
        ])
        .run(tauri::generate_context!())
        .expect("error while running roms-curator GUI");
}
