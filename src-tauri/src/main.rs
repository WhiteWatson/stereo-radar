// Windows release 下隐藏控制台窗口。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    stereo_radar_app_lib::run()
}
