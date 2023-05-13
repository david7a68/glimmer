use std::{cell::RefCell, collections::HashMap, rc::Rc};

use geometry::{Extent, Point, Rect, ScreenPx};
use visuals::{
    window::{
        ButtonState, MouseButton, VirtualKeyCode, Window, WindowDesc, WindowFlags, WindowHandler,
        WindowSpawner,
    },
    Color, DrawRect, GraphicsConfig, GraphicsContext, Image, PixelBuffer,
    RectPart::{BottomLeft, BottomRight, Left, Right, TopLeft, TopRight},
    RenderGraph, RenderGraphNodeId, Surface,
};

#[derive(Clone)]
struct IoRequester {
    sender: crossbeam::channel::Sender<String>,
    receiver: crossbeam::channel::Receiver<(String, Option<PixelBuffer>)>,
}

struct ImageCache {
    cache: RefCell<HashMap<String, ImageState>>,
    // default_image: Image,
    io_requester: IoRequester,
}

#[derive(Clone, Copy)]
enum ImageState {
    Pending,
    Ready(Image),
}

impl ImageCache {
    fn new(io_requester: IoRequester) -> Self {
        Self {
            cache: RefCell::new(HashMap::new()),
            // default_image: Image::new(),
            io_requester,
        }
    }

    fn get(&self, graphics: &GraphicsContext, path: &str) -> Option<Image> {
        let image = self.cache.borrow().get(path).cloned();

        match image {
            Some(ImageState::Ready(image)) => Some(image),
            Some(ImageState::Pending) => {
                let mut found = None;

                while let Ok((p, Some(buf))) = self.io_requester.receiver.try_recv() {
                    let image = graphics.upload_image(buf.as_ref());

                    if p == path {
                        found = Some(image);
                    }

                    self.cache.borrow_mut().insert(p, ImageState::Ready(image));
                }

                found
            }
            None => {
                self.cache
                    .borrow_mut()
                    .insert(path.to_string(), ImageState::Pending);
                self.io_requester.sender.send(path.to_string()).unwrap();
                None
            }
        }
    }
}

fn spawn_loader_thread() -> IoRequester {
    let (request_sender, request_receiver) = crossbeam::channel::unbounded();
    let (result_sender, result_receiver) = crossbeam::channel::unbounded();

    // Thread will be dropped automatically when the request sender is dropped.
    let _thread = std::thread::spawn(move || {
        while let Ok(request) = request_receiver.recv() {
            let buf = PixelBuffer::from_file(&std::fs::read(&request).unwrap());
            let Ok(_) = result_sender.send((request, Some(buf))) else { break; };
        }
    });

    IoRequester {
        sender: request_sender,
        receiver: result_receiver,
    }
}

fn main() {
    let graphics = Rc::new(GraphicsContext::new(&GraphicsConfig {
        debug_mode: true,
        ..Default::default()
    }));

    let io_requester = spawn_loader_thread();
    let image_cache = Rc::new(ImageCache::new(io_requester));

    let main_window = WindowDesc {
        title: "Sandbox",
        size: Extent::new(1280, 720),
        min_size: None,
        max_size: None,
        position: None,
        flags: WindowFlags::VISIBLE | WindowFlags::RESIZABLE,
        handler: &mut |window| {
            let surface = graphics.create_surface(&window);
            AppWindow::new(window, surface, graphics.clone(), image_cache.clone())
        },
    };

    visuals::window::enter_event_loop([main_window]);
}

struct AppWindow {
    window: Window,
    surface: Surface,
    graphics: Rc<GraphicsContext>,
    image_cache: Rc<ImageCache>,
}

impl AppWindow {
    pub fn new(
        window: Window,
        surface: Surface,
        graphics: Rc<GraphicsContext>,
        image_cache: Rc<ImageCache>,
    ) -> Self {
        Self {
            window,
            surface,
            graphics,
            image_cache,
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
        _at: Point<i32, ScreenPx>,
    ) {
        // no-op
    }

    fn on_cursor_move(
        &mut self,
        _control: &mut dyn WindowSpawner<Self>,
        _at: Point<i32, ScreenPx>,
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
                            AppWindow::new(
                                window,
                                surface,
                                self.graphics.clone(),
                                self.image_cache.clone(),
                            )
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
        _inner_size: Extent<u32, ScreenPx>,
    ) {
        self.graphics.resize(&mut self.surface);
    }

    fn on_rescale(
        &mut self,
        _control: &mut dyn WindowSpawner<Self>,
        _scale_factor: f64,
        _new_inner_size: Extent<u32, ScreenPx>,
    ) {
        // no-op
    }

    fn on_idle(&mut self, _spawner: &mut dyn WindowSpawner<Self>) {
        // no-op
        self.window.request_redraw();
    }

    fn on_redraw(&mut self, _control: &mut dyn WindowSpawner<Self>) {
        let image = self.graphics.get_next_image(&mut self.surface);

        let mut render_graph = RenderGraph::new();

        let _i = self.image_cache.get(&self.graphics, "D:/test.png");

        render_graph.draw_rect(
            RenderGraphNodeId::root(),
            &DrawRect::new(Rect::new(
                Point::new(800.0, 100.0),
                Extent::new(400.0, 400.0),
            ))
            .with_colors([
                TopLeft(Color::RED),
                TopRight(Color::GREEN),
                BottomLeft(Color::BLUE),
                BottomRight(Color::BLACK),
            ])
            .with_radii([BottomRight(20.0), Left(300.0)]),
        );

        render_graph.draw_rect(
            RenderGraphNodeId::root(),
            &DrawRect::new(Rect::new(
                Point::new(400.0, 100.0),
                Extent::new(400.0, 400.0),
            ))
            .with_radii([BottomLeft(20.0), Right(300.0)])
            .with_colors([
                TopLeft(Color::RED),
                TopRight(Color::GREEN),
                BottomLeft(Color::BLUE),
                BottomRight(Color::BLACK),
            ]),
        );

        self.graphics.draw(&image, &render_graph);

        self.graphics.present(&mut self.surface);
    }
}

impl Drop for AppWindow {
    fn drop(&mut self) {
        self.graphics.destroy_surface(&mut self.surface);
    }
}
