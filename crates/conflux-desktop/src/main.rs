#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Release builds have no console (windows_subsystem = "windows"), so a panic
    // would otherwise vanish silently. Persist it to the OS temp directory.
    // `info`'s Display output already contains "panicked at file:line:col:" plus the message.
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("PANIC: {}\n", info);
        eprintln!("{}", msg);
        let _ = std::fs::write(std::env::temp_dir().join("conflux_panic.txt"), &msg);
    }));

    conflux_desktop_lib::run();
}
