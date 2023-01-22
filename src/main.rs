use eframe::{egui, epaint::ColorImage};
use rfd::FileDialog;

fn main() {
    let native_options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(500.0, 500.0)),
        ..Default::default()
    };
    eframe::run_native(
        "Liquid Resize Demo",
        native_options,
        Box::new(|cc| Box::new(LiquidResizeApp::new(cc))),
    )
}

// I probably wanna do this and write to a framebuffer bound to a texture
// https://stackoverflow.com/questions/3887636/how-to-manipulate-texture-content-on-the-fly/10702468#10702468
// https://github.com/emilk/egui/blob/master/examples/custom_3d_glow/src/main.rs

// App state goes here
#[derive(Default)]
struct LiquidResizeApp {
    picked_path: Option<String>,
    image: Option<egui::ColorImage>,
    loaded_texture: Option<egui::TextureHandle>,
}

// If I never use this, remove and simply use LiquidResizeApp::default() in main fn
impl LiquidResizeApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Default::default()
    }

    // fn pick_image(&mut self) {
    //     if let Some(path) = FileDialog::new().pick_file() {
    //         self.picked_path = Some(path.display().to_string());
    //         self.image = Some(load_image_from_path(path.as_path()));

    //     }
    // }
}

impl eframe::App for LiquidResizeApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Hello");
            if ui.button("Open image").clicked() {
                if let Some(path) = FileDialog::new().pick_file() {
                    self.picked_path = Some(path.display().to_string());
                    self.image = match load_image_from_path(path.as_path()) {
                        Ok(img) => {
                            self.loaded_texture = Some(ui.ctx().load_texture(
                                "image",
                                img.clone(), // think this is uploading the image to the gpu via a texture
                                Default::default(),
                            ));
                            Some(img)
                        }
                        Err(err) => {
                            self.picked_path = Some(err.to_string());
                            None
                        }
                    };
                }
            }

            if let Some(picked_path) = &self.picked_path {
                ui.horizontal(|ui| {
                    ui.label("Status:");
                    ui.monospace(picked_path);
                });
            }

            if let Some(tex) = &self.loaded_texture {
                ui.image(tex, tex.size_vec2());
            }
        });
    }
}

fn load_image_from_path(path: &std::path::Path) -> Result<egui::ColorImage, image::ImageError> {
    let img = image::io::Reader::open(path)?.decode()?;
    let rgb8_data = img.to_rgb8();
    let pixels = rgb8_data.as_flat_samples();
    Ok(ColorImage::from_rgb(
        [img.width() as usize, img.height() as usize],
        pixels.as_slice(),
    ))
}
