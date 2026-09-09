use std::sync::Arc;

use herdr::runtime::Runtime;
use tauri::Manager;
use tauri_plugin_window_state::WindowExt;

mod commands;
pub mod herdr;
#[cfg(all(windows, feature = "webview-memory-experiment"))]
mod memory;

fn window_state_flags() -> tauri_plugin_window_state::StateFlags {
    use tauri_plugin_window_state::StateFlags;

    StateFlags::POSITION | StateFlags::SIZE
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let runtime = Arc::new(Runtime::new().expect("failed to initialize Herdr runtime"));
    let monitor = Arc::clone(&runtime);

    tauri::Builder::default()
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(window_state_flags())
                .build(),
        )
        .manage(runtime)
        .invoke_handler(tauri::generate_handler![
            commands::get_app_state,
            commands::retry_connection,
            commands::focus_agent,
            commands::set_always_on_top,
        ])
        .setup(move |app| {
            let window = app.get_webview_window("main").ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "main window was not created")
            })?;
            window.restore_state(window_state_flags())?;
            #[cfg(all(windows, feature = "webview-memory-experiment"))]
            memory::apply(&window, window.is_focused().unwrap_or(true));
            window.show()?;
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

#[cfg(test)]
mod tests {
    #[test]
    fn restores_only_window_position_and_size() {
        use tauri_plugin_window_state::StateFlags;

        let flags = super::window_state_flags();
        assert!(flags.contains(StateFlags::POSITION));
        assert!(flags.contains(StateFlags::SIZE));
        assert!(!flags.contains(StateFlags::MAXIMIZED));
        assert!(!flags.contains(StateFlags::FULLSCREEN));
        assert!(!flags.contains(StateFlags::VISIBLE));
    }
}
