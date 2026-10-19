use maverick_os::Window;
use prism::drawable::Instruction;

pub struct Renderer {window: Window, renderer: wgpu_prism::Renderer<'static>}
impl maverick_os::Renderer for Renderer {
    type Instructions = Vec<Instruction>;

    fn new(window: Window) -> Self {
        let size = window.physical();
        let renderer = futures::executor::block_on(wgpu_prism::Renderer::new(
            window.clone(), size.width, size.height
        ));
        Self {window, renderer}
    }
    fn resize(&mut self) {
        let size = self.window.physical();
        self.renderer.resize(size.width, size.height);
    }
    fn draw(&mut self, instructions: Vec<Instruction>) {
        let size = self.window.physical();
        if size.width == 0 || size.height == 0 {return}
        self.renderer.draw(instructions, size.scale_factor as f32);
    }
}
