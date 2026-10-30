mod files;

/// A read-only handshake for the development shell.
#[tauri::command]
fn backend_status() -> &'static str {
    "ready"
}

pub fn run() {
    if let Err(error) = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            backend_status,
            files::inspect_files
        ])
        .run(tauri::generate_context!())
    {
        eprintln!("Zapbeam could not start: {error}");
        std::process::exit(1);
    }
}
