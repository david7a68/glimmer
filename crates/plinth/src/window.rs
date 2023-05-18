use std::{cell::RefCell, collections::HashMap, rc::Rc};

use geometry::{Extent, Offset, Point, ScreenPx};
use raw_window_handle::{
    HasRawDisplayHandle, HasRawWindowHandle, RawDisplayHandle, RawWindowHandle,
};
use winit::{
    dpi::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize},
    event::{Event, WindowEvent},
    event_loop::EventLoop,
    platform::windows::WindowBuilderExtWindows,
};

use crate::input::{ButtonState, MouseButton, VirtualKeyCode, KEY_MAP};

/// A unique identifier assigned to a window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WindowId(winit::window::WindowId);

/// Trait for handling window events.
pub trait WindowHandler {
    /// Called when the window is destroyed. This is the last event that will be
    /// received by the window handler before it is dropped.
    fn on_destroy(&mut self);

    /// Called when the user has requested that the window be closed, either by
    /// clicking the X, by pressing Alt-F4, etc.
    fn on_close_request(&mut self, spawner: &mut dyn WindowSpawner<Self>) -> bool;

    /// Called when a mouse button is pressed or released within the bounds of
    /// the window.
    fn on_mouse_button(
        &mut self,
        spawner: &mut dyn WindowSpawner<Self>,
        button: MouseButton,
        state: ButtonState,
        at: Point<i32, ScreenPx>,
    );

    /// Called when the cursor moves within the bounds of the window.
    ///
    /// Captive cursor mode is not currently supported.
    fn on_cursor_move(&mut self, spawner: &mut dyn WindowSpawner<Self>, at: Point<i32, ScreenPx>);

    /// Called when a key is pressed or released.
    fn on_key(
        &mut self,
        spawner: &mut dyn WindowSpawner<Self>,
        key: VirtualKeyCode,
        state: ButtonState,
    );

    /// Called when the window is resized.
    fn on_resize(
        &mut self,
        spawner: &mut dyn WindowSpawner<Self>,
        inner_size: Extent<u32, ScreenPx>,
    );

    /// Called when window DPI scaling changes. This may change if the user
    /// changes OS DPI or resolution settings, or if the window moves between
    /// two monitors with different DPI.
    fn on_rescale(
        &mut self,
        spawner: &mut dyn WindowSpawner<Self>,
        scale_factor: f64,
        new_inner_size: Extent<u32, ScreenPx>,
    );

    fn on_idle(&mut self, spawner: &mut dyn WindowSpawner<Self>);

    /// Called when the OS requests that the window be redrawn.
    fn on_redraw(&mut self, spawner: &mut dyn WindowSpawner<Self>);
}

/// Event loop interface for spawing new windows.
///
/// Only accessible from within a window handler (and event loop).
pub trait WindowSpawner<Handler: WindowHandler> {
    /// Creates a new window bound to the event loop.
    fn spawn(&mut self, desc: WindowDesc<Handler>);
}

bitflags::bitflags! {
    pub struct WindowFlags: u32 {
        const RESIZABLE = 0x1;
        const VISIBLE = 0x2;
        const TRANSPARENT = 0x4;
        const ALWAYS_ON_TOP = 0x8;
    }
}

impl Default for WindowFlags {
    fn default() -> Self {
        WindowFlags::RESIZABLE | WindowFlags::VISIBLE
    }
}

/// A description of a window to be created.
///
/// Pass this in to the `spawn` method of a `WindowControl` or to the `run`
/// function on event loop start.
pub struct WindowDesc<'a, Handler: WindowHandler> {
    pub title: &'a str,
    pub size: Extent<u32, ScreenPx>,
    pub min_size: Option<Extent<u32, ScreenPx>>,
    pub max_size: Option<Extent<u32, ScreenPx>>,
    pub position: Option<Offset<i32, ScreenPx>>,
    pub flags: WindowFlags,
    /// Constructor for the window handler.
    pub handler: &'a mut dyn FnMut(Window) -> Handler,
}

impl<'a, Handler: WindowHandler> WindowDesc<'a, Handler> {
    fn build(
        self,
        target: &winit::event_loop::EventLoopWindowTarget<()>,
        deferred_destroy: DeferredDestroy,
    ) -> WindowState<Handler> {
        let mut builder = winit::window::WindowBuilder::new()
            .with_title(self.title)
            .with_inner_size(as_logical_size(self.size))
            .with_resizable(self.flags.contains(WindowFlags::RESIZABLE))
            .with_visible(self.flags.contains(WindowFlags::VISIBLE))
            .with_transparent(self.flags.contains(WindowFlags::TRANSPARENT))
            .with_always_on_top(self.flags.contains(WindowFlags::ALWAYS_ON_TOP));

        if let Some(position) = self.position {
            builder = builder.with_position(as_logical_position(position));
        }

        if let Some(min_size) = self.min_size {
            builder = builder.with_min_inner_size(as_logical_size(min_size));
        }

        if let Some(max_size) = self.max_size {
            builder = builder.with_max_inner_size(as_logical_size(max_size));
        }

        #[cfg(target_os = "windows")]
        let builder = builder.with_no_redirection_bitmap(true);

        let window = builder.build(target).unwrap();
        let id = window.id();

        let extent = as_extent(window.inner_size());

        let handler = (self.handler)(Window {
            inner: window,
            deferred_destroy,
        });

        WindowState {
            id,
            handler,
            extent,
            cursor_position: Point::zero(),
            repeated_key: None,
        }
    }
}

