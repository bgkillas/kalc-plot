#[cfg(not(feature = "rayon"))]
use crate::IntoIter;
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
use rupl::types::{Bound, Complex, Graph, GraphType, Name, Prec};
#[cfg(feature = "bincode")]
use serde::{Deserialize, Serialize};
#[cfg_attr(feature = "bincode", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub(crate) struct Type {
    pub(crate) val: Val,
    pub(crate) how: HowGraphing,
    pub(crate) inv: Option<bool>,
}
impl Type {
    fn inv(&self) -> bool {
        if let Some(inv) = self.inv {
            inv
        } else {
            !self.how.x && self.how.y
        }
    }
    fn is_3d_i(&self) -> bool {
        self.how.x && self.how.y
    }
    fn is_3d_o(&self) -> bool {
        match &self.val {
            Val::Num(_) => self.is_3d_i(),
            Val::Vector(_) => false,
            Val::Vector3D => true,
            Val::Matrix(m) => m.is_3d(),
            Val::List => false,
        }
    }
    fn on_var(&self) -> bool {
        match self.val {
            Val::Num(_) => false,
            Val::Vector(_) => true,
            Val::Vector3D => true,
            Val::Matrix(_) => false,
            Val::List => false,
        }
    }
}
#[cfg_attr(feature = "bincode", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub(crate) enum Mat {
    D2(Vec<rupl::types::Vec2>),
    D3(Vec<rupl::types::Vec3>),
}
impl Mat {
    pub(crate) fn is_3d(&self) -> bool {
        matches!(self, Mat::D3(_))
    }
}
#[cfg_attr(feature = "bincode", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub(crate) enum Val {
    Num(Option<Complex>),
    Vector(Option<rupl::types::Vec2>),
    Vector3D,
    Matrix(Mat),
    List,
}

#[cfg_attr(feature = "bincode", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub(crate) struct Plot {
    pub(crate) func: Vec<NumStr>,
    pub(crate) funcvar: Vec<(String, Vec<NumStr>)>,
    pub(crate) graph_type: Type,
}

