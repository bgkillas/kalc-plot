use kalc_lib::complex::NumStr;
use kalc_lib::complex::NumStr::{Num, Vector};
use kalc_lib::load_vars::{get_vars, set_commands_or_vars};
use kalc_lib::math::do_math;
use kalc_lib::misc::{place_funcvar, place_var};
use kalc_lib::options::silent_commands;
use kalc_lib::parse::simplify;
use kalc_lib::units::{Colors, HowGraphing, Number, Options, Variable};
use rayon::iter::ParallelIterator;
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator};
use rupl::types::{Bound, Complex, Graph, GraphType, Name, Prec, Show};
use std::env::args;
use std::io::StdinLock;
#[cfg(any(feature = "skia", feature = "tiny-skia"))]
use std::io::Write;
use std::process::exit;
//TODO {x/2, x^2} does not graph off of var
fn main() {
    let args = args().collect::<Vec<String>>();
    if let Some(function) = args.last() {
        let data = if args.len() > 2 && args[1] == "-d" {
            let stdin = std::io::stdin().lock();
            let mut data =
                serde_json::from_reader::<StdinLock, kalc_lib::units::Data>(stdin).unwrap();
            data.options.prec = data.options.graph_prec;
            data
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
}

struct App {
    plot: Graph,
    data: Data,
    #[cfg(any(feature = "skia", feature = "tiny-skia"))]
    surface_state: Option<
        softbuffer::Surface<std::rc::Rc<winit::window::Window>, std::rc::Rc<winit::window::Window>>,
    >,
    #[cfg(any(feature = "skia", feature = "tiny-skia"))]
    input_state: rupl::types::InputState,
    #[cfg(any(feature = "skia", feature = "tiny-skia"))]
    name: String,
    #[cfg(any(feature = "skia", feature = "tiny-skia"))]
    touch_positions: std::collections::HashMap<u64, rupl::types::Vec2>,
    #[cfg(any(feature = "skia", feature = "tiny-skia"))]
    last_touch_positions: std::collections::HashMap<u64, rupl::types::Vec2>,
}
struct Type {
    val: Val,
    inv: bool,
}
enum Val {
    Num,
    Vector,
    Vector3D,
}

struct Plot {
    func: Vec<NumStr>,
    funcvar: Vec<(String, Vec<NumStr>)>,
    graph_type: Type,
}

struct Data {
    data: Vec<Plot>,
    options: Options,
    vars: Vec<Variable>,
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
                    self.input_state.pointer_down = true;
                    self.input_state.pointer_pos = self.touch_positions.values().next().copied();
                    self.input_state.pointer_just_down = self.last_touch_positions.is_empty();
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
            winit::event::WindowEvent::MouseInput { state, button, .. } => {
                if button == winit::event::MouseButton::Left {
                    let Some(s) = &mut self.surface_state else {
                        return;
                    };
                    if s.window().id() != window {
                        return;
                    }
                    s.window().request_redraw();
                    self.input_state.pointer_down = state.is_pressed();
                    if state.is_pressed() {
                        self.input_state.pointer_just_down = true
                    }
                }
            }
            winit::event::WindowEvent::CursorEntered { .. } => {
                self.input_state.pointer_down = false;
            }
            winit::event::WindowEvent::CursorMoved { position, .. } => {
                let Some(s) = &mut self.surface_state else {
                    return;
                };
                if s.window().id() != window {
                    return;
                }
                if self.input_state.pointer_down
                    || (!self.plot.is_3d
                        && (!self.plot.disable_coord || self.plot.ruler_pos.is_some()))
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
                        self.input_state.pointer_down = false;
                        self.input_state.pointer_pos = None;
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

impl App {
    fn new(function: String, data: kalc_lib::units::Data) -> Self {
        let kalc_lib::units::Data {
            mut options,
            vars,
            colors,
        } = data;
        let (data, names, graphing_mode) = init(&function, &mut options, vars.clone()).unwrap();
        let mut data = Data {
            data,
            options,
            vars,
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
        let names = names
            .into_iter()
            .zip(graph.iter())
            .map(|(name, data)| {
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
                        data.iter().any(|(_, _, a)| {
                            matches!(a, Complex::Real(_) | Complex::Complex(_, _))
                        }),
                        data.iter().any(|(_, _, a)| {
                            matches!(a, Complex::Imag(_) | Complex::Complex(_, _))
                        }),
                    ),
                };
                let show = if real && imag {
                    Show::Complex
                } else if imag {
                    Show::Imag
                } else {
                    Show::Real
                };
                Name {
                    name,
                    show,
                    vars: Vec::new(),
                }
            })
            .collect();
        if options.vxr.0 != 0.0 || options.vxr.1 != 0.0 {
            options.xr = options.vxr;
        }
        if options.vyr.0 != 0.0 || options.vyr.1 != 0.0 {
            options.yr = options.vyr;
        }
        let mut plot = Graph::new(graph, names, complex, options.xr.0, options.xr.1);
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
                self.data.update(&mut self.plot);
                self.plot.update(ctx, ui);
            });
    }
    #[cfg(any(feature = "skia", feature = "tiny-skia"))]
    fn main(&mut self, width: u32, height: u32) {
        if let Some(buffer) = &mut self.surface_state {
            let mut buffer = buffer.buffer_mut().unwrap();
            self.plot.keybinds(&self.input_state);
            self.data.update(&mut self.plot);
            self.plot.update(width, height, &mut buffer);
            buffer.present().unwrap();
        }
    }
}
impl Data {
    fn update(&mut self, plot: &mut Graph) {
        if let Some(name) = plot.update_res_name() {
            let func = name
                .iter()
                .map(|n| {
                    if n.vars.is_empty() {
                        n.name.clone()
                    } else {
                        format!("{};{}", n.vars.join(";"), n.name)
                    }
                })
                .collect::<Vec<String>>()
                .join("#");
            let how;
            (self.data, _, how) = init(&func, &mut self.options, self.vars.clone()).unwrap_or((
                Vec::new(),
                Vec::new(),
                HowGraphing::default(),
            ));
            plot.set_is_3d(how.x && how.y && how.graph)
        }
        if let Some(bound) = plot.update_res() {
            match bound {
                Bound::Width(s, e, Prec::Mult(p)) => {
                    plot.clear_data();
                    let (data, complex) =
                        self.generate_2d(s, e, (p * self.options.samples_2d as f64) as usize);
                    plot.is_complex |= complex;
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
                    plot.is_complex |= complex;
                    plot.set_data(data);
                }
                Bound::Width(_, _, _) => unreachable!(),
            }
        }
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
        let data = self
            .data
            .par_iter()
            .map(|data| match data.graph_type.val {
                Val::Num => {
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
                                        eprintln!("\x1b[Ginconsistent data type 1");
                                        exit(1)
                                    },
                                )
                            }
                            data
                        })
                        .collect::<Vec<Complex>>();
                    let (a, b) = compact(data);
                    (GraphType::Width3D(a, startx, starty, endx, endy), b)
                }
                Val::Vector => {
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
                                        if n.len() != 2 {
                                            eprintln!("\x1b[Ginconsistent vector length 2");
                                            exit(1)
                                        }
                                        (
                                            n[0].number.real().to_f64(),
                                            Complex::Complex(
                                                n[1].number.real().to_f64(),
                                                n[1].number.real().to_f64(),
                                            ),
                                        )
                                    } else {
                                        eprintln!("\x1b[Gdata type 2");
                                        exit(1)
                                    },
                                )
                            }
                            data
                        })
                        .collect::<Vec<(f64, Complex)>>();
                    let (a, b) = compact_coord(data);
                    (GraphType::Coord(a), b)
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
                                            eprintln!("\x1b[Ginconsistent vector length 3");
                                            exit(1)
                                        }
                                        (
                                            n[0].number.real().to_f64(),
                                            n[1].number.real().to_f64(),
                                            Complex::Complex(
                                                n[2].number.real().to_f64(),
                                                n[2].number.imag().to_f64(),
                                            ),
                                        )
                                    } else {
                                        eprintln!("\x1b[Gdata type 3");
                                        exit(1)
                                    },
                                )
                            }
                            data
                        })
                        .collect::<Vec<(f64, f64, Complex)>>();
                    let (a, b) = compact_coord3d(data);
                    (GraphType::Coord3D(a), b)
                }
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
            self.data
                .par_iter()
                .map(|data| {
                    let mut modified = place_var(data.func.clone(), "y", y.clone());
                    let mut modifiedvars = place_funcvar(data.funcvar.clone(), "y", y.clone());
                    simplify(&mut modified, &mut modifiedvars, self.options);
                    match data.graph_type.val {
                        Val::Num => {
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
                                        eprintln!("\x1b[Gdata type 4");
                                        exit(1)
                                    }
                                })
                                .collect::<Vec<Complex>>();
                            let (a, b) = compact(data);
                            (GraphType::Width3D(a, startx, starty, endx, endy), b)
                        }
                        Val::Vector => unreachable!(),
                        Val::Vector3D => unreachable!(),
                    }
                })
                .collect::<Vec<(GraphType, bool)>>()
        } else {
            let x = startx + (slice as f64 + lenx as f64 / 2.0) * dx;
            let x = NumStr::new(Number::from(
                rug::Complex::with_val(self.options.prec, x),
                None,
            ));
            self.data
                .par_iter()
                .map(|data| {
                    let mut modified = place_var(data.func.clone(), "x", x.clone());
                    let mut modifiedvars = place_funcvar(data.funcvar.clone(), "x", x.clone());
                    simplify(&mut modified, &mut modifiedvars, self.options);
                    match data.graph_type.val {
                        Val::Num => {
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
                                        eprintln!("\x1b[Gdata type 5");
                                        exit(1)
                                    }
                                })
                                .collect::<Vec<Complex>>();
                            let (a, b) = compact(data);
                            (GraphType::Width3D(a, startx, starty, endx, endy), b)
                        }
                        Val::Vector => unreachable!(),
                        Val::Vector3D => unreachable!(),
                    }
                })
                .collect::<Vec<(GraphType, bool)>>()
        };
        let complex = data.iter().any(|(_, b)| *b);
        (data.into_iter().map(|(a, _)| a).collect(), complex)
    }
    fn generate_2d(&self, start: f64, end: f64, len: usize) -> (Vec<GraphType>, bool) {
        let dx = (end - start) / len as f64;
        let data = self
            .data
            .par_iter()
            .map(|data| match data.graph_type.val {
                Val::Num => {
                    if data.graph_type.inv {
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
                                    eprintln!("\x1b[Gdata type 6i");
                                    exit(1)
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
                                    eprintln!("\x1b[Gdata type 6");
                                    exit(1)
                                }
                            })
                            .collect::<Vec<Complex>>();
                        let (a, b) = compact(data);
                        (GraphType::Width(a, start, end), b)
                    }
                }
                Val::Vector => {
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
                                    eprintln!("\x1b[Ginconsistent vector length 7");
                                    exit(1)
                                }
                                (
                                    n[0].number.real().to_f64(),
                                    Complex::Complex(
                                        n[1].number.real().to_f64(),
                                        n[1].number.imag().to_f64(),
                                    ),
                                )
                            } else {
                                eprintln!("\x1b[Gdata type 7");
                                exit(1)
                            }
                        })
                        .collect::<Vec<(f64, Complex)>>();
                    let (a, b) = compact_coord(data);
                    (GraphType::Coord(a), b)
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
                                    eprintln!("\x1b[Ginconsistent vector length 8");
                                    exit(1)
                                }
                                (
                                    n[0].number.real().to_f64(),
                                    n[1].number.real().to_f64(),
                                    Complex::Complex(
                                        n[2].number.real().to_f64(),
                                        n[2].number.imag().to_f64(),
                                    ),
                                )
                            } else {
                                eprintln!("\x1b[Gdata type 8");
                                exit(1)
                            }
                        })
                        .collect::<Vec<(f64, f64, Complex)>>();
                    let (a, b) = compact_coord3d(data);
                    (GraphType::Coord3D(a), b)
                }
            })
            .collect::<Vec<(GraphType, bool)>>();
        let complex = data.iter().any(|(_, b)| *b);
        (data.into_iter().map(|(a, _)| a).collect(), complex)
    }
}
#[allow(clippy::type_complexity)]
fn init(
    function: &str,
    options: &mut Options,
    mut vars: Vec<Variable>,
) -> Result<(Vec<Plot>, Vec<String>, HowGraphing), &'static str> {
    let mut function = function.to_string();
    {
        let mut split = function
            .split(';')
            .map(|a| a.to_string())
            .collect::<Vec<String>>();
        if split.len() != 1 {
            function = split.pop().unwrap();
            for s in split {
                silent_commands(
                    options,
                    &s.chars()
                        .filter(|&c| !c.is_whitespace())
                        .collect::<Vec<char>>(),
                );
                if s.contains('=') {
                    set_commands_or_vars(
                        &mut Colors::default(),
                        options,
                        &mut vars,
                        &s.chars().collect::<Vec<char>>(),
                    )?
                }
            }
        }
    }
    let dataerr =
        function
            .split('#')
            .collect::<Vec<&str>>()
            .into_par_iter()
            .map(|function| {
                match kalc_lib::parse::input_var(
                    function,
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
                        Ok((function.to_string(), func, funcvar, how))
                    }
                    Err(s) => Err(s),
                }
            })
            .collect::<Vec<
                Result<
                    (String, Vec<NumStr>, Vec<(String, Vec<NumStr>)>, HowGraphing),
                    &'static str,
                >,
            >>();
    let mut data = Vec::with_capacity(dataerr.len());
    for d in dataerr {
        data.push(d?)
    }
    let how = data[0].3;
    let (ae, be): (
        Vec<Result<Plot, &'static str>>,
        Vec<Result<String, &'static str>>,
    ) = data
        .into_iter()
        .map(|(name, func, funcvar, how)| {
            let x = NumStr::new(Number::from(rug::Complex::new(options.prec), None));
            let graph_type = match do_math(
                place_var(place_var(func.clone(), "x", x.clone()), "y", x.clone()),
                *options,
                place_funcvar(place_funcvar(funcvar.clone(), "x", x.clone()), "y", x),
            ) {
                Ok(Num(_)) => Type {
                    val: Val::Num,
                    inv: how.y && !how.x,
                },
                Ok(Vector(v)) if v.len() == 2 => Type {
                    val: Val::Vector,
                    inv: how.y && !how.x,
                },
                Ok(Vector(v)) if v.len() == 3 => Type {
                    val: Val::Vector3D,
                    inv: how.y && !how.x,
                },
                Ok(_) => {
                    return (Err("bad output"), Err("bad output"));
                }
                Err(s) => {
                    return (Err(s), Err(s));
                }
            };
            (
                Ok(Plot {
                    func,
                    funcvar,
                    graph_type,
                }),
                Ok(name),
            )
        })
        .unzip();
    let mut a = Vec::with_capacity(ae.len());
    let mut b = Vec::with_capacity(be.len());
    for (i, j) in ae.into_iter().zip(be.into_iter()) {
        a.push(i?);
        b.push(j?);
    }
    Ok((a, b, how))
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
