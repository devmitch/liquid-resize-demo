use std::{
    sync::{Arc, Mutex},
    thread,
    time::Instant,
};

use algorithms::OriginalAlgo;
use eframe::egui::{self};
use egui_glow::CallbackFn;
use glow::{NativeBuffer, NativeShader, NativeTexture};
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
    );
}

enum LoadStatus {
    NotLoaded,
    Loaded(String),
    Error(String),
}

impl Default for LoadStatus {
    fn default() -> Self {
        Self::NotLoaded
    }
}

// (write description)
struct CarvingEngine {
    canvas: Arc<Mutex<GlowImageCanvas>>,
    image: DynamicImage,
    algo: Arc<Mutex<OriginalAlgo>>,
    removed_seams: Arc<Mutex<Vec<Vec<u32>>>>,
}

impl CarvingEngine {
    fn new(image: DynamicImage, gl: &glow::Context) -> Self {
        let pixels_rgb8: Vec<[u8; 3]> = image
            .to_rgb8()
            .pixels()
            .map(|x| [x[0], x[1], x[2]])
            .collect();

        let algo = Arc::new(Mutex::new(OriginalAlgo::new(
            pixels_rgb8,
            image.width(),
            image.height(),
        )));

        let removed_seams = Arc::new(Mutex::new(Vec::new()));

        let canvas = Arc::new(Mutex::new(GlowImageCanvas::new(
            gl,
            image.width(),
            image.height(),
            image.as_bytes(),
            image.color().has_alpha(),
        )));

        let mut ret = Self {
            canvas,
            image,
            algo,
            removed_seams,
        };

        ret.run_engine();
        ret
    }

    fn remove_seams(&mut self, gl: &glow::Context, num_seams: u32) {
        let start = Instant::now();
        let mut pixel_data: Vec<[u8; 3]> = self
            .image
            .to_rgb8()
            .pixels()
            .map(|x| [x[0], x[1], x[2]])
            .collect();
        for i in 0..num_seams {
            let mut k = 0;
            let to_remove = &self.removed_seams.lock().unwrap()[i as usize];
            pixel_data = pixel_data
                .iter()
                .enumerate()
                .filter(|(i, _pix)| {
                    if k != to_remove.len() && *i == to_remove[k] as usize {
                        k += 1;
                        false
                    } else {
                        true
                    }
                })
                .map(|(_i, pix)| *pix)
                .collect();
        }
        let flat: Vec<u8> = pixel_data.into_iter().flatten().collect();
        self.canvas.lock().unwrap().update_pixels(
            gl,
            self.image.width() - num_seams,
            self.image.height(),
            &flat,
            false,
        );
        println!("removing {} seams took {:?}", num_seams, start.elapsed());
    }

    fn run_engine(&mut self) {
        let width = self.image.width().clone();
        let algo = self.algo.clone();
        let removed_seams = self.removed_seams.clone();
        thread::spawn(move || {
            let process_start = Instant::now();
            let mut algo = algo.lock().unwrap();
            for _carve_iteration in 0..width - 10 {
                let removed_incices = algo.remove_vertical_seam();
                removed_seams.lock().unwrap().push(removed_incices);
            }
            println!("entire carve took {:?}", process_start.elapsed());
        });
    }

    // draw the image data to the canvas
    fn draw(&self, ui: &mut egui::Ui, seams_removed: u32) {
        let (rect, _) = ui.allocate_exact_size(
            // can scale width and height down if image is too big
            egui::Vec2::new(
                (self.image.width() - seams_removed) as f32,
                self.image.height() as f32,
            ),
            egui::Sense::drag(),
        );

        let canvas = self.canvas.clone();
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

// Contains main app state
#[derive(Default)]
struct LiquidResizeApp {
    image_bundle: Option<CarvingEngine>,
    status: LoadStatus,
    slider_value: u32,
}

impl LiquidResizeApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.gl
            .as_ref()
            .expect("eframe not running with glow backend");
        Self::default()
    }
}

impl eframe::App for LiquidResizeApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let gl = frame.gl().expect("eframe not running with glow backend");
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Liquid Resize (Seam Carving) Demonstration");
            if ui.button("Open image").clicked() {
                if let Some(path) = FileDialog::new().pick_file() {
                    match image::open(&path) {
                        Ok(image) => {
                            self.image_bundle = Some(CarvingEngine::new(image.flipv(), gl));
                            let loaded_status = format!("{} loaded!", path.display().to_string());
                            self.status = LoadStatus::Loaded(loaded_status);
                        }
                        Err(err) => {
                            let file_name = match path.file_name() {
                                Some(name) => name.to_str().unwrap_or("FILENAME DECODE ERROR"),
                                None => "..",
                            };
                            let err = format!("({}): {}", file_name, err.to_string());
                            self.status = LoadStatus::Error(err);
                        }
                    }
                }
            }

            ui.horizontal(|ui| {
                ui.label("Status:");
                match &self.status {
                    LoadStatus::NotLoaded => ui.monospace("No images loaded!"),
                    LoadStatus::Loaded(status) => ui.monospace(status),
                    LoadStatus::Error(err) => ui.monospace(err),
                }
            });

            if let Some(image_bundle) = &mut self.image_bundle {
                egui::Frame::canvas(ui.style()).show(ui, |ui| {
                    image_bundle.draw(ui, self.slider_value);
                });
                ui.label("image is loaded!");

                let seams_removed = image_bundle.removed_seams.lock().unwrap().len();
                ui.label(format!(
                    "carving progress: {}%",
                    ((seams_removed * 100) as f32 / (image_bundle.image.width() - 10) as f32)
                        as u32
                ));

                let slider = egui::Slider::new(&mut self.slider_value, 0..=seams_removed as u32)
                    .text("slide to preview interpolation (normal resize), release to carve")
                    .show_value(false); // turn to false?
                if ui.add(slider).drag_released() {
                    image_bundle.remove_seams(gl, self.slider_value);
                };
            }
        });
    }
}

