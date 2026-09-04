use std::sync::Arc;

use herdr::runtime::Runtime;

mod commands;
pub mod herdr;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let runtime = Arc::new(Runtime::new().expect("failed to initialize Herdr runtime"));
    let monitor = Arc::clone(&runtime);

    tauri::Builder::default()
        .manage(runtime)
        .invoke_handler(tauri::generate_handler![
            commands::get_app_state,
            commands::retry_connection,
            commands::focus_agent,
            commands::set_always_on_top,
        ])
        .setup(move |app| {
            monitor.start(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run Herdr Companion");
}
