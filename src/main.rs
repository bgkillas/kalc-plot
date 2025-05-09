use kalc_lib::complex::NumStr;
use kalc_lib::complex::NumStr::{Matrix, Num, Vector};
use kalc_lib::load_vars::{get_vars, set_commands_or_vars};
use kalc_lib::math::do_math;
use kalc_lib::misc::{place_funcvar, place_var};
use kalc_lib::options::silent_commands;
use kalc_lib::parse::simplify;
use kalc_lib::units::{Colors, HowGraphing, Number, Options, Variable};
#[cfg(feature = "rayon")]
use rayon::iter::IntoParallelIterator;
#[cfg(feature = "rayon")]
use rayon::iter::ParallelIterator;
use rupl::types::{Bound, Complex, Graph, GraphType, Name, Prec, Show};
#[cfg(feature = "bincode")]
use serde::{Deserialize, Serialize};
use std::env::args;
#[cfg(any(feature = "skia", feature = "tiny-skia"))]
use std::io::Write;
//TODO {x/2, x^2} does not graph off of var
fn main() {
    let args = args().collect::<Vec<String>>();
    let s = String::new();
    let function = args.last().unwrap_or(&s);
    let data = if args.len() > 2 && args[1] == "-d" && cfg!(feature = "bincode") {
        #[cfg(feature = "bincode")]
        {
            let mut stdin = std::io::stdin().lock();
            let mut data: kalc_lib::units::Data =
                bincode::serde::decode_from_std_read(&mut stdin, bincode::config::standard())
                    .unwrap();
            data.options.prec = data.options.graph_prec;
            data
        }
        #[cfg(not(feature = "bincode"))]
        {
            unreachable!()
        }
    } else {
        let options = Options {
            prec: 128,
            graph_prec: 128,
            graphing: true,
            ..Options::default()
        };
        kalc_lib::units::Data {
            vars: get_vars(options),
            options,
            colors: Default::default(),
        }
    };
    #[cfg(feature = "egui")]
    {
        eframe::run_native(
            &function.clone(),
            eframe::NativeOptions {
                ..Default::default()
            },
            Box::new(|cc| {
                let mut fonts = egui::FontDefinitions::default();
                fonts.font_data.insert(
                    "notosans".to_owned(),
                    std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
                        "../notosans.ttf"
                    ))),
                );
                fonts
                    .families
                    .get_mut(&egui::FontFamily::Proportional)
                    .unwrap()
                    .insert(0, "notosans".to_owned());
                fonts
                    .families
                    .get_mut(&egui::FontFamily::Monospace)
                    .unwrap()
                    .insert(0, "notosans".to_owned());
                cc.egui_ctx.set_fonts(fonts);
                Ok(Box::new(App::new(function.to_string(), data)))
            }),
        )
        .unwrap();
    }
    #[cfg(any(feature = "skia", feature = "tiny-skia"))]
    {
        let f = data.colors.graphtofile.clone();
        let (width, height) = data.options.window_size;
        let mut app = App::new(function.to_string(), data);
        if f.is_empty() {
            let event_loop = winit::event_loop::EventLoop::new().unwrap();
            event_loop.run_app(&mut app).unwrap()
        } else {
            app.plot.set_screen(width as f64, height as f64, true);
            app.plot.mult = 1.0;
            app.plot.disable_lines = true;
            app.plot.disable_axis = true;
            app.data.update(&mut app.plot);
            #[cfg(feature = "skia")]
            {
                let bytes = app.plot.get_png(width as u32, height as u32);
                if f == "-" {
                    std::io::stdout()
                        .lock()
                        .write_all(bytes.as_bytes())
                        .unwrap()
                } else {
                    std::fs::write(f, bytes.as_bytes()).unwrap()
                }
            }
            #[cfg(feature = "tiny-skia")]
            {
                let bytes = &app.plot.get_png(width as u32, height as u32);
                if f == "-" {
                    std::io::stdout().lock().write_all(&bytes).unwrap()
                } else {
                    std::fs::write(f, &bytes).unwrap()
                }
            }
        }
    }
}

#[cfg_attr(feature = "bincode", derive(Serialize, Deserialize))]
struct App {
    plot: Graph,
    data: Data,
    #[cfg(any(feature = "skia", feature = "tiny-skia"))]
    #[cfg_attr(feature = "bincode", serde(skip_serializing, skip_deserializing))]
    surface_state: Option<
        softbuffer::Surface<std::rc::Rc<winit::window::Window>, std::rc::Rc<winit::window::Window>>,
    >,
    #[cfg(any(feature = "skia", feature = "tiny-skia"))]
    #[cfg_attr(feature = "bincode", serde(skip_serializing, skip_deserializing))]
    input_state: rupl::types::InputState,
    #[cfg(any(feature = "skia", feature = "tiny-skia"))]
    name: String,
    #[cfg(any(feature = "skia", feature = "tiny-skia"))]
    touch_positions: std::collections::HashMap<u64, rupl::types::Vec2>,
    #[cfg(any(feature = "skia", feature = "tiny-skia"))]
    last_touch_positions: std::collections::HashMap<u64, rupl::types::Vec2>,
}
#[cfg_attr(feature = "bincode", derive(Serialize, Deserialize))]
struct Type {
    val: Val,
    inv: bool,
}
#[cfg_attr(feature = "bincode", derive(Serialize, Deserialize))]
enum Mat {
    D2(Vec<rupl::types::Vec2>),
    D3(Vec<rupl::types::Vec3>),
}
#[cfg_attr(feature = "bincode", derive(Serialize, Deserialize))]
enum Val {
    Num(Option<Complex>),
    Vector(Option<rupl::types::Vec2>),
    Vector3D,
    Matrix(Option<Mat>),
    List,
}

#[cfg_attr(feature = "bincode", derive(Serialize, Deserialize))]
struct Plot {
    func: Vec<NumStr>,
    funcvar: Vec<(String, Vec<NumStr>)>,
    graph_type: Type,
}

#[cfg_attr(feature = "bincode", derive(Serialize, Deserialize))]
struct Data {
    data: Vec<Plot>,
    options: Options,
    vars: Vec<Variable>,
    blacklist: Vec<usize>,
}

#[cfg(feature = "egui")]
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.main(ctx);
    }
}

