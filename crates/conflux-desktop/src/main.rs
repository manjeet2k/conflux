#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let is_native_host = args.iter().any(|a| {
        a == "--native-host"
            || a.starts_with("chrome-extension://")
            || a == "conflux@conflux.app"
            || (a.ends_with(".json") && a.contains("com.conflux.desktop"))
    });
    if is_native_host {
        conflux_desktop_lib::run_native_host();
        return;
    }
    if args.iter().any(|a| a == "--register-browser") {
        let _ = conflux_desktop_lib::register_browser_integration();
        return;
    }
    if args.iter().any(|a| a == "--unregister-browser") {
        let _ = conflux_desktop_lib::unregister_browser_integration();
        return;
    }
    // The panic hook and file logging are installed in `run()`.
    conflux_desktop_lib::run();
}
