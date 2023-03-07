use std::collections::HashMap;

use geometry::{Extent, Offset, Point};
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use winit::{
    dpi::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize},
    event::{Event, WindowEvent},
    event_loop::EventLoop,
};

/// Enumerates mouse buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    Other(u16),
}

/// Enumerates button states.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ButtonState {
    Pressed,
    Released,
    /// The number of 'repeat' cycles a button has been pressed for. The
    /// frequency of these cycles is operating system dependent and may be
    /// changed by the user.
    Repeated(u16),
}

/// Symbolic name for a key on the keyboard.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VirtualKeyCode {
    Invalid,

    Key0,
    Key1,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
    Key8,
    Key9,

    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,

    Keypad0,
    Keypad1,
    Keypad2,
    Keypad3,
    Keypad4,
    Keypad5,
    Keypad6,
    Keypad7,
    Keypad8,
    Keypad9,

    KeypadAdd,
    KeypadSubtract,
    KeypadMultiply,
    KeypadDivide,
    KeypadDecimal,

    /// For any country/region, the '=+' key.
    Equals,
    /// For any country/region, the ',<' key.
    Comma,
    /// For any country/region, the '-_' key.
    Minus,
    /// For any country/region, the '.>' key.
    Period,

    /// For the US standard keyboard, the ';:' key.
    Semicolon,
    /// For the US standard keyboard, the '/?' key.
    Slash,
    /// For the US standard keyboard, the '`~' key.
    Grave,
    /// For the US standard keyboard, the '[{' key.
    LBracket,
    /// For the US standard keyboard, the '\\|' key.
    Backslash,
    /// For the US standard keyboard, the ']}' key.
    Rbracket,
    /// For the US standard keyboard, the 'single-quote/double-quote' key.
    Apostrophe,

    Tab,
    Space,

    ImeKana,
    // ImeModeHangul,
    // ImeJunja,
    // ImeFinal,
    // ImeHanje,
    ImeKanji,

    ImeConvert,
    ImeNonConvert,
    // ImeAccept,
    // ImeModeChange,
    // ImeProcess,
    Insert,
    Delete,

    Backspace,
    Enter,
    LShift,
    RShift,
    LControl,
    RControl,
    LMenu,
    RMenu,
    Pause,
    CapsLock,
    Escape,

    PageUp,
    PageDown,
    End,
    Home,

    Left,
    Right,
    Up,
    Down,

    NumLock,
    ScrollLock,

    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,

    LSuper,
    RSuper,

    Select,
    Snapshot,

    MediaNextTrack,
    MediaPrevTrack,
    MediaStop,
    MediaPlayPause,
}

/// A unique identifier assigned to a window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WindowId(winit::window::WindowId);

/// Trait for handling window events.
pub trait WindowHandler {
    /// Called when the window is first created.
    fn on_create(&mut self, control: &mut dyn WindowControl, id: WindowId, handle: RawWindowHandle);

    /// Called when the window is destroyed. This is the last event that will be
    /// received by the window handler before it is dropped.
    fn on_destroy(&mut self);

    /// Called when the user has requested that the window be closed, either by
    /// clicking the X, by pressing Alt-F4, etc.
    fn on_close_request(&mut self, control: &mut dyn WindowControl) -> bool;

    /// Called when a mouse button is pressed or released within the bounds of
    /// the window.
    fn on_mouse_button(
        &mut self,
        control: &mut dyn WindowControl,
        button: MouseButton,
        state: ButtonState,
        at: Point<i32>,
    );

    /// Called when the cursor moves within the bounds of the window.
    ///
    /// Captive cursor mode is not currently supported.
    fn on_cursor_move(&mut self, control: &mut dyn WindowControl, at: Point<i32>);

    /// Called when a key is pressed or released.
    fn on_key(&mut self, control: &mut dyn WindowControl, key: VirtualKeyCode, state: ButtonState);

    /// Called when the window is resized.
    fn on_resize(&mut self, control: &mut dyn WindowControl, inner_size: Extent<u32>);

