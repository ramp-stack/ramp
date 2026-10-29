use futures::channel::mpsc::UnboundedSender;
use maverick_os::Window;
use prism::drawable::Instruction;

use super::Action;

pub struct NativeText;
impl NativeText {
    pub fn new(_window: &Window, _sender: UnboundedSender<Action>, _revision: u64) -> Self {Self}
    pub fn update(&mut self, _instructions: &[Instruction], _revision: u64) {}
}
