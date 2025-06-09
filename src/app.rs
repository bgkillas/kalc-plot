use crate::data::Data;
#[cfg(feature = "kalc-lib")]
use crate::data::init;
use crate::{App, get_names};
use rupl::types::Graph;
impl App {
    #[cfg(feature = "kalc-lib")]
    pub(crate) fn new(function: String, data: kalc_lib::units::Data) -> Self {
        #[cfg(feature = "bincode")]
        let mut function = function;
        #[cfg(feature = "bincode")]
        let tiny = (&function).try_into().ok();
        #[cfg(feature = "bincode")]
        if tiny.is_some() {
            function = String::new()
        }
        let kalc_lib::units::Data {
            mut options,
            vars,
            colors,
        } = data;
        let mut side = false;
        let (data, names, graphing_mode) =
            if let Ok(a) = init(&function, &mut options, vars.clone()) {
                if a.0.iter().any(|a| a.is_none()) {
                    side = true
                }
                a
            } else {
                side = true;
                Default::default()
            };
        let tab_complete = {
            let vars = vars.clone();
            let word =
                move |w: &str| -> Vec<String> { kalc_lib::misc::get_word_bank(w, &vars, options) };
            Some(Box::new(word) as Box<dyn Fn(&str) -> Vec<String>>)
        };
        let mut data = Data {
            data,
            options,
            vars,
            blacklist: Vec::new(),
            var: rupl::types::Vec2::new(options.xr.0, options.xr.1),
            count_changed: false,
        };
        let (graph, complex) = if graphing_mode.x && graphing_mode.y {
            data.generate_3d(
                options.xr.0,
                options.yr.0,
                options.xr.1,
                options.yr.1,
                options.samples_3d.0,
                options.samples_3d.1,
                None,
            )
        } else {
            data.generate_2d(options.xr.0, options.xr.1, options.samples_2d, None)
        };
        let names = get_names(&graph, &names);
        if options.vxr.0 != 0.0 || options.vxr.1 != 0.0 {
            options.xr = options.vxr;
        }
        if options.vyr.0 != 0.0 || options.vyr.1 != 0.0 {
            options.yr = options.vyr;
        }
        if options.vzr.0 != 0.0 || options.vzr.1 != 0.0 {
            options.zr = options.vzr;
        }
        #[cfg(feature = "bincode")]
        let b = side && tiny.is_none();
        #[cfg(not(feature = "bincode"))]
        let b = side;
        let mut plot = Graph::new(graph, names, complex, options.xr.0, options.xr.1);
        plot.tab_complete = tab_complete;
        #[cfg(feature = "bincode")]
        {
            plot.save_file =
                dirs::config_dir().unwrap().to_str().unwrap().to_owned() + "/kalc/plot";
        }
        if b {
            plot.menu = rupl::types::Menu::Side;
            plot.text_box = Some((0, 0));
        }
        plot.is_complex = complex;
        plot.mult = 1.0 / 16.0;
        plot.main_colors = colors
            .recol
            .iter()
            .map(|color| rupl::types::Color {
                r: u8::from_str_radix(&color[1..3], 16).unwrap(),
                g: u8::from_str_radix(&color[3..5], 16).unwrap(),
                b: u8::from_str_radix(&color[5..7], 16).unwrap(),
            })
            .collect();
        plot.alt_colors = colors
            .imcol
            .iter()
            .map(|color| rupl::types::Color {
                r: u8::from_str_radix(&color[1..3], 16).unwrap(),
                g: u8::from_str_radix(&color[3..5], 16).unwrap(),
                b: u8::from_str_radix(&color[5..7], 16).unwrap(),
            })
            .collect();
        if plot.is_3d {
            match options.graphtype {
                kalc_lib::units::GraphType::Domain => {
                    plot.set_mode(rupl::types::GraphMode::DomainColoring)
                }
                kalc_lib::units::GraphType::DomainAlt => {
                    plot.set_mode(rupl::types::GraphMode::DomainColoring);
                    plot.domain_alternate = true;
                }
                _ => {}
            }
        }
        data.update(&mut plot);
        Self {
            plot,
            data,
            #[cfg(feature = "bincode")]
            tiny,
            #[cfg(any(feature = "skia", feature = "tiny-skia"))]
            #[cfg(not(feature = "skia-vulkan"))]
            surface_state: None,
            #[cfg(any(feature = "skia", feature = "tiny-skia"))]
            input_state: rupl::types::InputState::default(),
            #[cfg(any(feature = "skia", feature = "tiny-skia"))]
            name: function,
            #[cfg(any(feature = "skia", feature = "tiny-skia"))]
            touch_positions: Default::default(),
            #[cfg(any(feature = "skia", feature = "tiny-skia"))]
            last_touch_positions: Default::default(),
        }
    }
    #[cfg(not(feature = "kalc-lib"))]
    pub(crate) fn new(_function: String) -> Self {
        let options = crate::data::Options::default();
        let mut data = Data {
            data: vec![Some(crate::data::Plot {
                graph_type: crate::data::Type {
                    val: crate::data::Val::Num(None),
                    how: crate::data::HowGraphing {
                        graph: true,
                        x: true,
                        y: true,
                        w: false,
                    },
                    inv: None,
                },
            })],
            blacklist: Vec::new(),
            options,
            var: rupl::types::Vec2::new(options.xr.0, options.xr.1),
            count_changed: false,
        };
        let (graph, complex) = data.generate_3d(
            options.xr.0,
            options.yr.0,
            options.xr.1,
            options.yr.1,
            options.samples_3d.0,
            options.samples_3d.1,
            None,
        );
        let names = &[];
        let names = get_names(&graph, names);
        let mut plot = Graph::new(graph, names, complex, options.xr.0, options.xr.1);
        #[cfg(feature = "bincode")]
        {
            plot.save_file =
                dirs::config_dir().unwrap().to_str().unwrap().to_owned() + "/kalc/plot";
        }
        plot.is_complex = complex;
        plot.mult = 1.0 / 16.0;
        data.update(&mut plot);
        Self {
            plot,
            data,
            #[cfg(feature = "bincode")]
            tiny,
            #[cfg(feature = "wasm")]
            window: None,
            #[cfg(any(feature = "skia", feature = "tiny-skia"))]
            #[cfg(not(feature = "wasm"))]
            surface_state: None,
            #[cfg(any(feature = "skia", feature = "tiny-skia"))]
            input_state: rupl::types::InputState::default(),
            #[cfg(any(feature = "skia", feature = "tiny-skia"))]
            name: _function,
            #[cfg(any(feature = "skia", feature = "tiny-skia"))]
            touch_positions: Default::default(),
            #[cfg(any(feature = "skia", feature = "tiny-skia"))]
            last_touch_positions: Default::default(),
        }
    }
    #[cfg(feature = "egui")]
    pub(crate) fn main(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(egui::Color32::from_rgb(255, 255, 255)))
            .show(ctx, |ui| {
                self.plot.keybinds(ui);
                let rect = ctx.available_rect();
                self.plot
                    .set_screen(rect.width() as f64, rect.height() as f64, true, true);
                #[cfg(feature = "bincode")]
                if let Some(tiny) = std::mem::take(&mut self.tiny) {
                    self.plot.apply_tiny(tiny);
                }
                if let Some(n) = self.data.update(&mut self.plot) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Title(n))
                }
                self.plot.update(ctx, ui);
            });
    }
    #[cfg(any(feature = "skia", feature = "tiny-skia"))]
    #[cfg(feature = "skia-vulkan")]
    pub(crate) fn main(&mut self, width: u32, height: u32) {
        let mut b = false;
        self.plot.keybinds(&self.input_state);
        self.plot
            .set_screen(width as f64, height as f64, true, true);
        #[cfg(feature = "bincode")]
        if let Some(tiny) = std::mem::take(&mut self.tiny) {
            self.plot.apply_tiny(tiny);
        }
        if let Some(n) = self.data.update(&mut self.plot) {
            b = true;
            self.name = n;
        };
        self.plot.update();
        if b {
            if let Some(w) = &self.plot.renderer {
                self.set_title(&w.window);
            }
        }
    }
    #[cfg(any(feature = "skia", feature = "tiny-skia"))]
    #[cfg(not(feature = "skia-vulkan"))]
    #[cfg(not(feature = "wasm"))]
    pub(crate) fn main(&mut self, width: u32, height: u32) {
        let mut b = false;
        if let Some(buffer) = &mut self.surface_state {
            self.plot.keybinds(&self.input_state);
            self.plot
                .set_screen(width as f64, height as f64, true, true);
            #[cfg(feature = "bincode")]
            if let Some(tiny) = std::mem::take(&mut self.tiny) {
                self.plot.apply_tiny(tiny);
            }
            if let Some(n) = self.data.update(&mut self.plot) {
                b = true;
                self.name = n;
            };
            let mut buffer = buffer.buffer_mut().unwrap();
            #[cfg(not(feature = "tiny-skia"))]
            self.plot.update(width, height, &mut buffer);
            #[cfg(feature = "tiny-skia")]
            {
                self.plot.update(width, height, &mut buffer);
            }
            buffer.present().unwrap();
        }
        if b {
            if let Some(w) = &self.surface_state {
                self.set_title(w.window());
            }
        }
    }
    #[cfg(any(feature = "skia", feature = "tiny-skia"))]
    #[cfg(not(feature = "skia-vulkan"))]
    #[cfg(feature = "wasm")]
    pub(crate) fn main(&mut self, width: u32, height: u32) {
        let mut b = false;
        self.plot.keybinds(&self.input_state);
        self.plot
            .set_screen(width as f64, height as f64, true, true);
        #[cfg(feature = "bincode")]
        if let Some(tiny) = std::mem::take(&mut self.tiny) {
            self.plot.apply_tiny(tiny);
        }
        if let Some(n) = self.data.update(&mut self.plot) {
            b = true;
            self.name = n;
        };
        let mut v = Vec::new();
        #[cfg(not(feature = "tiny-skia"))]
        self.plot.update(width, height, &mut v);
        #[cfg(feature = "tiny-skia")]
        {
            self.plot.update(width, height, &mut v);
        }
        let canvas = self.plot.canvas.as_ref().unwrap();
        draw_buffer_web(
            self.window.as_ref().unwrap(),
            canvas.width(),
            wasm_bindgen::Clamped(canvas.data()),
        );
        if b {
            let name = self.name.clone();
            if let Some(w) = self.window() {
                if name.is_empty() {
                    w.set_title("kalc-plot");
                } else {
                    w.set_title(&name);
                }
            }
        }
    }
}
#[cfg(feature = "wasm")]
fn draw_buffer_web(win: &winit::window::Window, width: u32, clamped: wasm_bindgen::Clamped<&[u8]>) {
    use wasm_bindgen::prelude::*;
    let canvas = get_a_canvas(win);
    let ctx: web_sys::CanvasRenderingContext2d = canvas
        .get_context("2d")
        .expect("Failed to get 2d context")
        .expect("Failed to get 2d context")
        .dyn_into()
        .expect("Failed to convert to CanvasRenderingContext2d");
    let image = web_sys::ImageData::new_with_u8_clamped_array(clamped, width)
        .expect("Failed to create image data");
    ctx.put_image_data(&image, 0.0, 0.0)
        .expect("Failed to put image data");

    fn get_a_canvas(win: &winit::window::Window) -> web_sys::HtmlCanvasElement {
        use winit::platform::web::WindowExtWebSys;
        win.canvas().expect("Failed to get canvas")
    }
}
