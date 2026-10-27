#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod ui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1320.0, 820.0])
            .with_min_inner_size([1060.0, 700.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Branchroom — Git workspace",
        options,
        Box::new(|cc| Ok(Box::new(ui::Branchroom::new(cc)))),
    )
}
