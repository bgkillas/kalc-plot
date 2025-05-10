use crate::get_names;
use kalc_lib::complex::NumStr;
use kalc_lib::complex::NumStr::{Matrix, Num, Vector};
use kalc_lib::load_vars::set_commands_or_vars;
use kalc_lib::math::do_math;
use kalc_lib::misc::{place_funcvar, place_var};
use kalc_lib::options::silent_commands;
use kalc_lib::parse::simplify;
use kalc_lib::units::{Colors, HowGraphing, Number, Options, Variable};
#[cfg(feature = "rayon")]
use rayon::iter::IntoParallelIterator;
#[cfg(feature = "rayon")]
use rayon::iter::ParallelIterator;
#[cfg(not(feature = "rayon"))]
use crate::IntoIter;
use rupl::types::{Bound, Complex, Graph, GraphType, Prec};
#[cfg(feature = "bincode")]
use serde::{Deserialize, Serialize};
#[cfg_attr(feature = "bincode", derive(Serialize, Deserialize))]
pub(crate) struct Type {
    pub(crate) val: Val,
    pub(crate) inv: bool,
}
#[cfg_attr(feature = "bincode", derive(Serialize, Deserialize))]
pub(crate) enum Mat {
    D2(Vec<rupl::types::Vec2>),
    D3(Vec<rupl::types::Vec3>),
}
#[cfg_attr(feature = "bincode", derive(Serialize, Deserialize))]
pub(crate) enum Val {
    Num(Option<Complex>),
    Vector(Option<rupl::types::Vec2>),
    Vector3D,
    Matrix(Option<Mat>),
    List,
}

#[cfg_attr(feature = "bincode", derive(Serialize, Deserialize))]
pub(crate) struct Plot {
    pub(crate) func: Vec<NumStr>,
    pub(crate) funcvar: Vec<(String, Vec<NumStr>)>,
    pub(crate) graph_type: Type,
}

#[cfg_attr(feature = "bincode", derive(Serialize, Deserialize))]
pub(crate) struct Data {
    pub(crate) data: Vec<Plot>,
    pub(crate) options: Options,
    pub(crate) vars: Vec<Variable>,
    pub(crate) blacklist: Vec<usize>,
}
impl Data {
    pub(crate) fn update(&mut self, plot: &mut Graph) -> Option<String> {
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
    pub(crate) fn generate_3d(
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
    pub(crate) fn generate_3d_slice(
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
    pub(crate) fn generate_2d(&self, start: f64, end: f64, len: usize) -> (Vec<GraphType>, bool) {
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
pub(crate) fn init(
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