use geometry::{Extent, Point};
use shell::{
    ButtonState, MouseButton, VirtualKeyCode, Window, WindowControl, WindowDesc, WindowHandler,
};

fn main() {
    let main_window = WindowDesc {
        title: "Sandbox",
        size: Extent::new(1280, 720),
        min_size: None,
        max_size: None,
        position: None,
        resizable: true,
        visible: true,
        transparent: false,
        always_on_top: false,
        handler: Box::new(AppWindow::new()),
    };

    shell::run([main_window]);
}

struct AppWindow {
    window: Option<Window>,
    click_count: u64,
}

impl AppWindow {
    pub fn new() -> Self {
        Self {
            window: None,
            click_count: 0,
        }
    }
}

impl WindowHandler for AppWindow {
    fn on_create(&mut self, _control: &mut dyn WindowControl, window: Window) {
        self.window = Some(window);
    }

    fn on_destroy(&mut self) {
        // no-op
    }

    fn on_close_request(&mut self, _control: &mut dyn WindowControl) -> bool {
        // always close the window opon request
        true
    }

    fn on_mouse_button(
        &mut self,
        control: &mut dyn WindowControl,
        button: MouseButton,
        state: ButtonState,
        _at: Point<i32>,
    ) {
        match button {
            MouseButton::Left => {
                if ButtonState::Released == state {
                    self.click_count += 1;
                    self.window
                        .as_mut()
                        .unwrap()
                        .set_title(&format!("Sandbox-Child-{}", self.click_count));
                }
            }
            MouseButton::Middle => {}
            MouseButton::Right => {
                if ButtonState::Released == state {
                    control.spawn(WindowDesc {
                        title: "Sandbox-Child",
                        size: Extent::new(1280, 720),
                        min_size: None,
                        max_size: None,
                        position: None,
                        resizable: true,
                        visible: true,
                        transparent: false,
                        always_on_top: false,
                        handler: Box::new(AppWindow::new()),
                    });
                }
            }
            MouseButton::Other(_) => {}
        }
    }

    fn on_cursor_move(&mut self, _control: &mut dyn WindowControl, _at: Point<i32>) {
        // no-op
    }

    fn on_key(&mut self, control: &mut dyn WindowControl, key: VirtualKeyCode, state: ButtonState) {
        match key {
            VirtualKeyCode::Escape => {
                if ButtonState::Pressed == state {
                    control.destroy(self.window.as_ref().unwrap().id());
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
                        resizable: true,
                        visible: true,
                        transparent: false,
                        always_on_top: false,
                        handler: Box::new(AppWindow::new()),
                    });
                }
            }
            _ => {}
        }
    }

    fn on_resize(&mut self, _control: &mut dyn WindowControl, _inner_size: Extent<u32>) {
        // no-op
    }

    fn on_rescale(
        &mut self,
        _control: &mut dyn WindowControl,
        _scale_factor: f64,
        _new_inner_size: Extent<u32>,
    ) {
        // no-op
    }

    fn on_redraw(&mut self, _control: &mut dyn WindowControl) {
        // no-op
    }
}
