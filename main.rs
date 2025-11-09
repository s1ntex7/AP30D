#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod screenshot_new;
mod simple_expansion;
mod voice_to_text;
mod hotkeys;
mod keyboard;

use std::sync::{Arc, RwLock, Once};
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
use simple_expansion::SimpleExpansionState;

#[derive(Clone)]
pub struct HotkeysState {
  vtt: Arc<RwLock<Shortcut>>,
}

fn default_vtt() -> Shortcut { Shortcut::new(Some(Modifiers::empty()), Code::F9) }

static EXPANSION_LISTENER_ONCE: Once = Once::new();

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let expansion_state = SimpleExpansionState::default();

    tauri::Builder::default()
        .manage(expansion_state.clone())
        .manage(HotkeysState { vtt: Arc::new(RwLock::new(default_vtt())) })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(move |app| {
            tracing::info!("🔧 setup() start");

            // Auto-load shortcuts from default file
            let loaded = expansion_state.load_from_file(None).unwrap_or(0);
            tracing::info!("[TEXP] Auto-loaded {} shortcuts from default file", loaded);

            let gs = app.global_shortcut();

            // Home → VTT (changed from F9 due to hotkey conflict)
            gs.on_shortcut("Home", {
                let app = app.handle().clone();
                move |_app, _shortcut, event| {
                    tracing::info!("🎹 Home (VTT) {:?}", event);
                    // Reaguj tylko na wciśnięcie (Pressed)
                    if format!("{:?}", event).contains("Pressed") {
                        let _ = app.emit_to("main", "vtt:hotkey", ());
                    }
                }
            }).map_err(|e| {
                tracing::error!("❌ Home register failed: {}", e);
                e
            })?;

            // F10 → screenshot active monitor (where cursor is) - NEW PRIMARY HOTKEY
            gs.on_shortcut("F10", {
                let app = app.handle().clone();
                move |_app, _shortcut, event| {
                    tracing::info!("🎹 F10 (Active Monitor) {:?}", event);
                    if format!("{:?}", event).contains("Pressed") {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.set_focus();
                        }
                        let _ = app.emit_to("main", "screenshot-active-monitor", ());
                    }
                }
            }).map_err(|e| {
                tracing::error!("❌ F10 register failed: {}", e);
                e
            })?;

            // F11 → screenshot ALL monitors - FOR POWER USERS
            gs.on_shortcut("F11", {
                let app = app.handle().clone();
                move |_app, _shortcut, event| {
                    tracing::info!("🎹 F11 (All Monitors) {:?}", event);
                    if format!("{:?}", event).contains("Pressed") {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.set_focus();
                        }
                        let _ = app.emit_to("main", "screenshot-all-monitors", ());
                    }
                }
            }).map_err(|e| {
                tracing::error!("❌ F11 register failed: {}", e);
                e
            })?;

            // TEXT EXPANSION: start global keyboard listener (rdev)
            EXPANSION_LISTENER_ONCE.call_once(|| {
                // 1) pobierz zarządzany stan
                let exp_state = app.state::<SimpleExpansionState>();
                let shortcuts = exp_state.shortcuts.clone();
                let paused = exp_state.paused.clone();

                // 2) ustal ścieżkę store
                let store_path = app.path()
                    .app_data_dir()
                    .expect("app_data_dir")
                    .join("shortcuts.json");

                tracing::info!("🧠 Starting TextExpansion listener at {:?}", store_path);
                simple_expansion::spawn_expansion_listener(
                    app.handle().clone(),
                    shortcuts,
                    store_path,
                    paused,
                );
            });

            tracing::info!("✅ setup() done");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            simple_expansion::add_shortcut,
            simple_expansion::update_shortcut,
            simple_expansion::remove_shortcut,
            simple_expansion::list_shortcuts,
            simple_expansion::save_shortcuts,
            simple_expansion::load_shortcuts,
            simple_expansion::get_storage_path,
            simple_expansion::export_shortcuts,
            simple_expansion::import_shortcuts,
            voice_to_text::paste_text,
            voice_to_text::set_recording_state,
            hotkeys::get_vtt_hotkey,
            screenshot_new::launch_screenshot_overlay,  // LEGACY F8 (deprecated)
            screenshot_new::launch_screenshot_overlay_active_monitor,  // NEW F10
            screenshot_new::launch_screenshot_overlay_all_monitors     // NEW F11
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}