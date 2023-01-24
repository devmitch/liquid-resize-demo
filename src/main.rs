use std::sync::{Arc, Mutex};

use eframe::egui;
use egui_glow::CallbackFn;
use glow::{NativeBuffer, NativeShader, NativeTexture, NativeVertexArray};
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
        if let (Some(canvas), Some(image)) = (&self.canvas, &self.pixel_data) {
            let (rect, _) = ui.allocate_exact_size(
                egui::Vec2::new(image.width() as f32, image.height() as f32),
                egui::Sense::click(),
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
        let shader_version = if cfg!(target_arch = "wasm32") {
            "#version 300 es"
        } else {
            "#version 330"
        };
        unsafe {
            let program = gl.create_program().expect("Failed to create program");
            let (vertex_shader_source, fragment_shader_source) = (
                r#"
                    in vec2 vVertex;
                    out vec2 vUV;
                    void main() {
                        gl_Position = vec4(vVertex*2.0-1,0,1);
                        vUV = vVertex;
                    }
                "#,
                r#"
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

            // ???
            for shader in shaders {
                gl.detach_shader(program, shader);
                gl.delete_shader(shader);
            }

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

            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR_MIPMAP_NEAREST as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
            );

            // gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::REPEAT as i32);
            // gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::REPEAT as i32);

            let pbo = gl.create_buffer().expect("Failed to create PBO");
            gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, Some(pbo));

            println!("before!");
            // maybe should be 3?
            const CHANNELS: u32 = 4;
            println!("width = {width}, height = {height}");
            // gl.buffer_data_size(
            //     glow::PIXEL_UNPACK_BUFFER,
            //     (width * height * CHANNELS) as i32,
            //     glow::STREAM_DRAW,
            // );
            // let mapped_buffer = gl.map_buffer_range(
            //     glow::PIXEL_UNPACK_BUFFER,
            //     0,
            //     (width * height * CHANNELS) as i32,
            //     glow::WRITE_ONLY,
            // );
            // println!("before!");
            // std::ptr::copy_nonoverlapping(
            //     pixel_data.as_ptr(),
            //     mapped_buffer,
            //     (width * height * CHANNELS) as usize,
            // );
            gl.buffer_data_u8_slice(glow::PIXEL_UNPACK_BUFFER, pixel_data, glow::STREAM_DRAW);
            println!("after!");

            //gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, Some(pbo));
            //gl.unmap_buffer(glow::PIXEL_UNPACK_BUFFER);
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
            gl.generate_mipmap(glow::TEXTURE_2D);

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
            gl.uniform_1_i32(
                gl.get_uniform_location(self.program, "textureMap").as_ref(),
                0,
            );
            gl.draw_elements(glow::TRIANGLES, 6, glow::UNSIGNED_INT, 0);
        }
    }
}
