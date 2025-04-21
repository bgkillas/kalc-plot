use kalc_lib::complex::NumStr;
use kalc_lib::complex::NumStr::{Num, Vector};
use kalc_lib::load_vars::{get_vars, set_commands_or_vars};
use kalc_lib::math::do_math;
use kalc_lib::misc::{place_funcvar, place_var};
use kalc_lib::options::silent_commands;
use kalc_lib::parse::simplify;
use kalc_lib::units::{Colors, HowGraphing, Number, Options};
use rayon::iter::ParallelIterator;
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator};
use rupl::types::{Complex, Graph, GraphType, Prec, UpdateResult};
use std::env::args;
use std::process::exit;
fn main() {
    if let Some(function) = args().next_back() {
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
                    Ok(Box::new(App::new(function)))
                }),
            )
            .unwrap();
        }
        #[cfg(feature = "skia")]
        {
            let event_loop = winit::event_loop::EventLoop::new().unwrap();
            let mut app = App::new(function);
            event_loop.run_app(&mut app).unwrap()
        }
    }
}

struct App {
    plot: Graph,
    data: Data,
    #[cfg(feature = "skia")]
    surface_state: Option<
        softbuffer::Surface<std::rc::Rc<winit::window::Window>, std::rc::Rc<winit::window::Window>>,
    >,
    #[cfg(feature = "skia")]
    modifiers: rupl::types::Modifiers,
    #[cfg(feature = "skia")]
    input_state: rupl::types::InputState,
    #[cfg(feature = "skia")]
    name: String,
}

enum Type {
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
}

#[cfg(feature = "egui")]
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.main(ctx);
    }
}

