use eframe::egui;

fn main() {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native("Liquid Resize Demo", native_options, Box::new(|cc| Box::new(LiquidResizeApp::new(cc))))
}

#[derive(Default)]
struct LiquidResizeApp {}


impl LiquidResizeApp {
    fn new(cc: &eframe::CreationContext) -> Self {
        Default::default()
    }
}

impl eframe::App for LiquidResizeApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Hello")
        });
    }
}