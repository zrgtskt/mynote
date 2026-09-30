// リリース版の Windows で余計なコンソールを出さない
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    mynote_lib::run()
}
