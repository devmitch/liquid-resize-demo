use std::sync::{Arc, Mutex};

use eframe::egui;
use egui_glow::CallbackFn;
use glow::{NativeBuffer, NativeTexture, NativeVertexArray};
use image::DynamicImage;
use rfd::FileDialog;

fn main() {
    let native_options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(500.0, 500.0)),
        multisampling: 8,
        renderer: eframe::Renderer::Glow,
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
    canvas: Option<Arc<Mutex<GlowImageCanvas>>>,
    pixel_data: Option<DynamicImage>,
}

// If I never use this, remove and simply use LiquidResizeApp::default() in main fn

impl LiquidResizeApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.gl
            .as_ref()
            .expect("eframe not running with glow backend");
        Self::default()
    }

    fn draw_image(&self, ui: &mut egui::Ui) {
        let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(300.0), egui::Sense::click());

        if let Some(canvas) = &self.canvas {
            let canvas = canvas.clone();
            let callback = egui::PaintCallback {
                callback: Arc::new(CallbackFn::new(move |_info, painter| {
                    canvas
                        .lock()
                        .expect("Failed to grab lock")
                        .paint(painter.gl());
                })),
                rect,
            };
            ui.painter().add(callback);
        }
    }
}

impl eframe::App for LiquidResizeApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Hello");
            if ui.button("Open image").clicked() {
                if let Some(path) = FileDialog::new().pick_file() {
                    let path = path.display().to_string();
                    self.pixel_data = match image::open(&path) {
                        Ok(image) => {
                            self.picked_path = Some(path);
                            let gl = frame.gl().expect("eframe not running with glow backend");
                            let new_canvas = Arc::new(Mutex::new(GlowImageCanvas::new(
                                gl,
                                image.width(),
                                image.height(),
                                image.as_bytes(),
                            )));
                            self.canvas = Some(new_canvas);
                            Some(image)
                        }
                        Err(err) => {
                            self.picked_path = Some(err.to_string());
                            None
                        }
                    }
                }
            }

            if let Some(picked_path) = &self.picked_path {
                ui.horizontal(|ui| {
                    ui.label("Status:");
                    ui.monospace(picked_path);
                });
            }

            if let Some(pixel_data) = &self.pixel_data {
                egui::Frame::canvas(ui.style()).show(ui, |ui| {
                    self.draw_image(ui);
                });
                ui.label("image is loaded!");
            }
        });
    }
}

fn load_image_from_path(path: &std::path::Path) -> Result<DynamicImage, image::ImageError> {
    image::open(path)
}

struct GlowImageCanvas {
    program: glow::Program,
    vao: NativeVertexArray,
    pbo: NativeBuffer,
    tex: NativeTexture,
}

impl GlowImageCanvas {
    fn new(gl: &glow::Context, width: u32, height: u32, pixel_data: &[u8]) -> Self {
        use glow::HasContext as ctx; // ????
        unsafe {
            let program = gl.create_program().expect("Failed to create program");
            let vao = gl.create_vertex_array().expect("Failed to create VAO");
            let vbo = gl.create_buffer().expect("Failed to create VBO");
            let ebo = gl.create_buffer().expect("Failed to create EBO");
            gl.bind_vertex_array(Some(vao));

            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            // could maybe be const variable?
            let vertices: [u8; 8] = [0, 0, 1, 0, 1, 1, 0, 1];
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &vertices, glow::STATIC_DRAW);

            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));
            let indices: [u8; 6] = [0, 1, 3, 1, 2, 3];
            gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, &indices, glow::STATIC_DRAW);

            gl.vertex_attrib_pointer_f32(
                0,
                2,
                glow::FLOAT,
                false,
                (2 * std::mem::size_of::<f32>()) as i32,
                0,
            );
            gl.enable_vertex_attrib_array(0);

            //cleanup
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            gl.bind_vertex_array(None);

            // texture setup
            let tex = gl.create_texture().expect("Failed to create texture");
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            let pbo = gl.create_buffer().expect("Failed to create PBO");
            gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, Some(pbo));
            // maybe should be *3?
            gl.buffer_data_size(
                glow::PIXEL_UNPACK_BUFFER,
                (width * height * 4) as i32,
                glow::STREAM_DRAW,
            );
            let mapped_buffer = gl.map_buffer_range(
                glow::PIXEL_UNPACK_BUFFER,
                0,
                (width * height * 4) as i32,
                glow::WRITE_ONLY,
            );
            std::ptr::copy_nonoverlapping(
                pixel_data.as_ptr(),
                mapped_buffer,
                (width * height * 4) as usize,
            );

            gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, Some(pbo));
            gl.unmap_buffer(glow::PIXEL_UNPACK_BUFFER);
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA as i32,
                width as i32,
                height as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                None,
            );

            Self {
                program,
                vao,
                pbo,
                tex,
            }
        }
    }

    // we might not need to use ColorImage, and instead use raw pixel data from image io read
    fn paint(&self, gl: &glow::Context) {
        use glow::HasContext as _;
        unsafe {
            gl.use_program(Some(self.program));
            gl.bind_vertex_array(Some(self.vao));
            gl.bind_texture(glow::TEXTURE_2D, Some(self.tex));
            gl.draw_elements(glow::TRIANGLES, 6, glow::UNSIGNED_INT, 0);
        }
    }
}
