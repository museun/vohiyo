use crate::twitch::Identity;

mod message;
pub use message::{Message, MessageOpts, Span};

mod channel;
pub use channel::Channel;

#[derive(Debug, Default)]
pub enum Screen {
    #[default]
    Disconnected,
    Connected {
        state: ViewState,
    },
    InvalidCredentials {
        kind: CredentialsKind,
    },
}

#[derive(Clone, Debug)]
pub enum CredentialsKind {
    Twitch,
    Helix,
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
