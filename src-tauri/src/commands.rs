use std::sync::Arc;

use tauri::{State, WebviewWindow};

use crate::herdr::runtime::{Runtime, RuntimeView};

#[tauri::command]
pub fn get_app_state(runtime: State<'_, Arc<Runtime>>) -> RuntimeView {
    runtime.view()
}

#[tauri::command]
pub fn retry_connection(runtime: State<'_, Arc<Runtime>>) {
    runtime.retry();
}

#[tauri::command]
pub fn focus_agent(target: String, runtime: State<'_, Arc<Runtime>>) -> Result<(), String> {
    if !runtime.is_connected() {
        return Err("Herdr 未连接，无法聚焦 Agent".to_owned());
    }

    runtime
        .client()
        .focus_agent(&target)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_always_on_top(window: WebviewWindow, enabled: bool) -> Result<(), String> {
    window
        .set_always_on_top(enabled)
        .map_err(|error| format!("无法更新置顶状态：{error}"))
}
