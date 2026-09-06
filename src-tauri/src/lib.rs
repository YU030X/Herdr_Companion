use std::sync::Arc;

use herdr::runtime::Runtime;
#[cfg(all(windows, feature = "webview-memory-experiment"))]
use tauri::Manager;

mod commands;
pub mod herdr;
#[cfg(all(windows, feature = "webview-memory-experiment"))]
mod memory;

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
            #[cfg(all(windows, feature = "webview-memory-experiment"))]
            if let Some(window) = app.get_webview_window("main") {
                memory::apply(&window, window.is_focused().unwrap_or(true));
            }
            monitor.start(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            #[cfg(all(windows, feature = "webview-memory-experiment"))]
            if let tauri::WindowEvent::Focused(focused) = event {
                if let Some(webview) = window.app_handle().get_webview_window(window.label()) {
                    memory::apply(&webview, *focused);
                }
            }
            #[cfg(not(all(windows, feature = "webview-memory-experiment")))]
            let _ = (window, event);
        })
        .run(tauri::generate_context!())
        .expect("failed to run Herdr Companion");
}
