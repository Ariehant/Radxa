pub mod commands;
pub mod error;
pub mod fs;
pub mod git;
pub mod index;
pub mod rpc;
pub mod scaffold;
pub mod state;
pub mod time;
pub mod vault;

use state::{AppState, EventSink};
use std::sync::Arc;
use tauri::{Emitter, Manager};

struct TauriSink(tauri::AppHandle);

impl EventSink for TauriSink {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        if let Err(e) = self.0.emit(event, payload) {
            log::warn!("emit {event} failed: {e}");
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let sink: Arc<dyn EventSink> = Arc::new(TauriSink(app.handle().clone()));
            app.manage(Arc::new(AppState::new(sink)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![commands::rpc])
        .run(tauri::generate_context!())
        .expect("error while running Nexus");
}
