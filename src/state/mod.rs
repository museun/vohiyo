use crate::twitch::Identity;

mod message;
pub use message::{Message, MessageOpts, Span};

mod channel;
pub use channel::Channel;

mod save_state;
pub use save_state::SavedState;

#[derive(Default, Debug)]
pub enum Screen {
    #[default]
    Disconnected,
    Connected {
        state: ViewState,
    },
}

#[derive(Debug)]
pub enum ViewState {
    Empty { buffer: String },
    MainView,
}

#[derive(Default)]
pub struct State {
    pub channels: Vec<Channel>,
    pub active: usize,
    pub identity: Option<Identity>,
}
