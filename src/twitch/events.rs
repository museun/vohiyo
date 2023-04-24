use std::time::Duration;

use tokio::sync::mpsc::UnboundedReceiver;
use twitch_message::messages::{Privmsg, UserState};

pub enum Event {
    Connecting,
    Connected { identity: super::Identity },
    Privmsg { msg: Privmsg<'static> },
    Join { channel: String },
    ChannelId { channel: String, room_id: String },
    UserState { msg: UserState<'static> },
    Reconnecting { duration: Duration },
}

pub struct Events {
    pub(in crate::twitch) recv: UnboundedReceiver<Event>,
}

impl Events {
    pub fn poll(&mut self) -> Option<Event> {
        self.recv.try_recv().ok()
    }
}
