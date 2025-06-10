#[cfg(any(feature = "skia", feature = "tiny-skia", feature = "wasm-draw"))]
use crate::App;
#[cfg(any(feature = "skia", feature = "tiny-skia", feature = "wasm-draw"))]
impl App {
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn set_title(&self, window: &std::sync::Arc<winit::window::Window>) {
        if self.name.is_empty() {
            window.set_title("kalc-plot");
        } else {
            window.set_title(&self.name);
        }
    }
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn set_title(&self, window: &winit::window::Window) {
        if self.name.is_empty() {
            window.set_title("kalc-plot");
        } else {
            window.set_title(&self.name);
        }
    }
    #[cfg(not(feature = "skia-vulkan"))]
    #[cfg(not(feature = "wasm-draw"))]
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn surface_state(
        &mut self,
    ) -> &mut Option<
        softbuffer::Surface<
            std::sync::Arc<winit::window::Window>,
            std::sync::Arc<winit::window::Window>,
        >,
    > {
        &mut self.surface_state
    }
    #[cfg(not(target_arch = "wasm32"))]
    #[cfg(feature = "skia-vulkan")]
    pub(crate) fn window(&self) -> Option<&std::sync::Arc<winit::window::Window>> {
        self.plot.renderer.as_ref().map(|s| &s.window)
    }
    #[cfg(not(feature = "skia-vulkan"))]
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn window(&mut self) -> Option<&std::sync::Arc<winit::window::Window>> {
        self.surface_state.as_mut().map(|s| s.window())
    }
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn window(&mut self) -> Option<&mut winit::window::Window> {
        self.window.as_mut()
    }
}
#[cfg(feature = "wasm")]
use wasm_bindgen::JsCast;
#[cfg(feature = "wasm")]
use web_sys::HtmlCanvasElement;
#[cfg(feature = "wasm")]
use winit::platform::web::WindowAttributesExtWebSys;
#[cfg(any(feature = "skia", feature = "tiny-skia", feature = "wasm-draw"))]
impl winit::application::ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let window = {
            #[cfg(feature = "wasm")]
            let window = web_sys::window().expect("no global `window` exists");
            #[cfg(feature = "wasm")]
            let document = window.document().expect("should have a document");
            #[cfg(feature = "wasm")]
            let canvas = document
                .get_element_by_id("canvas")
                .expect("document should have canvas")
                .dyn_into::<HtmlCanvasElement>()
                .expect("element should be a canvas");
            let window = winit::window::Window::default_attributes();
            #[cfg(feature = "wasm")]
            let window = window.with_canvas(Some(canvas));
            let window = event_loop.create_window(window);
            window.unwrap()
        };
        #[cfg(not(target_arch = "wasm32"))]
        {
            let window = std::sync::Arc::new(window);
            self.set_title(&window);
            #[cfg(not(feature = "skia-vulkan"))]
            #[cfg(not(feature = "wasm-draw"))]
            {
                let context = softbuffer::Context::new(window.clone()).unwrap();
                self.surface_state =
                    Some(softbuffer::Surface::new(&context, window.clone()).unwrap());
            }
            #[cfg(feature = "skia-vulkan")]
            self.plot.resumed(event_loop, window)
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.set_title(&window);
            self.window = Some(window)
        }
    }
    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        match event {
            winit::event::WindowEvent::Resized(_d) => {
                #[cfg(any(feature = "wasm", feature = "skia-vulkan"))]
                {
                    let Some(state) = self.window() else {
                        return;
                    };
                    #[cfg(feature = "wasm")]
                    {
                        use winit::platform::web::WindowExtWebSys;
                        let canvas: HtmlCanvasElement = state.canvas().unwrap();
                        canvas.set_width(_d.width);
                        canvas.set_height(_d.height);
                    }
                    state.request_redraw();
                    #[cfg(feature = "skia-vulkan")]
                    self.plot.resize();
                }
                #[cfg(not(any(feature = "wasm", feature = "skia-vulkan")))]
                {
                    let Some(state) = self.surface_state() else {
                        return;
                    };
                    state.window().request_redraw();
                    #[cfg(not(feature = "skia-vulkan"))]
                    #[cfg(feature = "softbuffer")]
                    state
                        .resize(
                            std::num::NonZeroU32::new(_d.width).unwrap(),
                            std::num::NonZeroU32::new(_d.height).unwrap(),
                        )
                        .unwrap();
                }
            }
            winit::event::WindowEvent::RedrawRequested => {
                let Some(state) = self.window() else {
                    return;
                };
                let (width, height) = {
                    let size = state.inner_size();
                    (size.width, size.height)
                };
                if self.touch_positions.len() > 1
                    && self.touch_positions.len() == self.last_touch_positions.len()
                {
                    fn avg(
                        vec: std::collections::hash_map::Values<u64, rupl::types::Vec2>,
                    ) -> rupl::types::Vec2 {
                        let l = vec.len();
                        vec.copied().sum::<rupl::types::Vec2>() / (l as f64)
                    }
                    let cpos = avg(self.touch_positions.values());
                    self.input_state.pointer_pos = Some(cpos);
                    let lpos = avg(self.last_touch_positions.values());
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
                if self.plot.request_redraw {
                    let Some(state) = self.window() else {
                        return;
                    };
                    state.request_redraw();
                }
                self.input_state.reset();
                self.last_touch_positions = self.touch_positions.clone();
            }
            winit::event::WindowEvent::CloseRequested => {
                #[cfg(any(feature = "skia-vulkan", feature = "bincode"))]
                self.plot.close();
                event_loop.exit();
            }
            winit::event::WindowEvent::KeyboardInput { event, .. } => {
                if event.state.is_pressed() {
                    let Some(state) = self.window() else {
                        return;
                    };
                    state.request_redraw();
                    self.input_state.keys_pressed.push(event.logical_key.into());
                }
            }
            winit::event::WindowEvent::MouseInput { state, button, .. } => match button {
                winit::event::MouseButton::Left => {
                    let Some(s) = self.window() else {
                        return;
                    };
                    s.request_redraw();
                    self.input_state.pointer = state.is_pressed().then_some(true);
                }
                winit::event::MouseButton::Right => {
                    let Some(s) = self.window() else {
                        return;
                    };
                    s.request_redraw();
                    self.input_state.pointer_right = state.is_pressed().then_some(true);
                }
                _ => {}
            },
            winit::event::WindowEvent::CursorEntered { .. } => {
                if self.input_state.pointer.is_none() && self.input_state.pointer_right.is_none() {
                    self.input_state.pointer = None;
                    self.input_state.pointer_right = None;
                }
            }
            winit::event::WindowEvent::CursorMoved { position, .. } => {
                let bool = self.input_state.pointer.is_some()
                    || (self.input_state.pointer_right.is_some() && self.plot.is_drag())
                    || (!self.plot.is_3d
                        && (!self.plot.disable_coord || self.plot.ruler_pos.is_some()));
                let Some(s) = self.window() else {
                    return;
                };
                if bool {
                    s.request_redraw();
                }
                self.input_state.pointer_pos = Some(rupl::types::Vec2::new(position.x, position.y));
            }
            winit::event::WindowEvent::MouseWheel { delta, .. } => {
                let Some(s) = self.window() else {
                    return;
                };
                s.request_redraw();
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
                let empty = self.input_state.keys_pressed.is_empty();
                let Some(s) = self.window() else {
                    return;
                };
                if !empty {
                    s.request_redraw();
                }
                self.input_state.modifiers.alt = modifiers.state().alt_key();
                self.input_state.modifiers.ctrl = modifiers.state().control_key();
                self.input_state.modifiers.shift = modifiers.state().shift_key();
                self.input_state.modifiers.command = modifiers.state().super_key();
            }
            winit::event::WindowEvent::PanGesture { delta, .. } => {
                let Some(s) = self.window() else {
                    return;
                };
                s.request_redraw();
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
                let Some(s) = self.window() else {
                    return;
                };
                s.request_redraw();
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
                let Some(s) = self.window() else {
                    return;
                };
                s.request_redraw();
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
        #[cfg(not(feature = "skia-vulkan"))]
        #[cfg(not(feature = "wasm-draw"))]
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.surface_state = None
        }
        #[cfg(feature = "skia-vulkan")]
        {
            self.plot.renderer = None
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.window = None;
        }
    }
}
