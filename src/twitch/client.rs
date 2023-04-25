use std::time::Instant;

use tokio::sync::{mpsc::unbounded_channel, oneshot};
use twitch_message::builders::{PrivmsgBuilder, TagsBuilder};

use crate::repaint::Repaint;

use super::{Config, Event, Events, Identity, Message, Signal, Status, Writer};

pub struct Client {
    events: Events,
    writer: Writer,
    signal: Option<oneshot::Sender<Signal>>,
    status: Status,
    config: Config,
}

impl Client {
    pub fn create(config: Config, repaint: impl Repaint) -> Self {
        let (read, recv) = unbounded_channel();
        let (send, write) = unbounded_channel();

        let (signal_tx, signal_rx) = oneshot::channel();

        tokio::spawn({
            let config = config.clone();
            async move {
                let wait = async move { signal_rx.await.unwrap_or(Signal::Ignore) };
                super::run(wait, config, repaint, read, write).await
            }
        });

        Self {
            events: Events { recv },
            writer: Writer { send },
            signal: Some(signal_tx),
            status: Status::default(),
            config,
        }
    }

    pub fn user_name(&self) -> &str {
        &self.config.user_name
    }

    pub fn connect(&mut self) {
        if let Some(signal) = self.signal.take() {
            let _ = signal.send(Signal::Start);
        }
    }

    pub const fn status(&self) -> Status {
        self.status
    }

    pub const fn writer(&self) -> &Writer {
        &self.writer
    }

    pub(crate) fn poll(
        &mut self,
        identity: &mut Option<Identity>,
        last: &mut Option<(PrivmsgBuilder, TagsBuilder)>,
    ) -> Option<Message> {
        self.status = match self.events.poll()? {
            Event::Connecting => {
                eprintln!("status: connecting");
                Status::Connecting
            }

            Event::Connected { identity: new } => {
                eprintln!("status: connected: {new:#?}");
                let _ = identity.replace(new);
                Status::Connected
            }

            Event::Reconnecting { duration } => {
                eprintln!("status: reconnecting: {duration:.2?}");
                Status::Reconnecting {
                    when: Instant::now(),
                    after: duration,
                }
            }

            Event::UserState { msg } => {
                let identity = identity
                    .as_mut()
                    .expect("we should have an identity at this point");
                identity.append_badges(&msg.channel, msg.badges());

                // app.state.channels[app.state.active].messages.push(msg);

                if let Some((pm, tags)) = last.take() {
                    let tags = tags
                        .add(
                            "id",
                            msg.msg_id()
                                .map(<twitch_message::messages::MsgIdRef>::to_string)
                                .expect("msg-id attached"),
                        )
                        .finish();

                    let pm = pm.tags(tags).finish_privmsg().expect("valid pm");
                    return Some(Message::Finished { msg: pm });
                }

                return None;
            }

            Event::ChannelId {
                channel: _,
                room_id: _,
            } => {
                return None;
            }

            Event::InvalidCredentials => return Some(Message::InvalidCredentials),
            Event::Join { channel } => return Some(Message::Join { channel }),
            Event::Privmsg { msg } => return Some(Message::Privmsg { msg }),
        };

        None
    }
}
