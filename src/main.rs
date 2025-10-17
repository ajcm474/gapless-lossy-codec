mod codec;
mod ui;
mod audio;

use eframe::egui;

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 700.0])
            .with_title("Gapless Audio Codec"),
        ..Default::default()
    };
    
    eframe::run_native(
        "Gapless Audio Codec",
        options,
        Box::new(|_cc| Box::new(ui::CodecApp::new())),
    )
}
