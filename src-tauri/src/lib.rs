mod commands;
mod job;
mod library_cmd;
mod progress;

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
            commands::write_plan_report,
            commands::write_plan_html,
            library_cmd::library_status,
            library_cmd::library_credentials,
            library_cmd::library_save_credentials,
            library_cmd::library_add_root,
            library_cmd::library_remove_root,
            library_cmd::library_scan,
            library_cmd::library_enrich,
            library_cmd::library_search,
            library_cmd::library_delete,
            library_cmd::library_open_search,
        ])
        .run(tauri::generate_context!())
        .expect("error while running roms-curator GUI");
}
