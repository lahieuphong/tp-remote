// Release builds use the Windows GUI subsystem so no console window opens.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

fn main() {
    tp_desktop_lib::run();
}
