#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // The panic hook and file logging are installed in `run()`.
    conflux_desktop_lib::run();
}
