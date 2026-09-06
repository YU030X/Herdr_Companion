//! Best-effort cache trimming; the WebView keeps executing scripts and receiving events.
use std::sync::OnceLock;

use tauri::WebviewWindow;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL,
    COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
};
use windows_core::Interface;

fn target(focused: bool, force_normal: bool) -> COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL {
    if focused || force_normal {
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
    } else {
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
    }
}

pub fn apply(window: &WebviewWindow, focused: bool) {
    static FORCE_NORMAL: OnceLock<bool> = OnceLock::new();
    static DIAGNOSTICS: OnceLock<bool> = OnceLock::new();
    // A same-binary control for local A/B measurements, never persisted to user preferences.
    let force_normal = *FORCE_NORMAL.get_or_init(|| {
        std::env::var("HERDR_COMPANION_MEMORY_POLICY").is_ok_and(|value| value == "normal")
    });
    let diagnostics = *DIAGNOSTICS.get_or_init(|| {
        std::env::var("HERDR_COMPANION_MEMORY_DIAGNOSTICS").is_ok_and(|value| value == "1")
    });
    let level = target(focused, force_normal);
    let dispatched = window.with_webview(move |webview| {
        // SAFETY: Tauri executes with_webview on the owning UI thread. COM handles
        // are created, cast and used only inside this callback and never retained.
        let result = unsafe {
            (|| -> windows_core::Result<i32> {
                let core: ICoreWebView2_19 = webview.controller().CoreWebView2()?.cast()?;
                core.SetMemoryUsageTargetLevel(level)?;
                let mut actual = COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL;
                core.MemoryUsageTargetLevel(&mut actual)?;
                Ok(actual.0)
            })()
        };
        if diagnostics {
            match result {
                Ok(actual) => eprintln!(
                    "memory_policy focused={focused} requested={} actual={actual}",
                    level.0
                ),
                Err(error) => eprintln!(
                    "memory_policy unsupported_or_failed hresult={:?}",
                    error.code()
                ),
            }
        }
        // Older runtimes may lack this interface. Failure keeps their normal policy
        // and must not interfere with the Herdr monitor or window lifecycle.
    });
    if diagnostics && dispatched.is_err() {
        eprintln!("memory_policy dispatch_failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_restores_normal_and_only_background_requests_low() {
        assert_eq!(
            target(true, false),
            COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
        );
        assert_eq!(
            target(false, false),
            COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
        );
        assert_eq!(
            target(false, true),
            COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
        );
        assert_eq!(
            target(true, true),
            COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
        );
    }
}
