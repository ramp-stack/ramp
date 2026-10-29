use futures::channel::mpsc::UnboundedSender;
use maverick_os::Window;
use prism::drawable::Instruction;

use crate::native_text::{Action, NativeText};

/// Drawing output and the private callback route back to its owning application.
pub struct Frame {
    instructions: Vec<Instruction>,
    sender: UnboundedSender<Action>,
    /// Latest native edit consumed before these drawing instructions were produced.
    revision: u64,
}
impl Frame {
    pub(crate) fn new(instructions: Vec<Instruction>, sender: UnboundedSender<Action>, revision: u64) -> Self {
        Self {instructions, sender, revision}
    }
}

pub struct Renderer {
    window: Window,
    renderer: wgpu_prism::Renderer<'static>,
    native_text: Option<NativeText>,
}
impl maverick_os::Renderer for Renderer {
    type Instructions = Frame;

    fn new(window: Window) -> Self {
        let size = window.physical();
        let renderer = futures::executor::block_on(wgpu_prism::Renderer::new(
            window.clone(), size.width, size.height
        ));
        Self {window, renderer, native_text: None}
    }
    fn resize(&mut self) {
        let size = self.window.physical();
        self.renderer.resize(size.width, size.height);
    }
    fn update(&mut self, frame: Frame) -> bool {
        let size = self.window.physical();
        if size.width == 0 || size.height == 0 {return false}
        self.native_text.get_or_insert_with(|| NativeText::new(&self.window, frame.sender, frame.revision))
            .update(&frame.instructions, frame.revision);
        self.renderer.update(frame.instructions, size.scale_factor as f32)
    }
    fn draw(&mut self) -> bool {
        let size = self.window.physical();
        if size.width == 0 || size.height == 0 {return false}
        self.renderer.render()
    }
}
