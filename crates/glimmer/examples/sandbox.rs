use geometry::{Extent, Point};
use raw_window_handle::RawWindowHandle;
use shell::{WindowDesc, WindowHandler, WindowId};

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
        handler: Box::new(AppWindow { id: None }),
    };

    shell::run([main_window]);
}

struct AppWindow {
    id: Option<WindowId>,
}

impl WindowHandler for AppWindow {
    fn on_create(
        &mut self,
        _control: &mut dyn shell::WindowControl,
        id: WindowId,
        _handle: RawWindowHandle,
    ) {
        self.id = Some(id);
    }

    fn on_destroy(&mut self) {
        // no-op
    }

    fn on_close_request(&mut self, _control: &mut dyn shell::WindowControl) -> bool {
        // always close the window opon request
        true
    }

    fn on_mouse_button(
        &mut self,
        control: &mut dyn shell::WindowControl,
        button: shell::MouseButton,
        state: shell::ButtonState,
        _at: Point<i32>,
    ) {
        match button {
            shell::MouseButton::Left => {}
            shell::MouseButton::Middle => {}
            shell::MouseButton::Right => {
                if shell::ButtonState::Released == state {
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
                        handler: Box::new(AppWindow { id: None }),
                    });
                }
            }
            shell::MouseButton::Other(_) => {}
        }
    }

    fn on_cursor_move(&mut self, _control: &mut dyn shell::WindowControl, _at: Point<i32>) {
        // no-op
    }

    fn on_key(
        &mut self,
        control: &mut dyn shell::WindowControl,
        key: shell::VirtualKeyCode,
        state: shell::ButtonState,
    ) {
        match key {
            shell::VirtualKeyCode::Escape => {
                if shell::ButtonState::Pressed == state {
                    control.destroy(self.id.unwrap());
                }
            }
            shell::VirtualKeyCode::N => {
                if shell::ButtonState::Released == state {
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
                        handler: Box::new(AppWindow { id: None }),
                    });
                }
            }
            _ => {}
        }
    }

    fn on_resize(&mut self, _control: &mut dyn shell::WindowControl, _inner_size: Extent<u32>) {
        // no-op
    }

    fn on_rescale(
        &mut self,
        _control: &mut dyn shell::WindowControl,
        _scale_factor: f64,
        _new_inner_size: Extent<u32>,
    ) {
        // no-op
    }

    fn on_redraw(&mut self, _control: &mut dyn shell::WindowControl) {
        // no-op
    }
}
