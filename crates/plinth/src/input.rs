/// Mouse buttons (e.g. left, right, middle, etc.)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    Other(u16),
}

/// The state of a button or key (e.g. pressed, released, repeated)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ButtonState {
    Pressed,
    Released,
    /// The number of 'repeat' cycles a button has been pressed for. The
    /// frequency of these cycles is operating system dependent and may be
    /// changed by the user.
    Repeated(u16),
}

/// The symbolic (read: English) name for a key on the keyboard.
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

pub(crate) const KEY_MAP: [VirtualKeyCode; 163] = {
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