#[cfg(feature = "skia")]
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
                self.main(width, height);
                self.input_state.reset();
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
            winit::event::WindowEvent::CursorMoved { position, .. } => {
                let Some(s) = &mut self.surface_state else {
                    return;
                };
                if s.window().id() != window {
                    return;
                }
                if self.input_state.pointer_down {
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
                self.modifiers.alt = modifiers.state().alt_key();
                self.modifiers.ctrl = modifiers.state().control_key();
                self.modifiers.shift = modifiers.state().shift_key();
                self.modifiers.command = modifiers.state().super_key();
            }
            winit::event::WindowEvent::PanGesture { delta, .. } => {
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
                if let Some(multi) = &mut self.input_state.multi {
                    multi.zoom_delta = zoom_delta
                } else {
                    self.input_state.multi = Some(rupl::types::Multi {
                        zoom_delta,
                        translation_delta: rupl::types::Vec2::splat(0.0),
                    })
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
    fn new(function: String) -> Self {
        let mut options = Options {
            prec: 128,
            graphing: true,
            ..Options::default()
        };
        let (data, graphing_mode) = init(&function, &mut options);
        if !graphing_mode.graph {
            println!("no graph");
            exit(1)
        }
        let data = Data { data, options };
        let (graph, complex) = if graphing_mode.y {
            data.generate_3d(-2.0, -2.0, 2.0, 2.0, 64, 64)
        } else {
            data.generate_2d(-2.0, 2.0, 256)
        };
        let mut plot = Graph::new(graph, complex, -2.0, 2.0);
        plot.is_complex = complex;
        plot.mult = 1.0 / 16.0;
        Self {
            plot,
            data,
            #[cfg(feature = "skia")]
            surface_state: None,
            #[cfg(feature = "skia")]
            modifiers: rupl::types::Modifiers::default(),
            #[cfg(feature = "skia")]
            input_state: rupl::types::InputState::default(),
            #[cfg(feature = "skia")]
            name: function,
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
    #[cfg(feature = "skia")]
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
    fn update(&self, plot: &mut Graph) {
        match plot.update_res() {
            UpdateResult::Width(s, e, Prec::Mult(p)) => {
                plot.clear_data();
                let (data, complex) = self.generate_2d(s, e, (p * 512.0) as usize);
                plot.is_complex |= complex;
                plot.set_data(data);
            }
            UpdateResult::Width3D(sx, sy, ex, ey, p) => {
                plot.clear_data();
                let (data, complex) = match p {
                    Prec::Mult(p) => {
                        let l = (p * 64.0) as usize;
                        self.generate_3d(sx, sy, ex, ey, l, l)
                    }
                    Prec::Dimension(x, y) => self.generate_3d(sx, sy, ex, ey, x / 16, y / 16),
                    Prec::Slice(p, view_x, slice) => {
                        let l = (p * 512.0) as usize;
                        self.generate_3d_slice(sx, sy, ex, ey, l, l, slice, view_x)
                    }
                };
                plot.is_complex |= complex;
                plot.set_data(data);
            }
            UpdateResult::Width(_, _, _) => unreachable!(),
            UpdateResult::None => {}
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
            .map(|data| match data.graph_type {
                Type::Num => {
                    let data = (0..=leny)
                        .into_par_iter()
                        .flat_map(|j| {
                            let y = starty + j as f64 * dy;
                            let y = Num(Number::from(
                                rug::Complex::with_val(self.options.prec, y),
                                None,
                            ));
                            let mut modified = place_var(data.func.clone(), "y", y.clone());
                            let mut modifiedvars = place_funcvar(data.funcvar.clone(), "y", y);
                            simplify(&mut modified, &mut modifiedvars, self.options);
                            let mut data = Vec::with_capacity(lenx + 1);
                            for i in 0..=lenx {
                                let x = startx + i as f64 * dx;
                                let x = Num(Number::from(
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
                                        println!("inconsistent data type 1");
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
                Type::Vector => {
                    let data = (0..=leny)
                        .into_par_iter()
                        .flat_map(|j| {
                            let y = starty + j as f64 * dy;
                            let y = Num(Number::from(
                                rug::Complex::with_val(self.options.prec, y),
                                None,
                            ));
                            let mut modified = place_var(data.func.clone(), "y", y.clone());
                            let mut modifiedvars = place_funcvar(data.funcvar.clone(), "y", y);
                            simplify(&mut modified, &mut modifiedvars, self.options);
                            let mut data = Vec::with_capacity(lenx + 1);
                            for i in 0..=lenx {
                                let x = startx + i as f64 * dx;
                                let x = Num(Number::from(
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
                                            println!("inconsistent vector length 2");
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
                                        println!("data type 2");
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
                Type::Vector3D => {
                    let data = (0..=leny)
                        .into_par_iter()
                        .flat_map(|j| {
                            let y = starty + j as f64 * dy;
                            let y = Num(Number::from(
                                rug::Complex::with_val(self.options.prec, y),
                                None,
                            ));
                            let mut modified = place_var(data.func.clone(), "y", y.clone());
                            let mut modifiedvars = place_funcvar(data.funcvar.clone(), "y", y);
                            simplify(&mut modified, &mut modifiedvars, self.options);
                            let mut data = Vec::with_capacity(lenx + 1);
                            for i in 0..=lenx {
                                let x = startx + i as f64 * dx;
                                let x = Num(Number::from(
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
                                            println!("inconsistent vector length 3");
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
                                        println!("data type 3");
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
            let y = Num(Number::from(
                rug::Complex::with_val(self.options.prec, y),
                None,
            ));
            self.data
                .par_iter()
                .map(|data| {
                    let mut modified = place_var(data.func.clone(), "y", y.clone());
                    let mut modifiedvars = place_funcvar(data.funcvar.clone(), "y", y.clone());
                    simplify(&mut modified, &mut modifiedvars, self.options);
                    match data.graph_type {
                        Type::Num => {
                            let data = (0..=lenx)
                                .into_par_iter()
                                .map(|i| {
                                    let x = startx + i as f64 * dx;
                                    let x = Num(Number::from(
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
                                        println!("data type 4");
                                        exit(1)
                                    }
                                })
                                .collect::<Vec<Complex>>();
                            let (a, b) = compact(data);
                            (GraphType::Width3D(a, startx, starty, endx, endy), b)
                        }
                        Type::Vector => todo!(),
                        Type::Vector3D => todo!(),
                    }
                })
                .collect::<Vec<(GraphType, bool)>>()
        } else {
            let x = startx + (slice as f64 + lenx as f64 / 2.0) * dx;
            let x = Num(Number::from(
                rug::Complex::with_val(self.options.prec, x),
                None,
            ));
            self.data
                .par_iter()
                .map(|data| {
                    let mut modified = place_var(data.func.clone(), "x", x.clone());
                    let mut modifiedvars = place_funcvar(data.funcvar.clone(), "x", x.clone());
                    simplify(&mut modified, &mut modifiedvars, self.options);
                    match data.graph_type {
                        Type::Num => {
                            let data = (0..=leny)
                                .into_par_iter()
                                .map(|i| {
                                    let y = starty + i as f64 * dy;
                                    let y = Num(Number::from(
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
                                        println!("data type 5");
                                        exit(1)
                                    }
                                })
                                .collect::<Vec<Complex>>();
                            let (a, b) = compact(data);
                            (GraphType::Width3D(a, startx, starty, endx, endy), b)
                        }
                        Type::Vector => todo!(),
                        Type::Vector3D => todo!(),
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
            .map(|data| match data.graph_type {
                Type::Num => {
                    let data = (0..=len)
                        .into_par_iter()
                        .map(|i| {
                            let x = start + i as f64 * dx;
                            let x = Num(Number::from(
                                rug::Complex::with_val(self.options.prec, x),
                                None,
                            ));
                            if let Ok(Num(n)) = do_math(
                                place_var(data.func.clone(), "x", x.clone()),
                                self.options,
                                place_funcvar(data.funcvar.clone(), "x", x),
                            ) {
                                Complex::Complex(n.number.real().to_f64(), n.number.imag().to_f64())
                            } else {
                                println!("data type 6");
                                exit(1)
                            }
                        })
                        .collect::<Vec<Complex>>();
                    let (a, b) = compact(data);
                    (GraphType::Width(a, start, end), b)
                }
                Type::Vector => {
                    let data = (0..=len)
                        .into_par_iter()
                        .map(|i| {
                            let x = start + i as f64 * dx;
                            let x = Num(Number::from(
                                rug::Complex::with_val(self.options.prec, x),
                                None,
                            ));
                            if let Ok(Vector(n)) = do_math(
                                place_var(data.func.clone(), "x", x.clone()),
                                self.options,
                                place_funcvar(data.funcvar.clone(), "x", x),
                            ) {
                                if n.len() != 2 {
                                    println!("inconsistent vector length 7");
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
                                println!("data type 7");
                                exit(1)
                            }
                        })
                        .collect::<Vec<(f64, Complex)>>();
                    let (a, b) = compact_coord(data);
                    (GraphType::Coord(a), b)
                }
                Type::Vector3D => {
                    let data = (0..=len)
                        .into_par_iter()
                        .map(|i| {
                            let x = start + i as f64 * dx;
                            let x = Num(Number::from(
                                rug::Complex::with_val(self.options.prec, x),
                                None,
                            ));
                            if let Ok(Vector(n)) = do_math(
                                place_var(data.func.clone(), "x", x.clone()),
                                self.options,
                                place_funcvar(data.funcvar.clone(), "x", x),
                            ) {
                                if n.len() != 3 {
                                    println!("inconsistent vector length 8");
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
                                println!("data type 8");
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
fn init(function: &str, options: &mut Options) -> (Vec<Plot>, HowGraphing) {
    let mut vars = get_vars(*options);
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
                    if let Err(s) = set_commands_or_vars(
                        &mut Colors::default(),
                        options,
                        &mut vars,
                        &s.chars().collect::<Vec<char>>(),
                    ) {
                        println!("{s}");
                        exit(1)
                    }
                }
            }
        }
    }
    let data = function
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
                Ok((func, funcvar, how, _, _)) => (func, funcvar, how),
                Err(s) => {
                    println!("{s}");
                    exit(1)
                }
            }
        })
        .collect::<Vec<(Vec<NumStr>, Vec<(String, Vec<NumStr>)>, HowGraphing)>>();
    let how = data[0].2;
    if !how.graph {
        println!("no graph 2");
        exit(1) //TODO
    }
    (
        data.into_iter()
            .map(|(func, funcvar, _)| {
                let x = Num(Number::from(rug::Complex::new(options.prec), None));
                let graph_type = match do_math(
                    place_var(place_var(func.clone(), "x", x.clone()), "y", x.clone()),
                    *options,
                    place_funcvar(place_funcvar(funcvar.clone(), "x", x.clone()), "y", x),
                ) {
                    Ok(Num(_)) => Type::Num,
                    Ok(Vector(v)) if v.len() == 2 => Type::Vector,
                    Ok(Vector(v)) if v.len() == 3 => Type::Vector3D,
                    Ok(_) => {
                        println!("bad output");
                        exit(1)
                    }
                    Err(s) => {
                        println!("{s}");
                        exit(1)
                    }
                };
                Plot {
                    func,
                    funcvar,
                    graph_type,
                }
            })
            .collect(),
        how,
    )
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
