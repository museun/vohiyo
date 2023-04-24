use uuid::Uuid;

use crate::queue::Queue;

use super::Message;

pub struct Channel {
    pub name: String,
    pub buffer: String,
    pub marker: Option<Uuid>,
    pub messages: Queue<Message>,
}

impl Channel {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.strip_prefix('#').unwrap_or(name).to_string(),
            marker: None,
            buffer: String::with_capacity(100),
            messages: Queue::with_capacity(1000),
        }
    }

    pub fn push(&mut self, message: Message) {
        self.marker.take();
        self.messages.push(message)
    }

    pub fn mark_end_of_history(&mut self, uuid: Uuid) {
        self.marker.replace(uuid);
    }
}
