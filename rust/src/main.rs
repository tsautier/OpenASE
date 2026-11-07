#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod gui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("OpenASE")
            .with_inner_size([1280.0, 760.0])
            .with_min_inner_size([960.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "OpenASE",
        options,
        Box::new(|cc| Ok(Box::new(gui::OpenAseApp::new(cc)))),
    )
}