/// An operating system window.
#[must_use]
pub struct Window {
    inner: winit::window::Window,
    deferred_destroy: DeferredDestroy,
}

unsafe impl HasRawWindowHandle for Window {
    fn raw_window_handle(&self) -> RawWindowHandle {
        self.inner.raw_window_handle()
    }
}

unsafe impl HasRawDisplayHandle for Window {
    fn raw_display_handle(&self) -> RawDisplayHandle {
        self.inner.raw_display_handle()
    }
}

impl Window {
    #[must_use]
    pub fn id(&self) -> WindowId {
        WindowId(self.inner.id())
    }

    #[must_use]
    pub fn extent(&self) -> Extent<u32, ScreenPx> {
        as_extent(self.inner.inner_size())
    }

    pub fn set_title(&mut self, title: &str) {
        self.inner.set_title(title);
    }

    pub fn destroy(&self) {
        self.deferred_destroy.borrow_mut().push(self.inner.id());
    }

    pub fn request_redraw(&self) {
        self.inner.request_redraw();
    }
}

#[must_use]
struct WindowState<Handler: WindowHandler> {
    id: winit::window::WindowId,
    handler: Handler,
    extent: Extent<u32, ScreenPx>,
    cursor_position: Point<i32, ScreenPx>,
    repeated_key: Option<(winit::event::KeyboardInput, u16)>,
}

#[must_use]
struct Control<'a, Handler: WindowHandler> {
    event_loop: &'a winit::event_loop::EventLoopWindowTarget<()>,
    buffered_creates: &'a mut Vec<WindowState<Handler>>,
    buffered_destroys: &'a DeferredDestroy,
}

impl<'a, Handler: WindowHandler> Control<'a, Handler> {
    fn new(
        event_loop: &'a winit::event_loop::EventLoopWindowTarget<()>,
        buffered_creates: &'a mut Vec<WindowState<Handler>>,
        buffered_destroys: &'a DeferredDestroy,
    ) -> Self {
        Self {
            event_loop,
            buffered_creates,
            buffered_destroys,
        }
    }
}

impl<'a, Handler: WindowHandler> WindowSpawner<Handler> for Control<'a, Handler> {
    fn spawn(&mut self, desc: WindowDesc<Handler>) {
        let window = desc.build(self.event_loop, self.buffered_destroys.clone());
        self.buffered_creates.push(window);
    }
}

/// Holds the ids of windows that are scheduled to be destroyed. They are kept
/// on the heap to allow `Window` to own a reference to it. This is necessary
/// for `Window::destroy` to schedule the window for destruction.
type DeferredDestroy = Rc<RefCell<Vec<winit::window::WindowId>>>;

