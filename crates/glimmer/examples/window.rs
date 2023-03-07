use geometry::{Extent, Point, ScreenSpace};
use shell::{
    ButtonState, MouseButton, VirtualKeyCode, Window, WindowDesc, WindowFlags, WindowHandler,
    WindowSpawner,
};

fn main() {
    let main_window = WindowDesc {
        title: "Sandbox",
        size: Extent::new(1280, 720),
        min_size: None,
        max_size: None,
        position: None,
        flags: WindowFlags::VISIBLE | WindowFlags::RESIZABLE,
        handler: &mut AppWindow::new,
    };

    shell::run([main_window]);
}

struct AppWindow {
    window: Window,
    click_count: u64,
}

impl AppWindow {
    pub fn new(window: Window) -> Self {
        Self {
            window,
            click_count: 0,
        }
    }
}

impl WindowHandler for AppWindow {
    fn on_destroy(&mut self) {
        // no-op
    }

    fn on_close_request(&mut self, _control: &mut dyn WindowSpawner<Self>) -> bool {
        // always close the window opon request
        true
    }

    fn on_mouse_button(
        &mut self,
        _control: &mut dyn WindowSpawner<Self>,
        button: MouseButton,
        state: ButtonState,
        _at: Point<i32, ScreenSpace>,
    ) {
        match button {
            MouseButton::Left => {
                if ButtonState::Released == state {
                    self.click_count += 1;
                    self.window
                        .set_title(&format!("Sandbox-Child-{}", self.click_count));
                }
            }
            MouseButton::Middle => {}
            MouseButton::Right => {}
            MouseButton::Other(_) => {}
        }
    }

    fn on_cursor_move(
        &mut self,
        _control: &mut dyn WindowSpawner<Self>,
        _at: Point<i32, ScreenSpace>,
    ) {
        // no-op
    }

    fn on_key(
        &mut self,
        control: &mut dyn WindowSpawner<Self>,
        key: VirtualKeyCode,
        state: ButtonState,
    ) {
        match key {
            VirtualKeyCode::Escape => {
                if ButtonState::Pressed == state {
                    self.window.destroy();
                }
            }
            VirtualKeyCode::N => {
                if ButtonState::Released == state {
                    control.spawn(WindowDesc {
                        title: "Sandbox-Child",
                        size: Extent::new(1280, 720),
                        min_size: None,
                        max_size: None,
                        position: None,
                        flags: WindowFlags::VISIBLE | WindowFlags::RESIZABLE,
                        handler: &mut AppWindow::new,
                    });
                }
            }
            _ => {}
        }

        if let ButtonState::Repeated(count) = state {
            println!("Key {:?} repeated {} times", key, count);
        }
    }

    fn on_resize(
        &mut self,
        _control: &mut dyn WindowSpawner<Self>,
        _inner_size: Extent<u32, ScreenSpace>,
    ) {
        // no-op
    }

    fn on_rescale(
        &mut self,
        _control: &mut dyn WindowSpawner<Self>,
        _scale_factor: f64,
        _new_inner_size: Extent<u32, ScreenSpace>,
    ) {
        // no-op
    }

    fn on_idle(&mut self, _spawner: &mut dyn WindowSpawner<Self>) {}

    fn on_redraw(&mut self, _control: &mut dyn WindowSpawner<Self>) {
        // no-op
    }
}
