use tokio::sync::mpsc::UnboundedSender;

#[derive(Clone)]
pub struct Writer {
    pub(in crate::twitch) send: UnboundedSender<WriteKind>,
}

pub(in crate::twitch) enum WriteKind {
    Join { channel: String },
    Part { channel: String },
    Privmsg { target: String, data: String },
}

impl Writer {
    pub fn privmsg(&self, target: impl ToString, data: impl ToString) {
        let _ = self.send.send(WriteKind::Privmsg {
            target: target.to_string(),
            data: data.to_string(),
        });
    }

    pub fn join(&self, channel: impl ToString) {
        let _ = self.send.send(WriteKind::Join {
            channel: channel.to_string(),
        });
    }

    pub fn part(&self, channel: impl ToString) {
        let _ = self.send.send(WriteKind::Part {
            channel: channel.to_string(),
        });
    }
}
