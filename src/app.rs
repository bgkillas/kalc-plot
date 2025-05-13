use crate::data::Data;
use crate::data::init;
use crate::{App, get_names};
use rupl::types::Graph;
impl App {
    pub(crate) fn new(function: String, data: kalc_lib::units::Data) -> Self {
        let kalc_lib::units::Data {
            mut options,
            vars,
            colors,
        } = data;
        let mut side = false;
        let (data, names, graphing_mode) =
            if let Ok(a) = init(&function, &mut options, vars.clone()) {
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
        };
        let (graph, complex) = if graphing_mode.x && graphing_mode.y {
            data.generate_3d(
                options.xr.0,
                options.yr.0,
                options.xr.1,
                options.yr.1,
                options.samples_3d.0,
                options.samples_3d.1,
            )
        } else {
            data.generate_2d(options.xr.0, options.xr.1, options.samples_2d)
        };
        let names = get_names(&graph, names);
        if options.vxr.0 != 0.0 || options.vxr.1 != 0.0 {
            options.xr = options.vxr;
        }
        if options.vyr.0 != 0.0 || options.vyr.1 != 0.0 {
            options.yr = options.vyr;
        }
        let mut plot = Graph::new(graph, names, complex, options.xr.0, options.xr.1);
        plot.tab_complete = tab_complete;
        if side {
            plot.draw_side = true;
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
            #[cfg(any(feature = "skia", feature = "tiny-skia"))]
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
    #[cfg(feature = "egui")]
    pub(crate) fn main(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(egui::Color32::from_rgb(255, 255, 255)))
            .show(ctx, |ui| {
                self.plot.keybinds(ui);
                let rect = ctx.available_rect();
                self.plot
                    .set_screen(rect.width() as f64, rect.height() as f64, true);
                if let Some(n) = self.data.update(&mut self.plot) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Title(n))
                }
                self.plot.update(ctx, ui);
            });
    }
    #[cfg(any(feature = "skia", feature = "tiny-skia"))]
    pub(crate) fn main(&mut self, width: u32, height: u32) {
        let mut b = false;
        if let Some(buffer) = &mut self.surface_state {
            self.plot.keybinds(&self.input_state);
            self.plot.set_screen(width as f64, height as f64, true);
            if let Some(n) = self.data.update(&mut self.plot) {
                b = true;
                self.name = n;
            };
            let mut buffer = buffer.buffer_mut().unwrap();
            self.plot.update(width, height, &mut buffer);
            buffer.present().unwrap();
        }
        if b {
            if let Some(w) = &self.surface_state {
                w.window().set_title(&self.name)
            }
        }
    }
}