#[cfg(any(feature = "skia", feature = "tiny-skia"))]
impl winit::application::ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let window = {
            let window = event_loop.create_window(winit::window::Window::default_attributes());
            std::rc::Rc::new(window.unwrap())
        };
        window.set_title(&self.name);
        let context = softbuffer::Context::new(window.clone()).unwrap();
        self.surface_state = Some(softbuffer::Surface::new(&context, window.clone()).unwrap())
    }
    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        match event {
            winit::event::WindowEvent::RedrawRequested => {
                let Some(state) = &mut self.surface_state else {
                    return;
                };
                if state.window().id() != window {
                    return;
                }
                let (width, height) = {
                    let size = state.window().inner_size();
                    (size.width, size.height)
                };
                state
                    .resize(
                        std::num::NonZeroU32::new(width).unwrap(),
                        std::num::NonZeroU32::new(height).unwrap(),
                    )
                    .unwrap();
                if self.touch_positions.len() > 1
                    && self.touch_positions.len() == self.last_touch_positions.len()
                {
                    fn avg(
                        vec: &std::collections::hash_map::Values<u64, rupl::types::Vec2>,
                    ) -> rupl::types::Vec2 {
                        vec.clone().copied().sum::<rupl::types::Vec2>() / (vec.len() as f64)
                    }
                    let cpos = avg(&self.touch_positions.values());
                    self.input_state.pointer_pos = Some(cpos);
                    let lpos = avg(&self.last_touch_positions.values());
                    let cdist = self
                        .touch_positions
                        .values()
                        .map(|v| (&cpos - v).norm())
                        .sum::<f64>();
                    let ldist = self
                        .last_touch_positions
                        .values()
                        .map(|v| (&lpos - v).norm())
                        .sum::<f64>();
                    let zoom_delta = if ldist != 0.0 { cdist / ldist } else { 0.0 };
                    let translation_delta = cpos - lpos;
                    self.input_state.multi = Some(rupl::types::Multi {
                        translation_delta,
                        zoom_delta,
                    })
                } else if self.touch_positions.len() == 1 {
                    self.input_state.pointer = Some(self.last_touch_positions.is_empty());
                    self.input_state.pointer_pos = self.touch_positions.values().next().copied();
                }
                self.main(width, height);
                self.input_state.reset();
                self.last_touch_positions = self.touch_positions.clone();
            }
            winit::event::WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            winit::event::WindowEvent::KeyboardInput { event, .. } => {
                if event.state.is_pressed() {
                    let Some(state) = &mut self.surface_state else {
                        return;
                    };
                    if state.window().id() != window {
                        return;
                    }
                    state.window().request_redraw();
                    self.input_state.keys_pressed.push(event.logical_key.into());
                }
            }
            winit::event::WindowEvent::MouseInput { state, button, .. } => match button {
                winit::event::MouseButton::Left => {
                    let Some(s) = &mut self.surface_state else {
                        return;
                    };
                    if s.window().id() != window {
                        return;
                    }
                    s.window().request_redraw();
                    self.input_state.pointer = state.is_pressed().then_some(true);
                }
                winit::event::MouseButton::Right => {
                    let Some(s) = &mut self.surface_state else {
                        return;
                    };
                    if s.window().id() != window {
                        return;
                    }
                    s.window().request_redraw();
                    self.input_state.pointer_right = state.is_pressed().then_some(true);
                }
                _ => {}
            },
            winit::event::WindowEvent::CursorEntered { .. } => {
                self.input_state.pointer = None;
                self.input_state.pointer_right = None;
            }
            winit::event::WindowEvent::CursorMoved { position, .. } => {
                let Some(s) = &mut self.surface_state else {
                    return;
                };
                if s.window().id() != window {
                    return;
                }
                if self.input_state.pointer.is_some()
                    || (self.input_state.pointer_right.is_some() && self.plot.draw_side)
                    || (!self.plot.is_3d
                        && (!self.plot.disable_coord
                            || self.plot.ruler_pos.is_some()
                            || self.plot.draw_side))
                {
                    s.window().request_redraw();
                }
                self.input_state.pointer_pos = Some(rupl::types::Vec2::new(position.x, position.y));
            }
            winit::event::WindowEvent::MouseWheel { delta, .. } => {
                let Some(s) = &mut self.surface_state else {
                    return;
                };
                if s.window().id() != window {
                    return;
                }
                s.window().request_redraw();
                self.input_state.raw_scroll_delta = match delta {
                    winit::event::MouseScrollDelta::LineDelta(x, y) => {
                        rupl::types::Vec2::new(x as f64 * 128.0, y as f64 * 128.0)
                    }
                    winit::event::MouseScrollDelta::PixelDelta(p) => {
                        rupl::types::Vec2::new(p.x, p.y)
                    }
                };
            }
            winit::event::WindowEvent::ModifiersChanged(modifiers) => {
                let Some(s) = &mut self.surface_state else {
                    return;
                };
                if s.window().id() != window {
                    return;
                }
                if !self.input_state.keys_pressed.is_empty() {
                    s.window().request_redraw();
                }
                self.input_state.modifiers.alt = modifiers.state().alt_key();
                self.input_state.modifiers.ctrl = modifiers.state().control_key();
                self.input_state.modifiers.shift = modifiers.state().shift_key();
                self.input_state.modifiers.command = modifiers.state().super_key();
            }
            winit::event::WindowEvent::PanGesture { delta, .. } => {
                let Some(s) = &mut self.surface_state else {
                    return;
                };
                if s.window().id() != window {
                    return;
                }
                s.window().request_redraw();
                let translation_delta = rupl::types::Vec2::new(delta.x as f64, delta.y as f64);
                if let Some(multi) = &mut self.input_state.multi {
                    multi.translation_delta = translation_delta
                } else {
                    self.input_state.multi = Some(rupl::types::Multi {
                        zoom_delta: 0.0,
                        translation_delta,
                    })
                }
            }
            winit::event::WindowEvent::PinchGesture {
                delta: zoom_delta, ..
            } => {
                let Some(s) = &mut self.surface_state else {
                    return;
                };
                if s.window().id() != window {
                    return;
                }
                s.window().request_redraw();
                if let Some(multi) = &mut self.input_state.multi {
                    multi.zoom_delta = zoom_delta
                } else {
                    self.input_state.multi = Some(rupl::types::Multi {
                        zoom_delta,
                        translation_delta: rupl::types::Vec2::splat(0.0),
                    })
                }
            }
            winit::event::WindowEvent::Touch(winit::event::Touch {
                location,
                phase,
                id,
                ..
            }) => {
                let Some(s) = &mut self.surface_state else {
                    return;
                };
                if s.window().id() != window {
                    return;
                }
                s.window().request_redraw();
                match phase {
                    winit::event::TouchPhase::Ended | winit::event::TouchPhase::Cancelled => {
                        self.input_state.pointer = None;
                        self.input_state.pointer_pos = None;
                        self.touch_positions.remove(&id);
                    }
                    winit::event::TouchPhase::Moved => {
                        self.touch_positions
                            .insert(id, rupl::types::Vec2::new(location.x, location.y));
                    }
                    winit::event::TouchPhase::Started => {
                        self.last_touch_positions.clear();
                        self.touch_positions
                            .insert(id, rupl::types::Vec2::new(location.x, location.y));
                    }
                }
            }
            _ => {}
        }
    }
    fn suspended(&mut self, _: &winit::event_loop::ActiveEventLoop) {
        self.surface_state = None
    }
}

