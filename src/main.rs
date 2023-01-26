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

// App state goes here
#[derive(Default)]
struct LiquidResizeApp {
    picked_path: Option<String>,
    canvas: Option<Arc<Mutex<GlowImageCanvas>>>,
    pixel_data: Option<DynamicImage>,
}

impl LiquidResizeApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.gl
            .as_ref()
            .expect("eframe not running with glow backend");
        Self::default()
    }

    fn draw_image(&self, ui: &mut egui::Ui) {
        if let (Some(canvas), Some(image)) = (&self.canvas, &self.pixel_data) {
            let (rect, _) = ui.allocate_exact_size(
                // can scale width and height down if image is too big
                egui::Vec2::new(image.width() as f32, image.height() as f32),
                egui::Sense::drag(),
            );

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

    fn invert_image(&mut self, gl: &glow::Context) {
        if let (Some(image), Some(canvas)) = (&mut self.pixel_data, &mut self.canvas) {
            image.invert();
            canvas.lock().expect("Failed to grab lock").update_pixels(
                gl,
                image.width(),
                image.height(),
                image.as_bytes(),
                image.color().has_alpha(),
            );
        }
    }

    fn crop_image(&mut self, gl: &glow::Context) {
        self.pixel_data =
            self.pixel_data
                .as_mut()
                .zip(self.canvas.as_mut())
                .map(|(image, canvas)| {
                    let new_image = image.crop_imm(0, 0, image.width() / 2, image.height());
                    canvas.lock().expect("Failed to grab lock").update_pixels(
                        gl,
                        new_image.width(),
                        new_image.height(),
                        new_image.as_bytes(),
                        new_image.color().has_alpha(),
                    );
                    new_image
                });
    }
}

impl eframe::App for LiquidResizeApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let gl = frame.gl().expect("eframe not running with glow backend");
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Hello");
            if ui.button("Open image").clicked() {
                if let Some(path) = FileDialog::new().pick_file() {
                    self.pixel_data = match image::open(&path) {
                        Ok(mut image) => {
                            image = image.flipv();
                            self.picked_path = Some(path.display().to_string());
                            let new_canvas = Arc::new(Mutex::new(GlowImageCanvas::new(
                                gl,
                                image.width(),
                                image.height(),
                                image.as_bytes(),
                                image.color().has_alpha(),
                            )));
                            self.canvas = Some(new_canvas);
                            Some(image)
                        }
                        Err(err) => {
                            let file_name = match path.file_name() {
                                Some(name) => name.to_str().unwrap_or("DECODE ERROR"),
                                None => "..",
                            };
                            self.picked_path = Some(format!("({}) {}", file_name, err.to_string()));
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

            if self.pixel_data.is_some() {
                egui::Frame::canvas(ui.style()).show(ui, |ui| {
                    self.draw_image(ui);
                });
                ui.label("image is loaded!");
                if ui.button("invert").clicked() {
                    self.invert_image(gl);
                }
                if ui.button("crop").clicked() {
                    self.crop_image(gl);
                }
            }
        });
    }
}

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