/// Creates the described windows and runs the OS event loop until all windows
/// are destroyed.
#[allow(clippy::too_many_lines)]
pub fn enter_event_loop<'a, Handler, I>(window_descs: I)
where
    Handler: WindowHandler + 'static,
    I: IntoIterator<Item = WindowDesc<'a, Handler>>,
{
    let event_loop = EventLoop::new();
    let mut windows = HashMap::with_capacity(2);

    // We need to buffer windows created within the event loop because we would
    // otherwise concurrently borrow from `windows` whilst potentially creating
    // new windows within a window's event handler. These buffered windows are
    // added to the map at the end of every event loop invocation.
    let mut buffered_window_creates: Vec<WindowState<Handler>> = Vec::new();
    let buffered_window_destroys: DeferredDestroy = Rc::new(RefCell::new(Vec::new()));

    for desc in window_descs {
        let window = desc.build(&event_loop, buffered_window_destroys.clone());
        windows.insert(window.id, window);
    }

    for window in buffered_window_creates.drain(..) {
        windows.insert(window.id, window);
    }

    for window_id in buffered_window_destroys.borrow_mut().drain(..) {
        let mut state = windows
            .remove(&window_id)
            .expect("cannot destory a window twice");
        state.handler.on_destroy();
    }

    event_loop.run(move |event, event_loop, control_flow| {
        // control_flow.set_wait();
        control_flow.set_poll();

        let mut control = Control::new(
            event_loop,
            &mut buffered_window_creates,
            &buffered_window_destroys,
        );

        match event {
            Event::WindowEvent { window_id, event } => {
                let Some(window_state) = windows.get_mut(&window_id) else {
                    // The window in question has been 'destroyed'.
                    if windows.is_empty() {
                        *control_flow = winit::event_loop::ControlFlow::Exit;
                    }
                    return;
                };

                match event {
                    WindowEvent::Resized(extent) => {
                        if as_extent(extent) != window_state.extent {
                            window_state
                                .handler
                                .on_resize(&mut control, as_extent(extent));
                        }
                    }
                    WindowEvent::CloseRequested => {
                        if window_state.handler.on_close_request(&mut control) {
                            buffered_window_destroys.borrow_mut().push(window_id);
                        }
                    }
                    WindowEvent::CursorMoved {
                        device_id: _,
                        position,
                        ..
                    } => {
                        window_state.cursor_position = as_point(position.cast());
                        window_state
                            .handler
                            .on_cursor_move(&mut control, window_state.cursor_position);
                    }
                    WindowEvent::MouseInput {
                        device_id: _,
                        state,
                        button,
                        ..
                    } => window_state.handler.on_mouse_button(
                        &mut control,
                        match button {
                            winit::event::MouseButton::Left => MouseButton::Left,
                            winit::event::MouseButton::Right => MouseButton::Right,
                            winit::event::MouseButton::Middle => MouseButton::Middle,
                            winit::event::MouseButton::Other(other) => MouseButton::Other(other),
                        },
                        match state {
                            winit::event::ElementState::Pressed => ButtonState::Pressed,
                            winit::event::ElementState::Released => ButtonState::Released,
                        },
                        window_state.cursor_position,
                    ),
                    WindowEvent::KeyboardInput {
                        device_id: _,
                        input,
                        is_synthetic: _,
                    } => {
                        let Some(virtual_keycode) = input.virtual_keycode else { return; };
                        let virtual_keycode = KEY_MAP[virtual_keycode as usize];

                        match input.state {
                            winit::event::ElementState::Pressed => {
                                if let Some((repeated_key, count)) = window_state.repeated_key {
                                    if repeated_key == input {
                                        window_state.handler.on_key(
                                            &mut control,
                                            virtual_keycode,
                                            ButtonState::Repeated(count + 1),
                                        );
                                        window_state.repeated_key = Some((input, count + 1));
                                    }
                                } else {
                                    window_state.handler.on_key(
                                        &mut control,
                                        virtual_keycode,
                                        ButtonState::Pressed,
                                    );
                                    window_state.repeated_key = Some((input, 0));
                                }
                            }
                            winit::event::ElementState::Released => {
                                window_state.handler.on_key(
                                    &mut control,
                                    virtual_keycode,
                                    ButtonState::Released,
                                );
                                window_state.repeated_key = None;
                            }
                        }
                    }
                    WindowEvent::ScaleFactorChanged {
                        scale_factor,
                        new_inner_size,
                    } => {
                        window_state.handler.on_rescale(
                            &mut control,
                            scale_factor,
                            as_extent(*new_inner_size),
                        );
                    }
                    _ => {}
                }
            }
            Event::MainEventsCleared => {
                for window in windows.values_mut() {
                    window.handler.on_idle(&mut control);
                }
            }
            Event::RedrawRequested(window_id) => {
                let window_state = windows
                    .get_mut(&window_id)
                    .expect("the window must exist for the OS to request that it be redrawn");
                window_state.handler.on_redraw(&mut control);
            }
            _ => {}
        }

        // Add any windows that were created during this iteration of the event
        // loop to the map.
        for window in buffered_window_creates.drain(..) {
            windows.insert(window.id, window);
        }

        // Remove any windows that were destroyed during this iteration of the
        // event loop to the map.
        for window_id in buffered_window_destroys.borrow_mut().drain(..) {
            let mut state = windows
                .remove(&window_id)
                .expect("cannot destroy a window twice");
            state.handler.on_destroy();
        }
    });
}

#[allow(clippy::needless_pass_by_value)]
fn as_logical_size(size: Extent<u32, ScreenPx>) -> LogicalSize<u32> {
    LogicalSize::new(size.width, size.height)
}

#[allow(clippy::needless_pass_by_value)]
fn as_logical_position(position: Offset<i32, ScreenPx>) -> LogicalPosition<i32> {
    LogicalPosition::new(position.x, position.y)
}

#[allow(clippy::needless_pass_by_value)]
fn as_extent(size: PhysicalSize<u32>) -> Extent<u32, ScreenPx> {
    Extent::new(size.width, size.height)
}

#[allow(clippy::needless_pass_by_value)]
fn as_point(position: PhysicalPosition<i32>) -> Point<i32, ScreenPx> {
    Point::new(position.x, position.y)
}
