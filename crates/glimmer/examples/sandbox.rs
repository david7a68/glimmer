use std::rc::Rc;

use geometry::{Extent, Point, ScreenSpace};
use graphics::{GraphicsConfig, GraphicsContext, RenderGraph, Surface};
use shell::{
    ButtonState, MouseButton, VirtualKeyCode, Window, WindowDesc, WindowFlags, WindowHandler,
    WindowSpawner,
};

fn main() {
    let graphics = Rc::new(GraphicsContext::new(&GraphicsConfig {
        debug_mode: true,
        ..Default::default()
    }));

    let main_window = WindowDesc {
        title: "Sandbox",
        size: Extent::new(1280, 720),
        min_size: None,
        max_size: None,
        position: None,
        flags: WindowFlags::VISIBLE | WindowFlags::RESIZABLE,
        handler: &mut |window| {
            let surface = graphics.create_surface(&window);
            AppWindow::new(window, surface, graphics.clone())
        },
    };

    shell::run([main_window]);
}

struct AppWindow {
    window: Window,
    surface: Surface,
    graphics: Rc<GraphicsContext>,
}

impl AppWindow {
    pub fn new(window: Window, surface: Surface, graphics: Rc<GraphicsContext>) -> Self {
        Self {
            window,
            surface,
            graphics,
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
        _button: MouseButton,
        _state: ButtonState,
        _at: Point<i32, ScreenSpace>,
    ) {
        // no-op
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
                        handler: &mut |window| {
                            let surface = self.graphics.create_surface(&window);
                            AppWindow::new(window, surface, self.graphics.clone())
                        },
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
        self.surface.resize();
    }

    fn on_rescale(
        &mut self,
        _control: &mut dyn WindowSpawner<Self>,
        _scale_factor: f64,
        _new_inner_size: Extent<u32, ScreenSpace>,
    ) {
        // no-op
    }

    fn on_idle(&mut self, _spawner: &mut dyn WindowSpawner<Self>) {
        // no-op
        // self.window.request_redraw();
    }

    fn on_redraw(&mut self, _control: &mut dyn WindowSpawner<Self>) {
        let image = self.surface.get_next_image();

        let render_graph = RenderGraph::new();

        self.graphics.draw(image.image(), &render_graph);

        image.present();
    }
}
