use egui::{Context, FontData, FontDefinitions, FontFamily};
use kalc_lib::complex::NumStr;
use kalc_lib::complex::NumStr::Num;
use kalc_lib::math::do_math;
use kalc_lib::misc::{place_funcvar, place_var};
use kalc_lib::parse::simplify;
use kalc_lib::units::{HowGraphing, Number, Options};
use rayon::iter::IntoParallelIterator;
use rayon::iter::ParallelIterator;
use rupl::types::{Complex, Graph, GraphType, Prec, UpdateResult};
use std::env::args;
use std::process::exit;
fn main() {
    if let Some(function) = args().next_back() {
        eframe::run_native(
            "eplot",
            eframe::NativeOptions {
                ..Default::default()
            },
            Box::new(|cc| {
                let mut fonts = FontDefinitions::default();
                fonts.font_data.insert(
                    "notosans".to_owned(),
                    std::sync::Arc::new(FontData::from_static(include_bytes!("../notosans.ttf"))),
                );
                fonts
                    .families
                    .get_mut(&FontFamily::Proportional)
                    .unwrap()
                    .insert(0, "notosans".to_owned());
                fonts
                    .families
                    .get_mut(&FontFamily::Monospace)
                    .unwrap()
                    .insert(0, "notosans".to_owned());
                cc.egui_ctx.set_fonts(fonts);
                Ok(Box::new(App::new(function)))
            }),
        )
        .unwrap();
    }
}

struct App {
    plot: Graph,
    data: Data,
}

struct Data {
    parsed: Vec<NumStr>,
    parsed_vars: Vec<(String, Vec<NumStr>)>,
    options: Options,
    is_complex: bool,
}

impl eframe::App for App {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.main(ctx);
    }
}

