use std::sync::{Arc, Mutex};

use eframe::egui;
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
    )
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

// Bundle that contains image data as well as a canvas to draw it on
struct ImageBundle {
    canvas: Arc<Mutex<GlowImageCanvas>>,
    image: DynamicImage,
}

impl ImageBundle {
    fn new(image: DynamicImage, gl: &glow::Context) -> Self {
        let canvas = Arc::new(Mutex::new(GlowImageCanvas::new(
            gl,
            image.width(),
            image.height(),
            image.as_bytes(),
            image.color().has_alpha(),
        )));
        Self { canvas, image }
    }
    // draw the image data to the canvas
    fn draw(&self, ui: &mut egui::Ui) {
        let (rect, _) = ui.allocate_exact_size(
            // can scale width and height down if image is too big
            egui::Vec2::new(self.image.width() as f32, self.image.height() as f32),
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

    // invert the image and update the texture on the GPU
    fn invert(&mut self, gl: &glow::Context) {
        self.image.invert(); // done in-place
        self.canvas
            .lock()
            .expect("Failed to grab lock")
            .update_pixels(
                gl,
                self.image.width(),
                self.image.height(),
                self.image.as_bytes(),
                self.image.color().has_alpha(),
            );
    }

    // crop the image in half and update the texture on the GPU
    fn crop_half(&mut self, gl: &glow::Context) {
        // unfortunately can't be cropped in place with image crate
        self.image = self
            .image
            .crop_imm(0, 0, self.image.width() / 2, self.image.height());
        self.canvas
            .lock()
            .expect("Failed to grab lock")
            .update_pixels(
                gl,
                self.image.width(),
                self.image.height(),
                self.image.as_bytes(),
                self.image.color().has_alpha(),
            );
    }
}

// Contains main app state
#[derive(Default)]
struct LiquidResizeApp {
    image_bundle: Option<ImageBundle>,
    status: LoadStatus,
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
            ui.heading("Hello");
            if ui.button("Open image").clicked() {
                if let Some(path) = FileDialog::new().pick_file() {
                    match image::open(&path) {
                        Ok(image) => {
                            self.image_bundle = Some(ImageBundle::new(image.flipv(), gl));
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
                    image_bundle.draw(ui);
                });
                ui.label("image is loaded!");
                if ui.button("invert").clicked() {
                    image_bundle.invert(gl);
                }
                if ui.button("crop").clicked() {
                    image_bundle.crop_half(gl);
                }
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
