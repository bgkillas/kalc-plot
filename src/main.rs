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

struct Plot {
    func: Vec<NumStr>,
    funcvar: Vec<(String, Vec<NumStr>)>,
}

struct Data {
    data: Vec<Plot>,
    options: Options,
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
        let (data, graphing_mode) = init(&function, options);
        if !graphing_mode.graph {
            exit(1)
        }
        let data = Data { data, options };
        let (graph, complex) = if graphing_mode.y {
            data.generate_3d(-2.0, -2.0, 2.0, 2.0, 64, 64)
        } else {
            data.generate_2d(-2.0, 2.0, 256)
        };
        let mut plot = Graph::new(
            if graphing_mode.y {
                graph
                    .into_iter()
                    .map(|graph| GraphType::Width3D(graph, -2.0, -2.0, 2.0, 2.0))
                    .collect()
            } else {
                graph
                    .into_iter()
                    .map(|graph| GraphType::Width(graph, -2.0, 2.0))
                    .collect()
            },
            complex,
            -2.0,
            2.0,
        );
        plot.is_complex = complex;
        plot.mult = 1.0 / 16.0;
        Self { plot, data }
    }
    fn main(&mut self, ctx: &Context) {
        match self.plot.update(ctx, false) {
            UpdateResult::Width(s, e, Prec::Mult(p)) => {
                self.plot.clear_data();
                let (plot, complex) = self.data.generate_2d(s, e, (p * 512.0) as usize);
                self.plot.is_complex |= complex;
                self.plot.set_data(
                    plot.into_iter()
                        .map(|plot| GraphType::Width(plot, s, e))
                        .collect(),
                );
                self.plot.update(ctx, true);
            }
            UpdateResult::Width3D(sx, sy, ex, ey, p) => {
                self.plot.clear_data();
                let (plot, complex) = match p {
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
                self.plot.is_complex |= complex;
                self.plot.set_data(
                    plot.into_iter()
                        .map(|plot| GraphType::Width3D(plot, sx, sy, ex, ey))
                        .collect(),
                );
                self.plot.update(ctx, true);
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
    ) -> (Vec<Vec<Complex>>, bool) {
        let dx = (endx - startx) / lenx as f64;
        let dy = (endy - starty) / leny as f64;
        let data = (0..self.data.len())
            .into_par_iter()
            .map(|k| {
                (0..=leny)
                    .into_par_iter()
                    .flat_map(|j| {
                        let y = starty + j as f64 * dy;
                        let y = Num(Number::from(
                            rug::Complex::with_val(self.options.prec, y),
                            None,
                        ));
                        let mut modified = place_var(self.data[k].func.clone(), "y", y.clone());
                        let mut modifiedvars = place_funcvar(self.data[k].funcvar.clone(), "y", y);
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
                                    Complex::Complex(0.0, 0.0)
                                },
                            )
                        }
                        data
                    })
                    .collect::<Vec<Complex>>()
            })
            .collect::<Vec<Vec<Complex>>>();
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
    ) -> (Vec<Vec<Complex>>, bool) {
        let dx = (endx - startx) / lenx as f64;
        let dy = (endy - starty) / leny as f64;
        let data = if view_x {
            let y = starty + slice as f64 * dy;
            let y = Num(Number::from(
                rug::Complex::with_val(self.options.prec, y),
                None,
            ));
            (0..self.data.len())
                .into_par_iter()
                .map(|k| {
                    let mut modified = place_var(self.data[k].func.clone(), "y", y.clone());
                    let mut modifiedvars =
                        place_funcvar(self.data[k].funcvar.clone(), "y", y.clone());
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
                })
                .collect()
        } else {
            let x = startx + slice as f64 * dx;
            let x = Num(Number::from(
                rug::Complex::with_val(self.options.prec, x),
                None,
            ));
            (0..self.data.len())
                .into_par_iter()
                .map(|k| {
                    let mut modified = place_var(self.data[k].func.clone(), "x", x.clone());
                    let mut modifiedvars =
                        place_funcvar(self.data[k].funcvar.clone(), "x", x.clone());
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
                })
                .collect()
        };
        compact(data)
    }
    fn generate_2d(&self, start: f64, end: f64, len: usize) -> (Vec<Vec<Complex>>, bool) {
        let len = len.min(67108864);
        let dx = (end - start) / len as f64;
        let data = (0..self.data.len())
            .into_par_iter()
            .map(|k| {
                (0..=len)
                    .into_par_iter()
                    .map(|i| {
                        let x = start + i as f64 * dx;
                        let x = Num(Number::from(
                            rug::Complex::with_val(self.options.prec, x),
                            None,
                        ));
                        if let Ok(Num(n)) = do_math(
                            place_var(self.data[k].func.clone(), "x", x.clone()),
                            self.options,
                            place_funcvar(self.data[k].funcvar.clone(), "x", x),
                        ) {
                            Complex::Complex(n.number.real().to_f64(), n.number.imag().to_f64())
                        } else {
                            Complex::Complex(0.0, 0.0)
                        }
                    })
                    .collect::<Vec<Complex>>()
            })
            .collect();
        compact(data)
    }
}
#[allow(clippy::type_complexity)]
fn init(function: &str, options: Options) -> (Vec<Plot>, HowGraphing) {
    let data = function
        .split('#')
        .collect::<Vec<&str>>()
        .into_par_iter()
        .map(|function| {
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
        })
        .collect::<Vec<(Vec<NumStr>, Vec<(String, Vec<NumStr>)>, HowGraphing)>>();
    let how = data[0].2;
    (
        data.into_iter()
            .map(|(func, funcvar, _)| Plot { func, funcvar })
            .collect(),
        how,
    )
}
fn compact(mut graph: Vec<Vec<Complex>>) -> (Vec<Vec<Complex>>, bool) {
    let complex = graph.iter().flatten().any(|a| {
        if let Complex::Complex(_, i) = a {
            i != &0.0
        } else {
            unreachable!()
        }
    });
    if !complex {
        graph = graph
            .into_iter()
            .map(|a| {
                a.into_iter()
                    .map(|a| Complex::Real(a.to_options().0.unwrap()))
                    .collect()
            })
            .collect()
    } else if graph.iter().flatten().all(|a| {
        if let Complex::Complex(r, _) = a {
            r == &0.0
        } else {
            unreachable!()
        }
    }) {
        graph = graph
            .into_iter()
            .map(|a| {
                a.into_iter()
                    .map(|a| Complex::Imag(a.to_options().1.unwrap()))
                    .collect()
            })
            .collect()
    }
    (graph, complex)
}