    /// Called when window DPI scaling changes. This may change if the user
    /// changes OS DPI or resolution settings, or if the window moves between
    /// two monitors with different DPI.
    fn on_rescale(
        &mut self,
        control: &mut dyn WindowControl,
        scale_factor: f64,
        new_inner_size: Extent<u32>,
    );

    /// Called when the OS requests that the window be redrawn.
    fn on_redraw(&mut self, control: &mut dyn WindowControl);
}

/// Trait for feeding back window control to the shell.
pub trait WindowControl {
    fn destroy(&mut self, window: WindowId);

    fn spawn(&mut self, desc: WindowDesc);
}

/// A description of a window. Pass this in to the `spawn` method of a
/// `WindowControl` or to the `run` function on event loop start.
pub struct WindowDesc<'a> {
    pub title: &'a str,
    pub size: Extent<u32>,
    pub min_size: Option<Extent<u32>>,
    pub max_size: Option<Extent<u32>>,
    pub position: Option<Offset<i32>>,
    pub resizable: bool,
    pub visible: bool,
    pub transparent: bool,
    pub always_on_top: bool,
    pub handler: Box<dyn WindowHandler>,
}

impl<'a> WindowDesc<'a> {
    fn build(
        mut self,
        target: &winit::event_loop::EventLoopWindowTarget<()>,
        buffered_creates: &'a mut Vec<(winit::window::Window, WindowState)>,
        buffered_destroys: &'a mut Vec<WindowId>,
    ) -> (winit::window::Window, WindowState) {
        let mut builder = winit::window::WindowBuilder::new()
            .with_title(self.title)
            .with_inner_size(as_logical_size(self.size))
            .with_resizable(self.resizable)
            .with_visible(self.visible)
            .with_transparent(self.transparent)
            .with_always_on_top(self.always_on_top);

        if let Some(position) = self.position {
            builder = builder.with_position(as_logical_position(position));
        }

        if let Some(min_size) = self.min_size {
            builder = builder.with_min_inner_size(as_logical_size(min_size));
        }

        if let Some(max_size) = self.max_size {
            builder = builder.with_max_inner_size(as_logical_size(max_size));
        }

        let window = builder.build(target).unwrap();

        // Inform the handler that the window has been created.
        self.handler.on_create(
            &mut Control::new(target, buffered_creates, buffered_destroys),
            WindowId(window.id()),
            window.raw_window_handle(),
        );

        (
            window,
            WindowState {
                handler: self.handler,
                cursor_position: Point::zero(),
                repeated_key: None,
            },
        )
    }
}

struct WindowState {
    handler: Box<dyn WindowHandler>,
    cursor_position: Point<i32>,
    repeated_key: Option<(winit::event::KeyboardInput, u16)>,
}

struct Control<'a> {
    event_loop: &'a winit::event_loop::EventLoopWindowTarget<()>,
    buffered_creates: &'a mut Vec<(winit::window::Window, WindowState)>,
    buffered_destroys: &'a mut Vec<WindowId>,
}

impl<'a> Control<'a> {
    fn new(
        event_loop: &'a winit::event_loop::EventLoopWindowTarget<()>,
        buffered_creates: &'a mut Vec<(winit::window::Window, WindowState)>,
        buffered_destroys: &'a mut Vec<WindowId>,
    ) -> Self {
        Self {
            event_loop,
            buffered_creates,
            buffered_destroys,
        }
    }
}

impl<'a> WindowControl for Control<'a> {
    fn destroy(&mut self, window: WindowId) {
        self.buffered_destroys.push(window);
    }

    fn spawn(&mut self, desc: WindowDesc) {
        let (window, state) = desc.build(
            self.event_loop,
            self.buffered_creates,
            self.buffered_destroys,
        );
        self.buffered_creates.push((window, state));
    }
}

