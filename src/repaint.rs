use std::sync::Arc;

pub trait Repaint: Sized + Send + Sync + 'static {
    fn repaint(&self) {}

    fn erased(self) -> ErasedRepaint {
        let this = self;
        Arc::new(move || this.repaint())
    }
}

impl Repaint for () {}

impl Repaint for egui::Context {
    fn repaint(&self) {
        self.request_repaint();
    }
}

pub type ErasedRepaint = Arc<dyn Fn() + Send + Sync + 'static>;