fn get_names(graph: &[GraphType], names: Vec<(Vec<String>, String)>) -> Vec<Name> {
    names
        .into_iter()
        .zip(graph.iter())
        .map(|((vars, name), data)| {
            let (real, imag) = match data {
                GraphType::Width(data, _, _) => (
                    data.iter()
                        .any(|a| matches!(a, Complex::Real(_) | Complex::Complex(_, _))),
                    data.iter()
                        .any(|a| matches!(a, Complex::Imag(_) | Complex::Complex(_, _))),
                ),
                GraphType::Coord(data) => (
                    data.iter()
                        .any(|(_, a)| matches!(a, Complex::Real(_) | Complex::Complex(_, _))),
                    data.iter()
                        .any(|(_, a)| matches!(a, Complex::Imag(_) | Complex::Complex(_, _))),
                ),
                GraphType::Width3D(data, _, _, _, _) => (
                    data.iter()
                        .any(|a| matches!(a, Complex::Real(_) | Complex::Complex(_, _))),
                    data.iter()
                        .any(|a| matches!(a, Complex::Imag(_) | Complex::Complex(_, _))),
                ),
                GraphType::Coord3D(data) => (
                    data.iter()
                        .any(|(_, _, a)| matches!(a, Complex::Real(_) | Complex::Complex(_, _))),
                    data.iter()
                        .any(|(_, _, a)| matches!(a, Complex::Imag(_) | Complex::Complex(_, _))),
                ),
                GraphType::Constant(c, _) => (
                    matches!(c, Complex::Real(_) | Complex::Complex(_, _)),
                    matches!(c, Complex::Imag(_) | Complex::Complex(_, _)),
                ),
                GraphType::Point(_) => (true, false),
            };
            let show = if real && imag {
                Show::Complex
            } else if imag {
                Show::Imag
            } else {
                Show::Real
            };
            Name { name, show, vars }
        })
        .collect()
}