impl App {
    fn new(function: String) -> Self {
        let options = Options {
            prec: 128,
            ..Options::default()
        };
        let (parsed, parsed_vars, graphing_mode) = init(&function, options);
        if !graphing_mode.graph {
            exit(1)
        }
        let mut data = Data {
            parsed,
            parsed_vars,
            options,
            is_complex: false,
        };
        let (graph, complex) = if graphing_mode.y {
            data.generate_3d(-2.0, -2.0, 2.0, 2.0, 64, 64)
        } else {
            data.generate_2d(-2.0, 2.0, 256)
        };
        data.is_complex = complex;
        let plot = Graph::new(
            vec![if graphing_mode.y {
                GraphType::Width3D(graph, -2.0, -2.0, 2.0, 2.0)
            } else {
                GraphType::Width(graph, -2.0, 2.0)
            }],
            complex,
            -2.0,
            2.0,
        );
        Self { plot, data }
    }
    fn main(&mut self, ctx: &Context) {
        match self.plot.update(ctx) {
            UpdateResult::Width(s, e, Prec::Mult(p)) => {
                self.plot.clear_data();
                let plot = self.data.generate_2d(s, e, (p * 512.0) as usize);
                self.data.is_complex |= plot.1;
                self.plot.set_complex(self.data.is_complex);
                self.plot.set_data(vec![GraphType::Width(plot.0, s, e)]);
            }
            UpdateResult::Width3D(sx, sy, ex, ey, p) => {
                self.plot.clear_data();
                let plot = match p {
                    Prec::Mult(p) => {
                        let l = (p * 64.0) as usize;
                        self.data.generate_3d(sx, sy, ex, ey, l, l)
                    }
                    Prec::Dimension(x, y) => self.data.generate_3d(sx, sy, ex, ey, x / 16, y / 16),
                    Prec::Slice(p, view_x, slice) => {
                        let l = (p * 512.0) as usize;
                        self.data
                            .generate_3d_slice(sx, sy, ex, ey, l, l, slice, view_x)
                    }
                };
                self.data.is_complex |= plot.1;
                self.plot.set_complex(self.data.is_complex);
                self.plot
                    .set_data(vec![GraphType::Width3D(plot.0, sx, sy, ex, ey)]);
            }
            UpdateResult::Width(_, _, _) => unreachable!(),
            UpdateResult::None => {}
        }
    }
}
impl Data {
    fn generate_3d(
        &self,
        startx: f64,
        starty: f64,
        endx: f64,
        endy: f64,
        lenx: usize,
        leny: usize,
    ) -> (Vec<Complex>, bool) {
        let dx = (endx - startx) / lenx as f64;
        let dy = (endy - starty) / leny as f64;
        let data = (0..=leny)
            .into_par_iter()
            .flat_map(|j| {
                let y = starty + j as f64 * dy;
                let y = Num(Number::from(
                    rug::Complex::with_val(self.options.prec, y),
                    None,
                ));
                let mut modified = place_var(self.parsed.clone(), "y", y.clone());
                let mut modifiedvars = place_funcvar(self.parsed_vars.clone(), "y", y);
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
                            Complex::Complex(n.number.real().to_f64(), n.number.imag().to_f64())
                        } else {
                            Complex::Complex(0.0, 0.0)
                        },
                    )
                }
                data
            })
            .collect::<Vec<Complex>>();
        compact(data)
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
    ) -> (Vec<Complex>, bool) {
        let dx = (endx - startx) / lenx as f64;
        let dy = (endy - starty) / leny as f64;
        let data = if view_x {
            let y = starty + slice as f64 * dy;
            let y = Num(Number::from(
                rug::Complex::with_val(self.options.prec, y),
                None,
            ));
            let mut modified = place_var(self.parsed.clone(), "y", y.clone());
            let mut modifiedvars = place_funcvar(self.parsed_vars.clone(), "y", y);
            simplify(&mut modified, &mut modifiedvars, self.options);
            (0..=lenx)
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
                        Complex::Complex(n.number.real().to_f64(), n.number.imag().to_f64())
                    } else {
                        Complex::Complex(0.0, 0.0)
                    }
                })
                .collect::<Vec<Complex>>()
        } else {
            let x = startx + slice as f64 * dx;
            let x = Num(Number::from(
                rug::Complex::with_val(self.options.prec, x),
                None,
            ));
            let mut modified = place_var(self.parsed.clone(), "x", x.clone());
            let mut modifiedvars = place_funcvar(self.parsed_vars.clone(), "x", x);
            simplify(&mut modified, &mut modifiedvars, self.options);
            (0..=leny)
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
                        Complex::Complex(n.number.real().to_f64(), n.number.imag().to_f64())
                    } else {
                        Complex::Complex(0.0, 0.0)
                    }
                })
                .collect::<Vec<Complex>>()
        };
        compact(data)
    }
    fn generate_2d(&self, start: f64, end: f64, len: usize) -> (Vec<Complex>, bool) {
        let len = len.min(67108864);
        let dx = (end - start) / len as f64;
        let data = (0..=len)
            .into_par_iter()
            .map(|i| {
                let x = start + i as f64 * dx;
                let x = Num(Number::from(
                    rug::Complex::with_val(self.options.prec, x),
                    None,
                ));
                if let Ok(Num(n)) = do_math(
                    place_var(self.parsed.clone(), "x", x.clone()),
                    self.options,
                    place_funcvar(self.parsed_vars.clone(), "x", x),
                ) {
                    Complex::Complex(n.number.real().to_f64(), n.number.imag().to_f64())
                } else {
                    Complex::Complex(0.0, 0.0)
                }
            })
            .collect::<Vec<Complex>>();
        compact(data)
    }
}
#[allow(clippy::type_complexity)]
fn init(
    function: &str,
    options: Options,
) -> (Vec<NumStr>, Vec<(String, Vec<NumStr>)>, HowGraphing) {
    let Ok((func, funcvar, how, _, _)) = kalc_lib::parse::input_var(
        function,
        &Vec::new(),
        &mut Vec::new(),
        &mut 0,
        options,
        false,
        0,
        Vec::new(),
        false,
        &mut Vec::new(),
        None,
    ) else {
        exit(1)
    };
    (func, funcvar, how)
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
