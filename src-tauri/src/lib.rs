mod diagnostics;
mod files;
use tauri::Manager;
mod optimizer;

/// A read-only handshake for the development shell.
#[tauri::command]
fn backend_status() -> &'static str {
    "ready"
}

pub fn run() {
    if let Err(error) = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if let Err(error) = app
                .path()
                .app_log_dir()
                .map_err(|e| e.to_string())
                .and_then(|directory| diagnostics::init(directory).map_err(|e| e.to_string()))
            {
                eprintln!("Compression diagnostics unavailable: {error}");
            }
            app.manage(optimizer::Optimizer::new());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            backend_status,
            files::inspect_files,
            optimizer::schedule_optimization,
            optimizer::cancel_optimization,
            optimizer::optimization_results
        ])
        .run(tauri::generate_context!())
    {
        eprintln!("Zapbeam could not start: {error}");
        std::process::exit(1);
    }
}