impl App {
    fn new(function: String, data: kalc_lib::units::Data) -> Self {
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
        plot.draw_side = side;
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
    fn main(&mut self, ctx: &egui::Context) {
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
    fn main(&mut self, width: u32, height: u32) {
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
impl Data {
    fn update(&mut self, plot: &mut Graph) -> Option<String> {
        let mut names = None;
        let mut ret = None;
        if let Some(name) = plot.update_res_name() {
            let func = name
                .iter()
                .filter_map(|n| {
                    let v: Vec<String> = n.vars.iter().filter(|a| !a.is_empty()).cloned().collect();
                    if n.name.is_empty() && v.is_empty() {
                        None
                    } else {
                        Some(if v.is_empty() {
                            n.name.clone()
                        } else {
                            format!("{};{}", v.join(";"), n.name)
                        })
                    }
                })
                .collect::<Vec<String>>()
                .join("#")
                .replace(";#", ";");
            let how;
            let new_name;
            (self.data, new_name, how) = init(&func, &mut self.options, self.vars.clone())
                .unwrap_or((Vec::new(), Vec::new(), HowGraphing::default()));
            if !new_name.is_empty() || name.is_empty() {
                names = Some(new_name);
            }
            plot.set_is_3d(how.x && how.y && how.graph);
            ret = Some(func);
        }
        if let (Some(bound), blacklist) = plot.update_res() {
            self.blacklist = blacklist;
            match bound {
                Bound::Width(s, e, Prec::Mult(p)) => {
                    plot.clear_data();
                    let (data, complex) =
                        self.generate_2d(s, e, (p * self.options.samples_2d as f64) as usize);
                    if let Some(names) = names {
                        let names = get_names(&data, names);
                        if names.len() == plot.names.len() {
                            for (a, b) in plot.names.iter_mut().zip(names.iter()) {
                                a.show = b.show
                            }
                        }
                        plot.is_complex = complex;
                    } else {
                        plot.is_complex |= complex;
                    }
                    plot.set_data(data);
                }
                Bound::Width3D(sx, sy, ex, ey, p) => {
                    plot.clear_data();
                    let (data, complex) = match p {
                        Prec::Mult(p) => {
                            let lx = (p * self.options.samples_3d.0 as f64) as usize;
                            let ly = (p * self.options.samples_3d.1 as f64) as usize;
                            self.generate_3d(sx, sy, ex, ey, lx, ly)
                        }
                        Prec::Dimension(x, y) => self.generate_3d(sx, sy, ex, ey, x, y),
                        Prec::Slice(p) => {
                            let l = (p * self.options.samples_2d as f64) as usize;
                            self.generate_3d_slice(sx, sy, ex, ey, l, l, plot.slice, plot.view_x)
                        }
                    };
                    if let Some(names) = names {
                        let names = get_names(&data, names);
                        if names.len() == plot.names.len() {
                            for (a, b) in plot.names.iter_mut().zip(names.iter()) {
                                a.show = b.show
                            }
                        }
                        plot.is_complex = complex;
                    } else {
                        plot.is_complex |= complex;
                    }
                    plot.set_data(data);
                }
                Bound::Width(_, _, _) => unreachable!(),
            }
        }
        ret
    }
    fn generate_3d(
        &self,
        startx: f64,
        starty: f64,
        endx: f64,
        endy: f64,
        lenx: usize,
        leny: usize,
    ) -> (Vec<GraphType>, bool) {
        let dx = (endx - startx) / lenx as f64;
        let dy = (endy - starty) / leny as f64;
        let data = (0..self.data.len())
            .into_par_iter()
            .filter_map(|i| {
                if self.blacklist.contains(&i) {
                    None
                } else {
                    Some(&self.data[i])
                }
            })
            .filter_map(|data| {
                Some(match &data.graph_type.val {
                    Val::Num(n) => {
                        if let Some(c) = n {
                            (
                                GraphType::Constant(*c, data.graph_type.inv),
                                matches!(c, Complex::Complex(_, _) | Complex::Imag(_)),
                            )
                        } else {
                            let data = (0..=leny)
                                .into_par_iter()
                                .flat_map(|j| {
                                    let y = starty + j as f64 * dy;
                                    let y = NumStr::new(Number::from(
                                        rug::Complex::with_val(self.options.prec, y),
                                        None,
                                    ));
                                    let mut modified = place_var(data.func.clone(), "y", y.clone());
                                    let mut modifiedvars =
                                        place_funcvar(data.funcvar.clone(), "y", y);
                                    simplify(&mut modified, &mut modifiedvars, self.options);
                                    let mut data = Vec::with_capacity(lenx + 1);
                                    for i in 0..=lenx {
                                        let x = startx + i as f64 * dx;
                                        let x = NumStr::new(Number::from(
                                            rug::Complex::with_val(self.options.prec, x),
                                            None,
                                        ));
                                        data.push(
                                            if let Ok(Num(n)) = do_math(
                                                place_var(modified.clone(), "x", x.clone()),
                                                self.options,
                                                place_funcvar(modifiedvars.clone(), "x", x),
                                            ) {
                                                Complex::Complex(
                                                    n.number.real().to_f64(),
                                                    n.number.imag().to_f64(),
                                                )
                                            } else {
                                                Complex::Complex(f64::NAN, f64::NAN)
                                            },
                                        )
                                    }
                                    data
                                })
                                .collect::<Vec<Complex>>();
                            let (a, b) = compact(data);
                            (GraphType::Width3D(a, startx, starty, endx, endy), b)
                        }
                    }
                    Val::Vector(v) => {
                        if let Some(v) = v {
                            (GraphType::Point(*v), false)
                        } else {
                            let data = (0..=leny)
                                .into_par_iter()
                                .flat_map(|j| {
                                    let y = starty + j as f64 * dy;
                                    let y = NumStr::new(Number::from(
                                        rug::Complex::with_val(self.options.prec, y),
                                        None,
                                    ));
                                    let mut modified = place_var(data.func.clone(), "y", y.clone());
                                    let mut modifiedvars =
                                        place_funcvar(data.funcvar.clone(), "y", y);
                                    simplify(&mut modified, &mut modifiedvars, self.options);
                                    let mut data = Vec::with_capacity(lenx + 1);
                                    for i in 0..=lenx {
                                        let x = startx + i as f64 * dx;
                                        let x = NumStr::new(Number::from(
                                            rug::Complex::with_val(self.options.prec, x),
                                            None,
                                        ));
                                        data.push(
                                            if let Ok(Vector(n)) = do_math(
                                                place_var(modified.clone(), "x", x.clone()),
                                                self.options,
                                                place_funcvar(modifiedvars.clone(), "x", x),
                                            ) {
                                                if n.len() != 2 {
                                                    (f64::NAN, Complex::Complex(f64::NAN, f64::NAN))
                                                } else {
                                                    (
                                                        n[0].number.real().to_f64(),
                                                        Complex::Complex(
                                                            n[1].number.real().to_f64(),
                                                            n[1].number.real().to_f64(),
                                                        ),
                                                    )
                                                }
                                            } else {
                                                (f64::NAN, Complex::Complex(f64::NAN, f64::NAN))
                                            },
                                        )
                                    }
                                    data
                                })
                                .collect::<Vec<(f64, Complex)>>();
                            let (a, b) = compact_coord(data);
                            (GraphType::Coord(a), b)
                        }
                    }
                    Val::Vector3D => {
                        let data = (0..=leny)
                            .into_par_iter()
                            .flat_map(|j| {
                                let y = starty + j as f64 * dy;
                                let y = NumStr::new(Number::from(
                                    rug::Complex::with_val(self.options.prec, y),
                                    None,
                                ));
                                let mut modified = place_var(data.func.clone(), "y", y.clone());
                                let mut modifiedvars = place_funcvar(data.funcvar.clone(), "y", y);
                                simplify(&mut modified, &mut modifiedvars, self.options);
                                let mut data = Vec::with_capacity(lenx + 1);
                                for i in 0..=lenx {
                                    let x = startx + i as f64 * dx;
                                    let x = NumStr::new(Number::from(
                                        rug::Complex::with_val(self.options.prec, x),
                                        None,
                                    ));
                                    data.push(
                                        if let Ok(Vector(n)) = do_math(
                                            place_var(modified.clone(), "x", x.clone()),
                                            self.options,
                                            place_funcvar(modifiedvars.clone(), "x", x),
                                        ) {
                                            if n.len() != 3 {
                                                (
                                                    f64::NAN,
                                                    f64::NAN,
                                                    Complex::Complex(f64::NAN, f64::NAN),
                                                )
                                            } else {
                                                (
                                                    n[0].number.real().to_f64(),
                                                    n[1].number.real().to_f64(),
                                                    Complex::Complex(
                                                        n[2].number.real().to_f64(),
                                                        n[2].number.imag().to_f64(),
                                                    ),
                                                )
                                            }
                                        } else {
                                            (
                                                f64::NAN,
                                                f64::NAN,
                                                Complex::Complex(f64::NAN, f64::NAN),
                                            )
                                        },
                                    )
                                }
                                data
                            })
                            .collect::<Vec<(f64, f64, Complex)>>();
                        let (a, b) = compact_coord3d(data);
                        (GraphType::Coord3D(a), b)
                    }
                    Val::List => {
                        let data = (0..=leny)
                            .into_par_iter()
                            .flat_map(|j| {
                                let ys = starty + j as f64 * dy;
                                let y = NumStr::new(Number::from(
                                    rug::Complex::with_val(self.options.prec, ys),
                                    None,
                                ));
                                let mut modified = place_var(data.func.clone(), "y", y.clone());
                                let mut modifiedvars = place_funcvar(data.funcvar.clone(), "y", y);
                                simplify(&mut modified, &mut modifiedvars, self.options);
                                let mut data = Vec::with_capacity(lenx + 1);
                                for i in 0..=lenx {
                                    let xs = startx + i as f64 * dx;
                                    let x = NumStr::new(Number::from(
                                        rug::Complex::with_val(self.options.prec, xs),
                                        None,
                                    ));
                                    if let Ok(Vector(v)) = do_math(
                                        place_var(modified.clone(), "x", x.clone()),
                                        self.options,
                                        place_funcvar(modifiedvars.clone(), "x", x),
                                    ) {
                                        data.extend(v.iter().map(|n| {
                                            (
                                                xs,
                                                ys,
                                                Complex::Complex(
                                                    n.number.real().to_f64(),
                                                    n.number.imag().to_f64(),
                                                ),
                                            )
                                        }))
                                    } else {
                                        data.push((xs, ys, Complex::Complex(f64::NAN, f64::NAN)))
                                    }
                                }
                                data
                            })
                            .collect::<Vec<(f64, f64, Complex)>>();
                        let (a, b) = compact_coord3d(data);
                        (GraphType::Coord3D(a), b)
                    }
                    Val::Matrix(m) => {
                        if let Some(Mat::D3(m)) = m {
                            (
                                GraphType::Coord3D(
                                    m.iter().map(|m| (m.x, m.y, Complex::Real(m.z))).collect(),
                                ),
                                false,
                            )
                        } else {
                            return None;
                        }
                    }
                })
            })
            .collect::<Vec<(GraphType, bool)>>();
        let complex = data.iter().any(|(_, b)| *b);
        (data.into_iter().map(|(a, _)| a).collect(), complex)
    }
    #[allow(clippy::too_many_arguments)]
    fn generate_3d_slice(
        &self,
        startx: f64,
        starty: f64,
        endx: f64,
        endy: f64,
        lenx: usize,
        leny: usize,
        slice: isize,
        view_x: bool,
    ) -> (Vec<GraphType>, bool) {
        let dx = (endx - startx) / lenx as f64;
        let dy = (endy - starty) / leny as f64;
        let data = if view_x {
            let y = starty + (slice as f64 + leny as f64 / 2.0) * dy;
            let y = NumStr::new(Number::from(
                rug::Complex::with_val(self.options.prec, y),
                None,
            ));
            (0..self.data.len())
                .into_par_iter()
                .filter_map(|i| {
                    if self.blacklist.contains(&i) {
                        None
                    } else {
                        Some(&self.data[i])
                    }
                })
                .filter_map(|data| {
                    Some(if let Val::Num(Some(c)) = data.graph_type.val {
                        (
                            GraphType::Constant(c, data.graph_type.inv),
                            matches!(c, Complex::Complex(_, _) | Complex::Imag(_)),
                        )
                    } else {
                        let mut modified = place_var(data.func.clone(), "y", y.clone());
                        let mut modifiedvars = place_funcvar(data.funcvar.clone(), "y", y.clone());
                        simplify(&mut modified, &mut modifiedvars, self.options);
                        match &data.graph_type.val {
                            Val::Num(_) => {
                                let data = (0..=lenx)
                                    .into_par_iter()
                                    .map(|i| {
                                        let x = startx + i as f64 * dx;
                                        let x = NumStr::new(Number::from(
                                            rug::Complex::with_val(self.options.prec, x),
                                            None,
                                        ));
                                        if let Ok(Num(n)) = do_math(
                                            place_var(modified.clone(), "x", x.clone()),
                                            self.options,
                                            place_funcvar(modifiedvars.clone(), "x", x),
                                        ) {
                                            Complex::Complex(
                                                n.number.real().to_f64(),
                                                n.number.imag().to_f64(),
                                            )
                                        } else {
                                            Complex::Complex(f64::NAN, f64::NAN)
                                        }
                                    })
                                    .collect::<Vec<Complex>>();
                                let (a, b) = compact(data);
                                (GraphType::Width3D(a, startx, starty, endx, endy), b)
                            }
                            Val::Vector(_) => return None,
                            Val::Vector3D => return None,
                            Val::List => {
                                let data = (0..=lenx)
                                    .into_par_iter()
                                    .flat_map(|i| {
                                        let xv = startx + i as f64 * dx;
                                        let x = NumStr::new(Number::from(
                                            rug::Complex::with_val(self.options.prec, xv),
                                            None,
                                        ));
                                        if let Ok(Vector(v)) = do_math(
                                            place_var(data.func.clone(), "x", x.clone()),
                                            self.options,
                                            place_funcvar(data.funcvar.clone(), "x", x),
                                        ) {
                                            v.iter()
                                                .map(|n| {
                                                    (
                                                        xv,
                                                        Complex::Complex(
                                                            n.number.real().to_f64(),
                                                            n.number.imag().to_f64(),
                                                        ),
                                                    )
                                                })
                                                .collect()
                                        } else {
                                            vec![(f64::NAN, Complex::Complex(f64::NAN, f64::NAN))]
                                        }
                                    })
                                    .collect::<Vec<(f64, Complex)>>();
                                let (a, b) = compact_coord(data);
                                (GraphType::Coord(a), b)
                            }
                            Val::Matrix(m) => {
                                if let Some(Mat::D2(m)) = m {
                                    (
                                        GraphType::Coord(
                                            m.iter().map(|m| (m.x, Complex::Real(m.y))).collect(),
                                        ),
                                        false,
                                    )
                                } else {
                                    return None;
                                }
                            }
                        }
                    })
                })
                .collect::<Vec<(GraphType, bool)>>()
        } else {
            let x = startx + (slice as f64 + lenx as f64 / 2.0) * dx;
            let x = NumStr::new(Number::from(
                rug::Complex::with_val(self.options.prec, x),
                None,
            ));
            (0..self.data.len())
                .into_par_iter()
                .filter_map(|i| {
                    if self.blacklist.contains(&i) {
                        None
                    } else {
                        Some(&self.data[i])
                    }
                })
                .filter_map(|data| {
                    Some(if let Val::Num(Some(c)) = data.graph_type.val {
                        (
                            GraphType::Constant(c, data.graph_type.inv),
                            matches!(c, Complex::Complex(_, _) | Complex::Imag(_)),
                        )
                    } else {
                        let mut modified = place_var(data.func.clone(), "x", x.clone());
                        let mut modifiedvars = place_funcvar(data.funcvar.clone(), "x", x.clone());
                        simplify(&mut modified, &mut modifiedvars, self.options);
                        match &data.graph_type.val {
                            Val::Num(_) => {
                                let data = (0..=leny)
                                    .into_par_iter()
                                    .map(|i| {
                                        let y = starty + i as f64 * dy;
                                        let y = NumStr::new(Number::from(
                                            rug::Complex::with_val(self.options.prec, y),
                                            None,
                                        ));
                                        if let Ok(Num(n)) = do_math(
                                            place_var(modified.clone(), "y", y.clone()),
                                            self.options,
                                            place_funcvar(modifiedvars.clone(), "y", y),
                                        ) {
                                            Complex::Complex(
                                                n.number.real().to_f64(),
                                                n.number.imag().to_f64(),
                                            )
                                        } else {
                                            Complex::Complex(f64::NAN, f64::NAN)
                                        }
                                    })
                                    .collect::<Vec<Complex>>();
                                let (a, b) = compact(data);
                                (GraphType::Width3D(a, startx, starty, endx, endy), b)
                            }
                            Val::Vector(_) => return None,
                            Val::Vector3D => return None,
                            Val::List => {
                                let data = (0..=leny)
                                    .into_par_iter()
                                    .flat_map(|i| {
                                        let xv = starty + i as f64 * dx;
                                        let x = NumStr::new(Number::from(
                                            rug::Complex::with_val(self.options.prec, xv),
                                            None,
                                        ));
                                        if let Ok(Vector(v)) = do_math(
                                            place_var(data.func.clone(), "y", x.clone()),
                                            self.options,
                                            place_funcvar(data.funcvar.clone(), "y", x),
                                        ) {
                                            v.iter()
                                                .map(|n| {
                                                    (
                                                        xv,
                                                        Complex::Complex(
                                                            n.number.real().to_f64(),
                                                            n.number.imag().to_f64(),
                                                        ),
                                                    )
                                                })
                                                .collect()
                                        } else {
                                            vec![(f64::NAN, Complex::Complex(f64::NAN, f64::NAN))]
                                        }
                                    })
                                    .collect::<Vec<(f64, Complex)>>();
                                let (a, b) = compact_coord(data);
                                (GraphType::Coord(a), b)
                            }
                            Val::Matrix(m) => {
                                if let Some(Mat::D2(m)) = m {
                                    (
                                        GraphType::Coord(
                                            m.iter().map(|m| (m.x, Complex::Real(m.y))).collect(),
                                        ),
                                        false,
                                    )
                                } else {
                                    return None;
                                }
                            }
                        }
                    })
                })
                .collect::<Vec<(GraphType, bool)>>()
        };
        let complex = data.iter().any(|(_, b)| *b);
        (data.into_iter().map(|(a, _)| a).collect(), complex)
    }
    fn generate_2d(&self, start: f64, end: f64, len: usize) -> (Vec<GraphType>, bool) {
        let dx = (end - start) / len as f64;
        let data = (0..self.data.len())
            .into_par_iter()
            .filter_map(|i| {
                if self.blacklist.contains(&i) {
                    None
                } else {
                    Some(&self.data[i])
                }
            })
            .filter_map(|data| {
                Some(match &data.graph_type.val {
                    Val::Num(n) => {
                        if let Some(c) = n {
                            (
                                GraphType::Constant(*c, data.graph_type.inv),
                                matches!(c, Complex::Complex(_, _) | Complex::Imag(_)),
                            )
                        } else if data.graph_type.inv {
                            let data = (0..=len)
                                .into_par_iter()
                                .map(|i| {
                                    let xv = start + i as f64 * dx;
                                    let x = NumStr::new(Number::from(
                                        rug::Complex::with_val(self.options.prec, xv),
                                        None,
                                    ));
                                    if let Ok(Num(n)) = do_math(
                                        place_var(data.func.clone(), "y", x.clone()),
                                        self.options,
                                        place_funcvar(data.funcvar.clone(), "y", x),
                                    ) {
                                        (n.number.real().to_f64(), Complex::Complex(xv, 0.0))
                                    } else {
                                        (f64::NAN, Complex::Complex(f64::NAN, f64::NAN))
                                    }
                                })
                                .collect::<Vec<(f64, Complex)>>();
                            let (a, b) = compact_coord(data);
                            (GraphType::Coord(a), b)
                        } else {
                            let data = (0..=len)
                                .into_par_iter()
                                .map(|i| {
                                    let x = start + i as f64 * dx;
                                    let x = NumStr::new(Number::from(
                                        rug::Complex::with_val(self.options.prec, x),
                                        None,
                                    ));
                                    if let Ok(Num(n)) = do_math(
                                        place_var(data.func.clone(), "x", x.clone()),
                                        self.options,
                                        place_funcvar(data.funcvar.clone(), "x", x),
                                    ) {
                                        Complex::Complex(
                                            n.number.real().to_f64(),
                                            n.number.imag().to_f64(),
                                        )
                                    } else {
                                        Complex::Complex(f64::NAN, f64::NAN)
                                    }
                                })
                                .collect::<Vec<Complex>>();
                            let (a, b) = compact(data);
                            (GraphType::Width(a, start, end), b)
                        }
                    }
                    Val::Vector(v) => {
                        if let Some(v) = v {
                            (GraphType::Point(*v), false)
                        } else {
                            let data = (0..=len)
                                .into_par_iter()
                                .map(|i| {
                                    let x = start + i as f64 * dx;
                                    let x = NumStr::new(Number::from(
                                        rug::Complex::with_val(self.options.prec, x),
                                        None,
                                    ));
                                    if let Ok(Vector(n)) = do_math(
                                        place_var(data.func.clone(), "x", x.clone()),
                                        self.options,
                                        place_funcvar(data.funcvar.clone(), "x", x),
                                    ) {
                                        if n.len() != 2 {
                                            (f64::NAN, Complex::Complex(f64::NAN, f64::NAN))
                                        } else {
                                            (
                                                n[0].number.real().to_f64(),
                                                Complex::Complex(
                                                    n[1].number.real().to_f64(),
                                                    n[1].number.imag().to_f64(),
                                                ),
                                            )
                                        }
                                    } else {
                                        (f64::NAN, Complex::Complex(f64::NAN, f64::NAN))
                                    }
                                })
                                .collect::<Vec<(f64, Complex)>>();
                            let (a, b) = compact_coord(data);
                            (GraphType::Coord(a), b)
                        }
                    }
                    Val::Vector3D => {
                        let data = (0..=len)
                            .into_par_iter()
                            .map(|i| {
                                let x = start + i as f64 * dx;
                                let x = NumStr::new(Number::from(
                                    rug::Complex::with_val(self.options.prec, x),
                                    None,
                                ));
                                if let Ok(Vector(n)) = do_math(
                                    place_var(data.func.clone(), "x", x.clone()),
                                    self.options,
                                    place_funcvar(data.funcvar.clone(), "x", x),
                                ) {
                                    if n.len() != 3 {
                                        (f64::NAN, f64::NAN, Complex::Complex(f64::NAN, f64::NAN))
                                    } else {
                                        (
                                            n[0].number.real().to_f64(),
                                            n[1].number.real().to_f64(),
                                            Complex::Complex(
                                                n[2].number.real().to_f64(),
                                                n[2].number.imag().to_f64(),
                                            ),
                                        )
                                    }
                                } else {
                                    (f64::NAN, f64::NAN, Complex::Complex(f64::NAN, f64::NAN))
                                }
                            })
                            .collect::<Vec<(f64, f64, Complex)>>();
                        let (a, b) = compact_coord3d(data);
                        (GraphType::Coord3D(a), b)
                    }
                    Val::List => {
                        if data.graph_type.inv {
                            let data = (0..=len)
                                .into_par_iter()
                                .flat_map(|i| {
                                    let xv = start + i as f64 * dx;
                                    let x = NumStr::new(Number::from(
                                        rug::Complex::with_val(self.options.prec, xv),
                                        None,
                                    ));
                                    if let Ok(Vector(v)) = do_math(
                                        place_var(data.func.clone(), "y", x.clone()),
                                        self.options,
                                        place_funcvar(data.funcvar.clone(), "y", x),
                                    ) {
                                        v.iter()
                                            .map(|n| (n.number.real().to_f64(), Complex::Real(xv)))
                                            .collect()
                                    } else {
                                        vec![(f64::NAN, Complex::Complex(f64::NAN, f64::NAN))]
                                    }
                                })
                                .collect::<Vec<(f64, Complex)>>();
                            let (a, b) = compact_coord(data);
                            (GraphType::Coord(a), b)
                        } else {
                            let data = (0..=len)
                                .into_par_iter()
                                .flat_map(|i| {
                                    let xv = start + i as f64 * dx;
                                    let x = NumStr::new(Number::from(
                                        rug::Complex::with_val(self.options.prec, xv),
                                        None,
                                    ));
                                    if let Ok(Vector(v)) = do_math(
                                        place_var(data.func.clone(), "x", x.clone()),
                                        self.options,
                                        place_funcvar(data.funcvar.clone(), "x", x),
                                    ) {
                                        v.iter()
                                            .map(|n| {
                                                (
                                                    xv,
                                                    Complex::Complex(
                                                        n.number.real().to_f64(),
                                                        n.number.imag().to_f64(),
                                                    ),
                                                )
                                            })
                                            .collect()
                                    } else {
                                        vec![(f64::NAN, Complex::Complex(f64::NAN, f64::NAN))]
                                    }
                                })
                                .collect::<Vec<(f64, Complex)>>();
                            let (a, b) = compact_coord(data);
                            (GraphType::Coord(a), b)
                        }
                    }
                    Val::Matrix(m) => {
                        if let Some(Mat::D2(m)) = m {
                            (
                                GraphType::Coord(
                                    m.iter().map(|m| (m.x, Complex::Real(m.y))).collect(),
                                ),
                                false,
                            )
                        } else {
                            return None;
                        }
                    }
                })
            })
            .collect::<Vec<(GraphType, bool)>>();
        let complex = data.iter().any(|(_, b)| *b);
        (data.into_iter().map(|(a, _)| a).collect(), complex)
    }
}
fn take_vars(
    function: &mut String,
    options: &mut Options,
    vars: &mut Vec<Variable>,
) -> Vec<String> {
    let mut s = function
        .split('#')
        .map(|a| a.to_string())
        .collect::<Vec<String>>();
    let mut split = s
        .remove(0)
        .split(';')
        .map(|a| a.to_string())
        .collect::<Vec<String>>();
    *function = split.pop().unwrap();
    for s in &split {
        silent_commands(
            options,
            &s.chars()
                .filter(|&c| !c.is_whitespace())
                .collect::<Vec<char>>(),
        );
        if s.contains('=') {
            let _ = set_commands_or_vars(
                &mut Colors::default(),
                options,
                vars,
                &s.chars().collect::<Vec<char>>(),
            );
        }
    }
    if !s.is_empty() {
        *function = format!("{function}#{}", s.join("#"))
    }
    split
}
#[allow(clippy::type_complexity)]
fn init(
    function: &str,
    options: &mut Options,
    mut vars: Vec<Variable>,
) -> Result<(Vec<Plot>, Vec<(Vec<String>, String)>, HowGraphing), &'static str> {
    let mut function = function.to_string();
    let mut split = vec![take_vars(&mut function, options, &mut vars)];
    let data = if function.contains(';') {
        let mut data = Vec::new();
        let mut first = true;
        for mut function in function.split('#').map(|a| a.to_string()) {
            if !first {
                split.push(take_vars(&mut function, options, &mut vars));
            }
            first = false;
            let x = function.starts_with("x=");
            let y = function.starts_with("y=");
            if let Ok((func, funcvar, how, _, _)) = kalc_lib::parse::input_var(
                if x || y { &function[2..] } else { &function },
                &vars,
                &mut Vec::new(),
                &mut 0,
                *options,
                false,
                0,
                Vec::new(),
                false,
                &mut Vec::new(),
                None,
            ) {
                data.push((function, func, funcvar, how, x))
            }
        }
        data
    } else {
        function
            .split('#')
            .collect::<Vec<&str>>()
            .into_par_iter()
            .filter_map(|function| {
                let x = function.starts_with("x=");
                let y = function.starts_with("y=");
                match kalc_lib::parse::input_var(
                    if x || y { &function[2..] } else { function },
                    &vars,
                    &mut Vec::new(),
                    &mut 0,
                    *options,
                    false,
                    0,
                    Vec::new(),
                    false,
                    &mut Vec::new(),
                    None,
                ) {
                    Ok((func, funcvar, how, _, _)) => {
                        Some((function.to_string(), func, funcvar, how, x))
                    }
                    Err(_) => None,
                }
            })
            .collect::<Vec<(
                String,
                Vec<NumStr>,
                Vec<(String, Vec<NumStr>)>,
                HowGraphing,
                bool,
            )>>()
    };
    if data.is_empty() {
        return Err("no data");
    }
    let mut how = data
        .iter()
        .find_map(|d| if d.3.graph { Some(d.3) } else { None })
        .unwrap_or(data[0].3);
    let (a, b): (Vec<Plot>, Vec<String>) = data
        .into_par_iter()
        .filter(|(_, _, _, a, _)| (a.x && a.y) == (how.x && how.y) || !a.graph)
        .filter_map(|(name, func, funcvar, how, b)| {
            let x = NumStr::new(Number::from(rug::Complex::new(options.prec), None));
            let (f, fv) = match (how.x, how.y) {
                (true, true) => (
                    place_var(place_var(func.clone(), "x", x.clone()), "y", x.clone()),
                    place_funcvar(place_funcvar(funcvar.clone(), "x", x.clone()), "y", x),
                ),
                (true, false) => (
                    place_var(func.clone(), "x", x.clone()),
                    place_funcvar(funcvar.clone(), "x", x.clone()),
                ),
                (false, true) => (
                    place_var(func.clone(), "y", x.clone()),
                    place_funcvar(funcvar.clone(), "y", x.clone()),
                ),
                (false, false) => (func.clone(), funcvar.clone()),
            };
            let graph_type = match do_math(f, *options, fv) {
                Ok(Num(c)) if !how.graph => Type {
                    val: Val::Num(Some(compact_constant(c.number))),
                    inv: !b,
                },
                Ok(Num(_)) => Type {
                    val: Val::Num(None),
                    inv: how.y && !how.x,
                },
                Ok(Vector(_)) if is_list(&func, &funcvar) => Type {
                    val: Val::List,
                    inv: how.y && !how.x,
                },
                Ok(Vector(v)) if v.len() == 2 && !how.graph => Type {
                    val: Val::Vector(Some(rupl::types::Vec2::new(
                        v[0].number.real().to_f64(),
                        v[1].number.real().to_f64(),
                    ))),
                    inv: false,
                },
                Ok(Vector(v)) if v.len() == 2 => Type {
                    val: Val::Vector(None),
                    inv: how.y && !how.x,
                },
                Ok(Vector(v)) if v.len() == 3 => Type {
                    val: Val::Vector3D,
                    inv: how.y && !how.x,
                },
                Ok(Matrix(m))
                    if !how.graph
                        && !m.is_empty()
                        && (m[0].len() == 2 || m[0].len() == 3)
                        && m.iter().all(|a| a.len() == m[0].len()) =>
                {
                    Type {
                        val: Val::Matrix(Some(if m[0].len() == 2 {
                            Mat::D2(
                                m.iter()
                                    .map(|v| {
                                        rupl::types::Vec2::new(
                                            v[0].number.real().to_f64(),
                                            v[1].number.real().to_f64(),
                                        )
                                    })
                                    .collect(),
                            )
                        } else {
                            Mat::D3(
                                m.iter()
                                    .map(|v| {
                                        rupl::types::Vec3::new(
                                            v[0].number.real().to_f64(),
                                            v[1].number.real().to_f64(),
                                            v[2].number.real().to_f64(),
                                        )
                                    })
                                    .collect(),
                            )
                        })),
                        inv: false,
                    }
                }
                Ok(_) | Err(_) => {
                    return None;
                }
            };
            Some((
                Plot {
                    func,
                    funcvar,
                    graph_type,
                },
                name,
            ))
        })
        .unzip();
    if a.iter()
        .all(|a| matches!(a.graph_type.val, Val::Matrix(Some(Mat::D3(_)))))
    {
        how.x = true;
        how.y = true;
    };
    if b.is_empty() {
        return Err("no data2");
    }
    let mut v = Vec::with_capacity(b.len());
    for _ in split.len()..b.len() {
        split.push(Vec::new());
    }
    for (b, a) in b.iter().zip(split.into_iter()) {
        v.push((a, b.to_string()));
    }
    Ok((a, v, how))
}
fn compact_constant(c: rug::Complex) -> Complex {
    match (c.real().is_zero(), c.imag().is_zero()) {
        (true, true) => Complex::Real(0.0),
        (false, true) => Complex::Real(c.real().to_f64()),
        (true, false) => Complex::Imag(c.imag().to_f64()),
        (false, false) => Complex::Complex(c.real().to_f64(), c.imag().to_f64()),
    }
}
fn compact(mut graph: Vec<Complex>) -> (Vec<Complex>, bool) {
    let complex = graph.iter().any(|a| {
        if let Complex::Complex(_, i) = a {
            i != &0.0
        } else {
            unreachable!()
        }
    });
    if !complex {
        graph = graph
            .into_iter()
            .map(|a| Complex::Real(a.to_options().0.unwrap()))
            .collect()
    } else if graph.iter().all(|a| {
        if let Complex::Complex(r, _) = a {
            r == &0.0
        } else {
            unreachable!()
        }
    }) {
        graph = graph
            .into_iter()
            .map(|a| Complex::Imag(a.to_options().1.unwrap()))
            .collect()
    }
    (graph, complex)
}
fn compact_coord(mut graph: Vec<(f64, Complex)>) -> (Vec<(f64, Complex)>, bool) {
    let complex = graph.iter().any(|(_, a)| {
        if let Complex::Complex(_, i) = a {
            i != &0.0
        } else {
            unreachable!()
        }
    });
    if !complex {
        graph = graph
            .into_iter()
            .map(|(b, a)| (b, Complex::Real(a.to_options().0.unwrap())))
            .collect()
    } else if graph.iter().all(|(_, a)| {
        if let Complex::Complex(r, _) = a {
            r == &0.0
        } else {
            unreachable!()
        }
    }) {
        graph = graph
            .into_iter()
            .map(|(b, a)| (b, Complex::Imag(a.to_options().1.unwrap())))
            .collect()
    }
    (graph, complex)
}
fn compact_coord3d(mut graph: Vec<(f64, f64, Complex)>) -> (Vec<(f64, f64, Complex)>, bool) {
    let complex = graph.iter().any(|(_, _, a)| {
        if let Complex::Complex(_, i) = a {
            i != &0.0
        } else {
            unreachable!()
        }
    });
    if !complex {
        graph = graph
            .into_iter()
            .map(|(b, c, a)| (b, c, Complex::Real(a.to_options().0.unwrap())))
            .collect()
    } else if graph.iter().all(|(_, _, a)| {
        if let Complex::Complex(r, _) = a {
            r == &0.0
        } else {
            unreachable!()
        }
    }) {
        graph = graph
            .into_iter()
            .map(|(b, c, a)| (b, c, Complex::Imag(a.to_options().1.unwrap())))
            .collect()
    }
    (graph, complex)
}
fn is_list(func: &[NumStr], funcvar: &[(String, Vec<NumStr>)]) -> bool {
    func.iter().any(|c| match c {
        NumStr::Func(s)
            if matches!(
                s.as_str(),
                "cubic"
                    | "domain_coloring_rgb"
                    | "quadratic"
                    | "quad"
                    | "quartic"
                    | "unity"
                    | "solve"
            ) =>
        {
            true
        }
        NumStr::PlusMinus => true,
        _ => false,
    }) || funcvar.iter().any(|(_, c)| {
        c.iter().any(|c| match c {
            NumStr::Func(s)
                if matches!(
                    s.as_str(),
                    "cubic"
                        | "domain_coloring_rgb"
                        | "quadratic"
                        | "quad"
                        | "quartic"
                        | "unity"
                        | "solve"
                ) =>
            {
                true
            }
            NumStr::PlusMinus => true,
            _ => false,
        })
    })
}
#[cfg(not(feature = "rayon"))]
pub trait IntoIter<T: ?Sized> {
    fn into_par_iter(self) -> T;
}
#[cfg(not(feature = "rayon"))]
macro_rules! impl_into_iter {
    ($(($ty:ty, $b:ty)),*) => {
$(        impl IntoIter<$b> for $ty {
    fn into_par_iter(self)->$b {
        self.into_iter()
    }
})*
    };
}
#[cfg(not(feature = "rayon"))]
impl_into_iter!(
    (
        std::ops::RangeInclusive<usize>,
        std::ops::RangeInclusive<usize>
    ),
    (std::ops::Range<usize>, std::ops::Range<usize>)
);
#[cfg(not(feature = "rayon"))]
impl<'a> IntoIter<std::vec::IntoIter<&'a str>> for Vec<&'a str> {
    fn into_par_iter(self) -> std::vec::IntoIter<&'a str> {
        self.into_iter()
    }
}
#[cfg(not(feature = "rayon"))]
impl
    IntoIter<
        std::vec::IntoIter<(
            String,
            Vec<NumStr>,
            Vec<(String, Vec<NumStr>)>,
            HowGraphing,
            bool,
        )>,
    >
    for Vec<(
        String,
        Vec<NumStr>,
        Vec<(String, Vec<NumStr>)>,
        HowGraphing,
        bool,
    )>
{
    fn into_par_iter(
        self,
    ) -> std::vec::IntoIter<(
        String,
        Vec<NumStr>,
        Vec<(String, Vec<NumStr>)>,
        HowGraphing,
        bool,
    )> {
        self.into_iter()
    }
}
#[cfg(not(feature = "rayon"))]
pub trait Iter<'a, T> {
    fn par_iter(&'a self) -> T;
}
#[cfg(not(feature = "rayon"))]
macro_rules! impl_iter {
    ($(($ty:ty, $lt:lifetime, $b:ty)),*) => {
$(        impl<$lt> Iter<$lt, $b> for $ty {
    fn par_iter(&$lt self)->$b {
        self.iter()
    }
})*
    };
}
#[cfg(not(feature = "rayon"))]
impl_iter!((Vec<Plot>,'a,std::slice::Iter<'a,Plot>));