// Glow/OpenGL canvas quad to draw textures on
struct GlowImageCanvas {
    program: glow::Program,
    tex: NativeTexture,
    pbo: NativeBuffer,
}

impl GlowImageCanvas {
    fn new(
        gl: &glow::Context,
        width: u32,
        height: u32,
        pixel_data: &[u8],
        has_alpha: bool,
    ) -> Self {
        use glow::HasContext as _;
        let shader_version = if cfg!(target_arch = "wasm32") {
            "#version 300 es"
        } else {
            "#version 330"
        };
        unsafe {
            let program = gl.create_program().expect("Failed to create program");
            let (vertex_shader_source, fragment_shader_source) = (
                r#"
                    const vec2 verts[6] = vec2[6](
                        vec2(-1.0, -1.0),
                        vec2(1.0, -1.0),
                        vec2(1.0, 1.0),
                        vec2(-1.0, 1.0),
                        vec2(-1.0, -1.0),
                        vec2(1.0, 1.0)
                    );
                    out vec2 vUV;
                    void main() {
                        gl_Position = vec4(verts[gl_VertexID], 0.0, 1.0);
                        vUV = (verts[gl_VertexID] + 1) / 2;
                    }
                "#,
                r#"
                    precision mediump float;
                    in vec2 vUV;
                    out vec4 vFragColor;
                    uniform sampler2D textureMap;
                    void main() {
                        vFragColor = texture(textureMap, vUV);
                    }
                "#,
            );

            let shader_sources = [
                (glow::VERTEX_SHADER, vertex_shader_source),
                (glow::FRAGMENT_SHADER, fragment_shader_source),
            ];
            let shaders: Vec<NativeShader> = shader_sources
                .iter()
                .map(|(shader_type, shader_source)| {
                    let shader = gl
                        .create_shader(*shader_type)
                        .expect("Cannot create shader");
                    gl.shader_source(shader, &format!("{}\n{}", shader_version, shader_source));
                    gl.compile_shader(shader);
                    assert!(
                        gl.get_shader_compile_status(shader),
                        "Failed to compile {shader_type}: {}",
                        gl.get_shader_info_log(shader)
                    );
                    gl.attach_shader(program, shader);
                    shader
                })
                .collect();

            gl.link_program(program);
            if !gl.get_program_link_status(program) {
                panic!("{}", gl.get_program_info_log(program));
            }

            for shader in shaders {
                gl.detach_shader(program, shader);
                gl.delete_shader(shader);
            }

            // texture setup
            let tex = gl.create_texture().expect("Failed to create texture");
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));

            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::NEAREST as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
            );

            let pbo = gl.create_buffer().expect("Failed to create PBO");
            gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, Some(pbo));
            gl.buffer_data_u8_slice(glow::PIXEL_UNPACK_BUFFER, pixel_data, glow::STREAM_DRAW);

            let format = if has_alpha { glow::RGBA } else { glow::RGB };
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                format as i32,
                width as i32,
                height as i32,
                0,
                format,
                glow::UNSIGNED_BYTE,
                None,
            );

            Self { program, tex, pbo }
        }
    }

    // Draw the texture previously loaded on the GPU via new() or update_pixels()
    fn paint(&self, gl: &glow::Context) {
        use glow::HasContext as _;
        unsafe {
            gl.use_program(Some(self.program));
            gl.bind_texture(glow::TEXTURE_2D, Some(self.tex));
            gl.uniform_1_i32(
                gl.get_uniform_location(self.program, "textureMap").as_ref(),
                0,
            );
            gl.draw_arrays(glow::TRIANGLES, 0, 6);
        }
    }

    // Change the pixel data on the texture via Pixelbuffer
    fn update_pixels(
        &mut self,
        gl: &glow::Context,
        width: u32,
        height: u32,
        pixel_data: &[u8],
        has_alpha: bool,
    ) {
        use glow::HasContext as _;
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(self.tex));
            gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, Some(self.pbo));
            gl.buffer_data_u8_slice(glow::PIXEL_UNPACK_BUFFER, pixel_data, glow::STREAM_DRAW);

            let format = if has_alpha { glow::RGBA } else { glow::RGB };
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                format as i32,
                width as i32,
                height as i32,
                0,
                format,
                glow::UNSIGNED_BYTE,
                None,
            );
        }
    }
}
