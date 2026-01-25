#![windows_subsystem = "windows"]
mod app;
mod data;
#[cfg(any(feature = "skia", feature = "tiny-skia", feature = "wasm-draw"))]
mod window;
use crate::data::Data;
#[cfg(feature = "kalc-lib")]
use kalc_lib::load_vars::get_vars;
#[cfg(feature = "kalc-lib")]
use kalc_lib::units::Options;
use rupl::types::{Complex, Graph, GraphType, Name, Show};
#[cfg(feature = "bincode")]
use serde::{Deserialize, Serialize};
use std::env::args;
#[cfg(feature = "kalc-lib")]
#[cfg(feature = "bincode")]
use std::io::Read;
#[cfg(feature = "kalc-lib")]
#[cfg(any(feature = "skia", feature = "tiny-skia"))]
use std::io::Write;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::wasm_bindgen;
#[cfg(feature = "wee")]
extern crate wee_alloc;
#[cfg(feature = "wee")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;
//pub type I = kalc_lib::rug::Integer;
//pub type F = kalc_lib::rug::Float;
//pub type C = kalc_lib::rug::Complex;
pub type I = kalc_lib::types::f64::Integer;
pub type F = kalc_lib::types::f64::Float;
pub type C = kalc_lib::types::f64::Complex;
#[cfg_attr(target_arch = "wasm32", wasm_bindgen(start))]
pub fn main() {
    #[cfg(feature = "wasm-console")]
    console_error_panic_hook::set_once();
    #[cfg(feature = "wasm-console")]
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    let mut args = args().collect::<Vec<String>>();
    if !args.is_empty() {
        args.remove(0);
    }
    let s = String::new();
    let function = args.last().unwrap_or(&s);
    #[cfg(feature = "kalc-lib")]
    let data = if args.len() > 1 && args[0] == "-d" && cfg!(feature = "bincode") {
        #[cfg(feature = "bincode")]
        {
            let mut stdin = std::io::stdin().lock();
            let len = &mut [0; 8];
            stdin.read_exact(len).unwrap();
            let mut data = Vec::with_capacity(usize::from_be_bytes(*len));
            stdin.read_to_end(&mut data).unwrap();
            let mut data: kalc_lib::units::Data<I, F, C> = bitcode::deserialize(&data).unwrap();
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
                #[cfg(feature = "kalc-lib")]
                let app = App::new(function.to_string(), data);
                #[cfg(not(feature = "kalc-lib"))]
                let app = App::new(function.to_string());
                Ok(Box::new(app))
            }),
        )
        .unwrap();
    }
    #[cfg(any(feature = "skia", feature = "tiny-skia", feature = "wasm-draw"))]
    {
        #[cfg(feature = "kalc-lib")]
        let f = data.colors.graphtofile.clone();
        #[cfg(feature = "kalc-lib")]
        let (width, height) = data.options.window_size;
        #[cfg(feature = "kalc-lib")]
        let mut app = App::new(function.to_string(), data);
        #[cfg(not(feature = "kalc-lib"))]
        let mut app = App::new(function.to_string());
        #[cfg(not(feature = "kalc-lib"))]
        {
            let event_loop = winit::event_loop::EventLoop::new().unwrap();
            event_loop.run_app(&mut app).unwrap()
        }
        #[cfg(feature = "kalc-lib")]
        if f.is_empty() {
            let event_loop = winit::event_loop::EventLoop::new().unwrap();
            event_loop.run_app(&mut app).unwrap()
        } else {
            app.plot.set_screen(width as f64, height as f64, true, true);
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
                    std::io::stdout()
                        .lock()
                        .write_all(bytes.as_bytes())
                        .unwrap()
                } else {
                    std::fs::write(f, bytes.as_bytes()).unwrap()
                }
            }
        }
    }
}

#[cfg_attr(feature = "bincode", derive(Serialize, Deserialize))]
struct App {
    plot: Graph,
    data: Data,
    #[cfg(feature = "bincode")]
    tiny: Option<rupl::types::GraphTiny>,
    #[cfg(any(feature = "skia", feature = "tiny-skia"))]
    #[cfg(not(feature = "skia-vulkan"))]
    #[cfg_attr(feature = "bincode", serde(skip))]
    #[cfg(not(feature = "wasm"))]
    surface_state: Option<
        softbuffer::Surface<
            std::sync::Arc<winit::window::Window>,
            std::sync::Arc<winit::window::Window>,
        >,
    >,
    #[cfg_attr(feature = "bincode", serde(skip))]
    #[cfg(feature = "wasm")]
    window: Option<winit::window::Window>,
    #[cfg(any(feature = "skia", feature = "tiny-skia", feature = "wasm-draw"))]
    #[cfg_attr(feature = "bincode", serde(skip))]
    input_state: rupl::types::InputState,
    #[cfg(any(feature = "skia", feature = "tiny-skia", feature = "wasm-draw"))]
    name: String,
    #[cfg(any(feature = "skia", feature = "tiny-skia", feature = "wasm-draw"))]
    touch_positions: std::collections::HashMap<u64, rupl::types::Vec2>,
    #[cfg(any(feature = "skia", feature = "tiny-skia", feature = "wasm-draw"))]
    last_touch_positions: std::collections::HashMap<u64, rupl::types::Vec2>,
    #[cfg(feature = "wasm")]
    dpr: f64,
}

#[cfg(feature = "egui")]
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.main(ctx);
    }
}

pub(crate) fn get_names(graph: &[GraphType], names: &[(Vec<String>, String)]) -> Vec<Name> {
    fn ri(data: &GraphType) -> (bool, bool) {
        match data {
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
            GraphType::List(d) => {
                let (mut a, mut b) = (false, false);
                for data in d {
                    let (c, d) = ri(data);
                    a |= c;
                    b |= d;
                }
                (a, b)
            }
            GraphType::None => (false, false),
        }
    }
    let mut graph = graph.iter();
    names
        .iter()
        .map(|(vars, name)| {
            let show = graph
                .next()
                .map(|data| {
                    let (real, imag) = ri(data);
                    if real && imag {
                        Show::Complex
                    } else if imag {
                        Show::Imag
                    } else {
                        Show::Real
                    }
                })
                .unwrap_or(Show::None);
            Name {
                name: name.to_string(),
                show,
                vars: vars.clone(),
            }
        })
        .collect()
}
#[cfg(not(feature = "rayon"))]
use crate::data::Plot;
#[cfg(not(feature = "rayon"))]
#[cfg(feature = "kalc-lib")]
use kalc_lib::complex::NumStr;
#[cfg(not(feature = "rayon"))]
#[cfg(feature = "kalc-lib")]
use kalc_lib::units::HowGraphing;
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
#[cfg(feature = "kalc-lib")]
impl
    IntoIter<
        std::vec::IntoIter<(
            String,
            Vec<NumStr<I, F, C>>,
            Vec<(String, Vec<NumStr<I, F, C>>)>,
            HowGraphing,
            bool,
        )>,
    >
    for Vec<(
        String,
        Vec<NumStr<I, F, C>>,
        Vec<(String, Vec<NumStr<I, F, C>>)>,
        HowGraphing,
        bool,
    )>
{
    fn into_par_iter(
        self,
    ) -> std::vec::IntoIter<(
        String,
        Vec<NumStr<I, F, C>>,
        Vec<(String, Vec<NumStr<I, F, C>>)>,
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