#[cfg_attr(feature = "bincode", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub(crate) struct Data {
    pub(crate) data: Vec<Option<Plot>>,
    pub(crate) options: Options,
    pub(crate) vars: Vec<Variable>,
    pub(crate) blacklist: Vec<usize>,
    pub(crate) var: rupl::types::Vec2,
}
impl Data {
    pub(crate) fn update(&mut self, plot: &mut Graph) -> Option<String> {
        let mut names = None;
        let mut ret = None;
        if let Some(name) = plot.update_res_name() {
            self.update_name(plot, &mut names, &mut ret, &name);
        }
        if let Some((bound, n)) = plot.update_res() {
            self.update_data(plot, names, bound, n);
        }
        ret
    }
    pub(crate) fn update_data(
        &mut self,
        plot: &mut Graph,
        names: Option<Vec<(Vec<String>, String)>>,
        bound: Bound,
        k: Option<usize>,
    ) {
        self.var = plot.var;
        self.blacklist = plot
            .blacklist_graphs
            .iter()
            .filter_map(|i| plot.index_to_name(*i, false).0)
            .collect();
        let n = k
            .into_iter()
            .filter_map(|i| match plot.index_to_name(i, false) {
                (Some(a), None) => Some(a),
                (None, Some((_, _))) => None, //TODO should not discard, but would have to update all after
                _ => None,
            })
            .next();
        let apply_names =
            |data: &[GraphType], complex: bool, plot: &mut Graph, k: Option<usize>| {
                if let Some(names) = names {
                    let names = get_names(data, names);
                    if let Some(k) = k {
                        plot.names[k].show = names[0].show;
                        plot.is_complex |= complex;
                    } else {
                        for (a, b) in plot
                            .names
                            .iter_mut()
                            .filter(|a| !a.name.is_empty())
                            .zip(names.iter())
                        {
                            a.show = b.show
                        }
                        plot.is_complex = complex;
                    }
                } else {
                    plot.is_complex |= complex;
                }
            };
        match bound {
            Bound::Width(s, e, Prec::Mult(p)) => {
                if n.is_none() {
                    plot.clear_data();
                }
                let (data, complex) =
                    self.generate_2d(s, e, (p * self.options.samples_2d as f64) as usize, n);
                apply_names(&data, complex, plot, n);
                plot.set_data(data, n);
            }
            Bound::Width3D(sx, sy, ex, ey, p) => {
                if n.is_none() {
                    plot.clear_data();
                }
                let (data, complex) = match p {
                    Prec::Mult(p) => {
                        let lx = (p * self.options.samples_3d.0 as f64) as usize;
                        let ly = (p * self.options.samples_3d.1 as f64) as usize;
                        self.generate_3d(sx, sy, ex, ey, lx, ly, n)
                    }
                    Prec::Dimension(x, y) => self.generate_3d(sx, sy, ex, ey, x, y, n),
                    Prec::Slice(p) => {
                        let l = (p * self.options.samples_2d as f64) as usize;
                        self.generate_2d_slice(sx, sy, ex, ey, l, l, plot.slice, plot.view_x, n)
                    }
                };
                apply_names(&data, complex, plot, n);
                plot.set_data(data, n);
            }
            Bound::Width(_, _, _) => unreachable!(),
        }
    }
    pub(crate) fn update_name(
        &mut self,
        plot: &mut Graph,
        names: &mut Option<Vec<(Vec<String>, String)>>,
        ret: &mut Option<String>,
        name: &[Name],
    ) {
        let mut i = 0;
        let mut func = Vec::with_capacity(name.len());
        for n in name {
            let mut v: Vec<String> = Vec::with_capacity(n.vars.len());
            for a in &n.vars {
                if !a.is_empty() && !plot.blacklist_graphs.contains(&i) {
                    v.push(a.clone())
                }
                i += 1;
            }
            if !n.name.is_empty() || !v.is_empty() {
                func.push(if v.is_empty() {
                    n.name.clone()
                } else {
                    format!("{};{}", v.join(";"), n.name)
                })
            }
            i += 1;
        }
        let func = func.join("#").replace(";#", ";");
        let new_name;
        (self.data, new_name, _) = init(&func, &mut self.options, self.vars.clone()).unwrap_or((
            Vec::new(),
            Vec::new(),
            HowGraphing::default(),
        ));
        if !new_name.is_empty() || name.is_empty() {
            *names = Some(new_name);
        }
        plot.set_is_3d(self.is_3d());
        *ret = Some(func);
    }
    pub(crate) fn is_3d(&self) -> bool {
        self.data.iter().any(|d| {
            d.iter()
                .map(|d| d.graph_type.is_3d_o())
                .next()
                .unwrap_or(false)
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn generate_3d(
        &self,
        startx: f64,
        starty: f64,
        endx: f64,
        endy: f64,
        lenx: usize,
        leny: usize,
        n: Option<usize>,
    ) -> (Vec<GraphType>, bool) {
        let data = if let Some(n) = n {
            n..n + 1
        } else {
            0..self.data.len()
        }
        .into_par_iter()
        .filter_map(|i| (!self.blacklist.contains(&i)).then(|| &self.data[i]))
        .filter_map(|data| {
            let Some(data) = data else { return None };
            if !data.graph_type.is_3d_o() {
                return None;
            }
            match (data.graph_type.is_3d_i(), data.graph_type.on_var()) {
                (true, false) => self.get_3d(data, startx, starty, endx, endy, lenx, leny),
                (true, true) => self.get_3d(
                    data, self.var.x, self.var.x, self.var.y, self.var.y, lenx, leny,
                ),
                (false, true) => self.get_2d(data, self.var.x, self.var.y, lenx * leny),
                (false, false) => None,
            }
        })
        .collect::<Vec<(GraphType, bool)>>();
        let complex = data.iter().any(|(_, b)| *b);
        (data.into_iter().map(|(a, _)| a).collect(), complex)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn get_3d(
        &self,
        data: &Plot,
        startx: f64,
        starty: f64,
        endx: f64,
        endy: f64,
        lenx: usize,
        leny: usize,
    ) -> Option<(GraphType, bool)> {
        let dx = (endx - startx) / lenx as f64;
        let dy = (endy - starty) / leny as f64;
        let ret = match &data.graph_type.val {
            Val::Num(n) => {
                if let Some(c) = n {
                    (
                        GraphType::Constant(*c, data.graph_type.inv()),
                        matches!(c, Complex::Complex(_, _) | Complex::Imag(_)),
                    )
                } else {
                    let data = (0..=leny)
                        .into_par_iter()
                        .flat_map(|j| {
                            let y = starty + j as f64 * dy;
                            let y = NumStr::new(Number::from_f64(y, &self.options));
                            let mut modified = place_var(data.func.clone(), "y", y.clone());
                            let mut modifiedvars = place_funcvar(data.funcvar.clone(), "y", y);
                            simplify(&mut modified, &mut modifiedvars, self.options);
                            let mut data = Vec::with_capacity(lenx + 1);
                            for i in 0..=lenx {
                                let x = startx + i as f64 * dx;
                                let x = NumStr::new(Number::from_f64(x, &self.options));
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
                            let y = NumStr::new(Number::from_f64(y, &self.options));
                            let mut modified = place_var(data.func.clone(), "y", y.clone());
                            let mut modifiedvars = place_funcvar(data.funcvar.clone(), "y", y);
                            simplify(&mut modified, &mut modifiedvars, self.options);
                            let mut data = Vec::with_capacity(lenx + 1);
                            for i in 0..=lenx {
                                let x = startx + i as f64 * dx;
                                let x = NumStr::new(Number::from_f64(x, &self.options));
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
                        let y = NumStr::new(Number::from_f64(y, &self.options));
                        let mut modified = place_var(data.func.clone(), "y", y.clone());
                        let mut modifiedvars = place_funcvar(data.funcvar.clone(), "y", y);
                        simplify(&mut modified, &mut modifiedvars, self.options);
                        let mut data = Vec::with_capacity(lenx + 1);
                        for i in 0..=lenx {
                            let x = startx + i as f64 * dx;
                            let x = NumStr::new(Number::from_f64(x, &self.options));
                            data.push(
                                if let Ok(Vector(n)) = do_math(
                                    place_var(modified.clone(), "x", x.clone()),
                                    self.options,
                                    place_funcvar(modifiedvars.clone(), "x", x),
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
                let mut ndata: Vec<Vec<(f64, f64, Complex)>> =
                    Vec::with_capacity((leny + 1) * (lenx + 1));
                for j in 0..=leny {
                    let ys = starty + j as f64 * dy;
                    let y = NumStr::new(Number::from_f64(ys, &self.options));
                    let mut modified = place_var(data.func.clone(), "y", y.clone());
                    let mut modifiedvars = place_funcvar(data.funcvar.clone(), "y", y);
                    simplify(&mut modified, &mut modifiedvars, self.options);
                    for i in 0..=lenx {
                        let xs = startx + i as f64 * dx;
                        let x = NumStr::new(Number::from_f64(xs, &self.options));
                        if let Ok(Vector(v)) = do_math(
                            place_var(modified.clone(), "x", x.clone()),
                            self.options,
                            place_funcvar(modifiedvars.clone(), "x", x),
                        ) {
                            let mut v = v.into_iter();
                            ndata.extend(vec![Vec::new(); v.len().saturating_sub(ndata.len())]);
                            for data in ndata.iter_mut() {
                                let n = v
                                    .next()
                                    .map(|n| {
                                        (
                                            xs,
                                            ys,
                                            Complex::Complex(
                                                n.number.real().to_f64(),
                                                n.number.imag().to_f64(),
                                            ),
                                        )
                                    })
                                    .unwrap_or((
                                        f64::NAN,
                                        f64::NAN,
                                        Complex::Complex(f64::NAN, f64::NAN),
                                    ));
                                data.push(n)
                            }
                        }
                    }
                }
                let mut b = false;
                (
                    GraphType::List(
                        ndata
                            .into_iter()
                            .map(|data| {
                                let (a, c) = compact_coord3d(data);
                                b |= c;
                                GraphType::Coord3D(a)
                            })
                            .collect(),
                    ),
                    b,
                )
            }
            Val::Matrix(m) => {
                if let Mat::D3(m) = m {
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
        };
        Some(ret)
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn generate_2d_slice(
        &self,
        startx: f64,
        starty: f64,
        endx: f64,
        endy: f64,
        lenx: usize,
        leny: usize,
        slice: isize,
        view_x: bool,
        n: Option<usize>,
    ) -> (Vec<GraphType>, bool) {
        let data = self.get_2d_slice(startx, starty, endx, endy, slice, lenx, leny, view_x, n);
        let complex = data.iter().any(|(_, b)| *b);
        (data.into_iter().map(|(a, _)| a).collect(), complex)
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn get_2d_slice(
        &self,
        starx: f64,
        stary: f64,
        enx: f64,
        eny: f64,
        slice: isize,
        mut lenx: usize,
        mut leny: usize,
        view_x: bool,
        n: Option<usize>,
    ) -> Vec<(GraphType, bool)> {
        let (xstr, ystr, startx, starty, endx, endy) = if view_x {
            (lenx, leny) = (leny, lenx);
            ("y", "x", stary, starx, eny, enx)
        } else {
            ("x", "y", starx, stary, enx, eny)
        };
        let dx = (endx - startx) / lenx as f64;
        let dy = (endy - starty) / leny as f64;
        let xs = startx + (slice as f64 + lenx as f64 / 2.0) * dx;
        let x = NumStr::new(Number::from_f64(xs, &self.options));
        if let Some(n) = n {
            n..n + 1
        } else {
            0..self.data.len()
        }
        .into_par_iter()
        .filter_map(|i| {
            if self.blacklist.contains(&i) {
                None
            } else {
                Some(&self.data[i])
            }
        })
        .filter_map(|data| {
            let Some(data) = data else { return None };
            Some(if let Val::Num(Some(c)) = data.graph_type.val {
                (
                    GraphType::Constant(c, data.graph_type.inv()),
                    matches!(c, Complex::Complex(_, _) | Complex::Imag(_)),
                )
            } else {
                let mut modified = place_var(data.func.clone(), xstr, x.clone());
                let mut modifiedvars = place_funcvar(data.funcvar.clone(), xstr, x.clone());
                simplify(&mut modified, &mut modifiedvars, self.options);
                match &data.graph_type.val {
                    Val::Num(_) => {
                        let data = (0..=leny)
                            .into_par_iter()
                            .map(|i| {
                                let y = starty + i as f64 * dy;
                                let y = NumStr::new(Number::from_f64(y, &self.options));
                                if let Ok(Num(n)) = do_math(
                                    place_var(modified.clone(), ystr, y.clone()),
                                    self.options,
                                    place_funcvar(modifiedvars.clone(), ystr, y),
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
                        (GraphType::Width3D(a, starx, stary, enx, eny), b)
                    }
                    Val::Vector(_) => return None,
                    Val::Vector3D => return None,
                    Val::List => {
                        let mut ndata: Vec<Vec<(f64, f64, Complex)>> = Vec::with_capacity(leny + 1);
                        for i in 0..=leny {
                            let xv = starty + i as f64 * dx;
                            let x = NumStr::new(Number::from_f64(xv, &self.options));
                            if let Ok(Vector(v)) = do_math(
                                place_var(modified.clone(), ystr, x.clone()),
                                self.options,
                                place_funcvar(modifiedvars.clone(), ystr, x),
                            ) {
                                let mut v = v.into_iter();
                                ndata.extend(vec![Vec::new(); v.len().saturating_sub(ndata.len())]);
                                for data in ndata.iter_mut() {
                                    let n = v
                                        .next()
                                        .map(|n| {
                                            (
                                                xs,
                                                xv,
                                                Complex::Complex(
                                                    n.number.real().to_f64(),
                                                    n.number.imag().to_f64(),
                                                ),
                                            )
                                        })
                                        .unwrap_or((
                                            f64::NAN,
                                            f64::NAN,
                                            Complex::Complex(f64::NAN, f64::NAN),
                                        ));
                                    data.push(n)
                                }
                            }
                        }
                        let mut b = false;
                        (
                            GraphType::List(
                                ndata
                                    .into_iter()
                                    .map(|data| {
                                        let (a, c) = compact_coord3d(data);
                                        b |= c;
                                        GraphType::Coord3D(a)
                                    })
                                    .collect(),
                            ),
                            b,
                        )
                    }
                    Val::Matrix(m) => {
                        if let Mat::D2(m) = m {
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
    }
    pub(crate) fn generate_2d(
        &self,
        start: f64,
        end: f64,
        len: usize,
        n: Option<usize>,
    ) -> (Vec<GraphType>, bool) {
        let data: Vec<(GraphType, bool)> = if let Some(n) = n {
            n..n + 1
        } else {
            0..self.data.len()
        }
        .into_par_iter()
        .filter_map(|i| (!self.blacklist.contains(&i)).then(|| &self.data[i]))
        .filter_map(|data| {
            let Some(data) = data else {
                return None;
            };
            match (data.graph_type.is_3d_i(), data.graph_type.on_var()) {
                (true, true) => self.get_3d(
                    data,
                    self.var.x,
                    self.var.x,
                    self.var.y,
                    self.var.y,
                    len.isqrt(),
                    len.isqrt(),
                ),
                (false, true) => self.get_2d(data, self.var.x, self.var.y, len),
                (false, false) => self.get_2d(data, start, end, len),
                (true, false) => None,
            }
        })
        .collect();
        let complex = data.iter().any(|(_, b)| *b);
        (data.into_iter().map(|(a, _)| a).collect(), complex)
    }
    pub(crate) fn get_2d(
        &self,
        data: &Plot,
        start: f64,
        end: f64,
        len: usize,
    ) -> Option<(GraphType, bool)> {
        let dx = (end - start) / len as f64;
        let ret = match &data.graph_type.val {
            Val::Num(n) => {
                if let Some(c) = n {
                    (
                        GraphType::Constant(*c, data.graph_type.inv()),
                        matches!(c, Complex::Complex(_, _) | Complex::Imag(_)),
                    )
                } else if data.graph_type.inv() {
                    let data = (0..=len)
                        .into_par_iter()
                        .map(|i| {
                            let xv = start + i as f64 * dx;
                            let x = NumStr::new(Number::from_f64(xv, &self.options));
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
                            let x = NumStr::new(Number::from_f64(x, &self.options));
                            if let Ok(Num(n)) = do_math(
                                place_var(data.func.clone(), "x", x.clone()),
                                self.options,
                                place_funcvar(data.funcvar.clone(), "x", x),
                            ) {
                                Complex::Complex(n.number.real().to_f64(), n.number.imag().to_f64())
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
                            let x = NumStr::new(Number::from_f64(x, &self.options));
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
                        let x = NumStr::new(Number::from_f64(x, &self.options));
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
                if data.graph_type.inv() {
                    let mut ndata: Vec<Vec<(f64, Complex)>> = Vec::with_capacity(len + 1);
                    for i in 0..=len {
                        let xv = start + i as f64 * dx;
                        let x = NumStr::new(Number::from_f64(xv, &self.options));
                        if let Ok(Vector(v)) = do_math(
                            place_var(data.func.clone(), "y", x.clone()),
                            self.options,
                            place_funcvar(data.funcvar.clone(), "y", x),
                        ) {
                            let mut v = v.into_iter();
                            ndata.extend(vec![Vec::new(); v.len().saturating_sub(ndata.len())]);
                            for data in ndata.iter_mut() {
                                let n = v
                                    .next()
                                    .map(|n| (n.number.real().to_f64(), Complex::Real(xv)))
                                    .unwrap_or((f64::NAN, Complex::Real(f64::NAN)));
                                data.push(n)
                            }
                        }
                    }
                    (
                        GraphType::List(ndata.into_iter().map(GraphType::Coord).collect()),
                        false,
                    )
                } else {
                    let mut ndata: Vec<Vec<(f64, Complex)>> = Vec::new();
                    for i in 0..=len {
                        let xv = start + i as f64 * dx;
                        let x = NumStr::new(Number::from_f64(xv, &self.options));
                        if let Ok(Vector(v)) = do_math(
                            place_var(data.func.clone(), "x", x.clone()),
                            self.options,
                            place_funcvar(data.funcvar.clone(), "x", x),
                        ) {
                            let mut v = v.into_iter();
                            ndata.extend(vec![Vec::new(); v.len().saturating_sub(ndata.len())]);
                            for data in ndata.iter_mut() {
                                let n = v
                                    .next()
                                    .map(|n| {
                                        (
                                            xv,
                                            Complex::Complex(
                                                n.number.real().to_f64(),
                                                n.number.imag().to_f64(),
                                            ),
                                        )
                                    })
                                    .unwrap_or((f64::NAN, Complex::Complex(f64::NAN, f64::NAN)));
                                data.push(n)
                            }
                        }
                    }
                    let mut b = false;
                    (
                        GraphType::List(
                            ndata
                                .into_iter()
                                .map(|data| {
                                    let (a, c) = compact_coord(data);
                                    b |= c;
                                    GraphType::Coord(a)
                                })
                                .collect(),
                        ),
                        b,
                    )
                }
            }
            Val::Matrix(m) => {
                if let Mat::D2(m) = m {
                    (
                        GraphType::Coord(m.iter().map(|m| (m.x, Complex::Real(m.y))).collect()),
                        false,
                    )
                } else {
                    return None;
                }
            }
        };
        Some(ret)
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
) -> Result<(Vec<Option<Plot>>, Vec<(Vec<String>, String)>, HowGraphing), &'static str> {
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
            data.push(
                if let Ok((func, funcvar, how, _, _)) = kalc_lib::parse::input_var(
                    &format!("({})", if x || y { &function[2..] } else { &function }),
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
                    None,
                ) {
                    (function, func, funcvar, how, x)
                } else {
                    (
                        function.to_string(),
                        Vec::new(),
                        Vec::new(),
                        Default::default(),
                        x,
                    )
                },
            );
        }
        data
    } else {
        function
            .split('#')
            .collect::<Vec<&str>>()
            .into_par_iter()
            .map(|function| {
                let x = function.starts_with("x=");
                let y = function.starts_with("y=");
                match kalc_lib::parse::input_var(
                    &format!("({})", if x || y { &function[2..] } else { &function }),
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
                    None,
                ) {
                    Ok((func, funcvar, how, _, _)) => (function.to_string(), func, funcvar, how, x),
                    Err(_) => (
                        function.to_string(),
                        Vec::new(),
                        Vec::new(),
                        Default::default(),
                        x,
                    ),
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
    let (a, b): (Vec<Option<Plot>>, Vec<String>) = data
        .into_par_iter()
        .map(|(name, func, funcvar, how, b)| {
            let x = NumStr::new(Number::new(options));
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
                    val: Val::Num(Some(compact_constant(*c))),
                    how,
                    inv: Some(!b),
                },
                Ok(Num(_)) => Type {
                    val: Val::Num(None),
                    how,
                    inv: None,
                },
                Ok(Vector(_)) if is_list(&func, &funcvar) => Type {
                    val: Val::List,
                    how,
                    inv: None,
                },
                Ok(Vector(v)) if v.len() == 2 && !how.graph => Type {
                    val: Val::Vector(Some(rupl::types::Vec2::new(
                        v[0].number.real().to_f64(),
                        v[1].number.real().to_f64(),
                    ))),
                    how,
                    inv: None,
                },
                Ok(Vector(v)) if v.len() == 2 => Type {
                    val: Val::Vector(None),
                    how,
                    inv: None,
                },
                Ok(Vector(v)) if v.len() == 3 => Type {
                    val: Val::Vector3D,
                    how,
                    inv: None,
                },
                Ok(Matrix(m))
                    if !how.graph
                        && !m.is_empty()
                        && (m[0].len() == 2 || m[0].len() == 3)
                        && m.iter().all(|a| a.len() == m[0].len()) =>
                {
                    Type {
                        val: Val::Matrix(if m[0].len() == 2 {
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
                        }),
                        how,
                        inv: None,
                    }
                }
                Ok(_) | Err(_) => {
                    return (None, name);
                }
            };
            (
                Some(Plot {
                    func,
                    funcvar,
                    graph_type,
                }),
                name,
            )
        })
        .unzip();
    if a.iter().all(|data| {
        let Some(data) = data else { return true };
        matches!(data.graph_type.val, Val::Matrix(Mat::D3(_)))
    }) {
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
fn compact_constant(c: Number) -> Complex {
    match (
        c.real().is_zero() && c.real().is_finite(),
        c.imag().is_zero() && c.imag().is_finite(),
    ) {
        (true, true) => Complex::Real(0.0),
        (false, true) => Complex::Real(c.real().to_f64()),
        (true, false) => Complex::Imag(c.imag().to_f64()),
        (false, false) => Complex::Complex(c.real().to_f64(), c.imag().to_f64()),
    }
}
fn compact(mut graph: Vec<Complex>) -> (Vec<Complex>, bool) {
    let complex = graph.iter().any(|a| {
        if let Complex::Complex(_, i) = a {
            i != &0.0 && i.is_finite()
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
            r == &0.0 || !r.is_finite()
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
            i != &0.0 && i.is_finite()
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
            r == &0.0 || !r.is_finite()
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
            i != &0.0 && i.is_finite()
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
            r == &0.0 || !r.is_finite()
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
                    | "isolate"
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
                        | "isolate"
                ) =>
            {
                true
            }
            NumStr::PlusMinus => true,
            _ => false,
        })
    })
}