/// Creates the described windows and runs the OS event loop until all windows
/// are destroyed.
pub fn run<'a>(window_descs: impl IntoIterator<Item = WindowDesc<'a>>) {
    let event_loop = EventLoop::new();
    let mut windows = HashMap::with_capacity(2);

    // We need to buffer windows created within the event loop because we would
    // otherwise concurrently borrow from `windows` whilst potentially creating
    // new windows within a window's event handler. These buffered windows are
    // added to the map at the end of every event loop invocation.
    let mut buffered_window_creates = Vec::new();
    let mut buffered_window_destroys = Vec::new();

    for desc in window_descs {
        let (window, state) = desc.build(
            &event_loop,
            &mut buffered_window_creates,
            &mut buffered_window_destroys,
        );
        windows.insert(window.id(), (window, state));
    }

    for (window, state) in buffered_window_creates.drain(..) {
        windows.insert(window.id(), (window, state));
    }

    for window_id in buffered_window_destroys.drain(..) {
        windows.remove(&window_id.0);
    }

    event_loop.run(move |event, event_loop, control_flow| {
        control_flow.set_wait();

        let mut control = Control::new(
            event_loop,
            &mut buffered_window_creates,
            &mut buffered_window_destroys,
        );

        match event {
            Event::NewEvents(_) => {}
            Event::WindowEvent { window_id, event } => {
                let Some((_window, window_state)) = windows.get_mut(&window_id) else {
                    // The window in question has been 'destroyed'.
                    if windows.is_empty() {
                        *control_flow = winit::event_loop::ControlFlow::Exit;
                    }
                    return;
                };

                match event {
                    WindowEvent::Resized(extent) => {
                        window_state
                            .handler
                            .on_resize(&mut control, as_extent(extent));
                    }
                    WindowEvent::CloseRequested => {
                        if window_state.handler.on_close_request(&mut control) {
                            let (_window, mut state) = windows.remove(&window_id).unwrap();
                            state.handler.on_destroy();
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
            Event::DeviceEvent { .. } => {}
            Event::UserEvent(_) => {}
            Event::Suspended => {}
            Event::Resumed => {}
            Event::MainEventsCleared => {}
            Event::RedrawRequested(window_id) => {
                let (_window, window_state) = windows.get_mut(&window_id).unwrap();
                window_state.handler.on_redraw(&mut control);
            }
            Event::RedrawEventsCleared => {}
            Event::LoopDestroyed => {}
        }

        // Add any windows that were created during this iteration of the event
        // loop to the map.
        for (window, state) in buffered_window_creates.drain(..) {
            windows.insert(window.id(), (window, state));
        }

        // Remove any windows that were destroyed during this iteration of the
        // event loop to the map.
        for window_id in buffered_window_destroys.drain(..) {
            let (_window, mut state) = windows.remove(&window_id.0).unwrap();
            state.handler.on_destroy();
            // _window gets dropped, producing the `WindowEvent::Destroyed` event.
        }
    });
}

fn as_logical_size(size: Extent<u32>) -> LogicalSize<u32> {
    LogicalSize::new(size.width, size.height)
}

fn as_logical_position(position: Offset<i32>) -> LogicalPosition<i32> {
    LogicalPosition::new(position.dx, position.dy)
}

fn as_extent(size: PhysicalSize<u32>) -> Extent<u32> {
    Extent::new(size.width, size.height)
}

fn as_point(position: PhysicalPosition<i32>) -> Point<i32> {
    Point::new(position.x, position.y)
}

const KEY_MAP: [VirtualKeyCode; 163] = {
    let mut table = [VirtualKeyCode::Invalid; 163];

    table[winit::event::VirtualKeyCode::Key1 as usize] = VirtualKeyCode::Key1;
    table[winit::event::VirtualKeyCode::Key2 as usize] = VirtualKeyCode::Key2;
    table[winit::event::VirtualKeyCode::Key3 as usize] = VirtualKeyCode::Key3;
    table[winit::event::VirtualKeyCode::Key4 as usize] = VirtualKeyCode::Key4;
    table[winit::event::VirtualKeyCode::Key5 as usize] = VirtualKeyCode::Key5;
    table[winit::event::VirtualKeyCode::Key6 as usize] = VirtualKeyCode::Key6;
    table[winit::event::VirtualKeyCode::Key7 as usize] = VirtualKeyCode::Key7;
    table[winit::event::VirtualKeyCode::Key8 as usize] = VirtualKeyCode::Key8;
    table[winit::event::VirtualKeyCode::Key9 as usize] = VirtualKeyCode::Key9;
    table[winit::event::VirtualKeyCode::Key0 as usize] = VirtualKeyCode::Key0;

    table[winit::event::VirtualKeyCode::A as usize] = VirtualKeyCode::A;
    table[winit::event::VirtualKeyCode::B as usize] = VirtualKeyCode::B;
    table[winit::event::VirtualKeyCode::C as usize] = VirtualKeyCode::C;
    table[winit::event::VirtualKeyCode::D as usize] = VirtualKeyCode::D;
    table[winit::event::VirtualKeyCode::E as usize] = VirtualKeyCode::E;
    table[winit::event::VirtualKeyCode::F as usize] = VirtualKeyCode::F;
    table[winit::event::VirtualKeyCode::G as usize] = VirtualKeyCode::G;
    table[winit::event::VirtualKeyCode::H as usize] = VirtualKeyCode::H;
    table[winit::event::VirtualKeyCode::I as usize] = VirtualKeyCode::I;
    table[winit::event::VirtualKeyCode::J as usize] = VirtualKeyCode::J;
    table[winit::event::VirtualKeyCode::K as usize] = VirtualKeyCode::K;
    table[winit::event::VirtualKeyCode::L as usize] = VirtualKeyCode::L;
    table[winit::event::VirtualKeyCode::M as usize] = VirtualKeyCode::M;
    table[winit::event::VirtualKeyCode::N as usize] = VirtualKeyCode::N;
    table[winit::event::VirtualKeyCode::O as usize] = VirtualKeyCode::O;
    table[winit::event::VirtualKeyCode::P as usize] = VirtualKeyCode::P;
    table[winit::event::VirtualKeyCode::Q as usize] = VirtualKeyCode::Q;
    table[winit::event::VirtualKeyCode::R as usize] = VirtualKeyCode::R;
    table[winit::event::VirtualKeyCode::S as usize] = VirtualKeyCode::S;
    table[winit::event::VirtualKeyCode::T as usize] = VirtualKeyCode::T;
    table[winit::event::VirtualKeyCode::U as usize] = VirtualKeyCode::U;
    table[winit::event::VirtualKeyCode::V as usize] = VirtualKeyCode::V;
    table[winit::event::VirtualKeyCode::W as usize] = VirtualKeyCode::W;
    table[winit::event::VirtualKeyCode::X as usize] = VirtualKeyCode::X;
    table[winit::event::VirtualKeyCode::Y as usize] = VirtualKeyCode::Y;
    table[winit::event::VirtualKeyCode::Z as usize] = VirtualKeyCode::Z;

    table[winit::event::VirtualKeyCode::Numpad1 as usize] = VirtualKeyCode::Keypad1;
    table[winit::event::VirtualKeyCode::Numpad2 as usize] = VirtualKeyCode::Keypad2;
    table[winit::event::VirtualKeyCode::Numpad3 as usize] = VirtualKeyCode::Keypad3;
    table[winit::event::VirtualKeyCode::Numpad4 as usize] = VirtualKeyCode::Keypad4;
    table[winit::event::VirtualKeyCode::Numpad5 as usize] = VirtualKeyCode::Keypad5;
    table[winit::event::VirtualKeyCode::Numpad6 as usize] = VirtualKeyCode::Keypad6;
    table[winit::event::VirtualKeyCode::Numpad7 as usize] = VirtualKeyCode::Keypad7;
    table[winit::event::VirtualKeyCode::Numpad8 as usize] = VirtualKeyCode::Keypad8;
    table[winit::event::VirtualKeyCode::Numpad9 as usize] = VirtualKeyCode::Keypad9;
    table[winit::event::VirtualKeyCode::Numpad0 as usize] = VirtualKeyCode::Keypad0;
    table[winit::event::VirtualKeyCode::NumpadAdd as usize] = VirtualKeyCode::KeypadAdd;
    table[winit::event::VirtualKeyCode::NumpadSubtract as usize] = VirtualKeyCode::KeypadSubtract;
    table[winit::event::VirtualKeyCode::NumpadMultiply as usize] = VirtualKeyCode::KeypadMultiply;
    table[winit::event::VirtualKeyCode::NumpadDivide as usize] = VirtualKeyCode::KeypadDivide;
    table[winit::event::VirtualKeyCode::NumpadDecimal as usize] = VirtualKeyCode::KeypadDecimal;

    table[winit::event::VirtualKeyCode::Equals as usize] = VirtualKeyCode::Equals;
    table[winit::event::VirtualKeyCode::Comma as usize] = VirtualKeyCode::Comma;
    table[winit::event::VirtualKeyCode::Minus as usize] = VirtualKeyCode::Minus;
    table[winit::event::VirtualKeyCode::Period as usize] = VirtualKeyCode::Period;

    table[winit::event::VirtualKeyCode::Semicolon as usize] = VirtualKeyCode::Semicolon;
    table[winit::event::VirtualKeyCode::Slash as usize] = VirtualKeyCode::Slash;
    table[winit::event::VirtualKeyCode::Grave as usize] = VirtualKeyCode::Grave;
    table[winit::event::VirtualKeyCode::LBracket as usize] = VirtualKeyCode::LBracket;
    table[winit::event::VirtualKeyCode::Backslash as usize] = VirtualKeyCode::Backslash;
    table[winit::event::VirtualKeyCode::RBracket as usize] = VirtualKeyCode::Rbracket;
    table[winit::event::VirtualKeyCode::Apostrophe as usize] = VirtualKeyCode::Apostrophe;

    table[winit::event::VirtualKeyCode::Tab as usize] = VirtualKeyCode::Tab;
    table[winit::event::VirtualKeyCode::Space as usize] = VirtualKeyCode::Space;

    table[winit::event::VirtualKeyCode::Kana as usize] = VirtualKeyCode::ImeKana;
    table[winit::event::VirtualKeyCode::Kanji as usize] = VirtualKeyCode::ImeKanji;

    table[winit::event::VirtualKeyCode::Convert as usize] = VirtualKeyCode::ImeConvert;
    table[winit::event::VirtualKeyCode::NoConvert as usize] = VirtualKeyCode::ImeNonConvert;

    table[winit::event::VirtualKeyCode::Insert as usize] = VirtualKeyCode::Insert;
    table[winit::event::VirtualKeyCode::Delete as usize] = VirtualKeyCode::Delete;

    table[winit::event::VirtualKeyCode::Back as usize] = VirtualKeyCode::Backspace;
    table[winit::event::VirtualKeyCode::Return as usize] = VirtualKeyCode::Enter;
    table[winit::event::VirtualKeyCode::LShift as usize] = VirtualKeyCode::LShift;
    table[winit::event::VirtualKeyCode::RShift as usize] = VirtualKeyCode::RShift;
    table[winit::event::VirtualKeyCode::LControl as usize] = VirtualKeyCode::LControl;
    table[winit::event::VirtualKeyCode::RControl as usize] = VirtualKeyCode::RControl;
    table[winit::event::VirtualKeyCode::LAlt as usize] = VirtualKeyCode::LMenu;
    table[winit::event::VirtualKeyCode::RAlt as usize] = VirtualKeyCode::RMenu;
    table[winit::event::VirtualKeyCode::Pause as usize] = VirtualKeyCode::Pause;
    table[winit::event::VirtualKeyCode::Capital as usize] = VirtualKeyCode::CapsLock;
    table[winit::event::VirtualKeyCode::Escape as usize] = VirtualKeyCode::Escape;

    table[winit::event::VirtualKeyCode::PageUp as usize] = VirtualKeyCode::PageUp;
    table[winit::event::VirtualKeyCode::PageDown as usize] = VirtualKeyCode::PageDown;
    table[winit::event::VirtualKeyCode::Home as usize] = VirtualKeyCode::Home;
    table[winit::event::VirtualKeyCode::End as usize] = VirtualKeyCode::End;
    table[winit::event::VirtualKeyCode::Left as usize] = VirtualKeyCode::Left;
    table[winit::event::VirtualKeyCode::Right as usize] = VirtualKeyCode::Right;
    table[winit::event::VirtualKeyCode::Up as usize] = VirtualKeyCode::Up;
    table[winit::event::VirtualKeyCode::Down as usize] = VirtualKeyCode::Down;

    table[winit::event::VirtualKeyCode::Scroll as usize] = VirtualKeyCode::ScrollLock;
    table[winit::event::VirtualKeyCode::Numlock as usize] = VirtualKeyCode::NumLock;

    table[winit::event::VirtualKeyCode::F1 as usize] = VirtualKeyCode::F1;
    table[winit::event::VirtualKeyCode::F2 as usize] = VirtualKeyCode::F2;
    table[winit::event::VirtualKeyCode::F3 as usize] = VirtualKeyCode::F3;
    table[winit::event::VirtualKeyCode::F4 as usize] = VirtualKeyCode::F4;
    table[winit::event::VirtualKeyCode::F5 as usize] = VirtualKeyCode::F5;
    table[winit::event::VirtualKeyCode::F6 as usize] = VirtualKeyCode::F6;
    table[winit::event::VirtualKeyCode::F7 as usize] = VirtualKeyCode::F7;
    table[winit::event::VirtualKeyCode::F8 as usize] = VirtualKeyCode::F8;
    table[winit::event::VirtualKeyCode::F9 as usize] = VirtualKeyCode::F9;
    table[winit::event::VirtualKeyCode::F10 as usize] = VirtualKeyCode::F10;
    table[winit::event::VirtualKeyCode::F11 as usize] = VirtualKeyCode::F11;
    table[winit::event::VirtualKeyCode::F12 as usize] = VirtualKeyCode::F12;
    table[winit::event::VirtualKeyCode::F13 as usize] = VirtualKeyCode::F13;
    table[winit::event::VirtualKeyCode::F14 as usize] = VirtualKeyCode::F14;
    table[winit::event::VirtualKeyCode::F15 as usize] = VirtualKeyCode::F15;
    table[winit::event::VirtualKeyCode::F16 as usize] = VirtualKeyCode::F16;
    table[winit::event::VirtualKeyCode::F17 as usize] = VirtualKeyCode::F17;
    table[winit::event::VirtualKeyCode::F18 as usize] = VirtualKeyCode::F18;
    table[winit::event::VirtualKeyCode::F19 as usize] = VirtualKeyCode::F19;
    table[winit::event::VirtualKeyCode::F20 as usize] = VirtualKeyCode::F20;
    table[winit::event::VirtualKeyCode::F21 as usize] = VirtualKeyCode::F21;
    table[winit::event::VirtualKeyCode::F22 as usize] = VirtualKeyCode::F22;
    table[winit::event::VirtualKeyCode::F23 as usize] = VirtualKeyCode::F23;
    table[winit::event::VirtualKeyCode::F24 as usize] = VirtualKeyCode::F24;

    table[winit::event::VirtualKeyCode::LWin as usize] = VirtualKeyCode::LSuper;
    table[winit::event::VirtualKeyCode::RWin as usize] = VirtualKeyCode::RSuper;

    table[winit::event::VirtualKeyCode::MediaSelect as usize] = VirtualKeyCode::Select;
    table[winit::event::VirtualKeyCode::Snapshot as usize] = VirtualKeyCode::Snapshot;

    table[winit::event::VirtualKeyCode::NextTrack as usize] = VirtualKeyCode::MediaNextTrack;
    table[winit::event::VirtualKeyCode::PrevTrack as usize] = VirtualKeyCode::MediaPrevTrack;
    table[winit::event::VirtualKeyCode::MediaStop as usize] = VirtualKeyCode::MediaStop;
    table[winit::event::VirtualKeyCode::PlayPause as usize] = VirtualKeyCode::MediaPlayPause;

    table
};
