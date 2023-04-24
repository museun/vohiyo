use std::{
    collections::HashSet,
    future::Future,
    time::{Duration, Instant},
};

use hashbrown::HashMap;
use tokio::{
    io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt},
    sync::{
        mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
        oneshot,
    },
};
use twitch_message::{
    builders::{PrivmsgBuilder, TagsBuilder},
    encode::{join, part, ping, privmsg, register, ALL_CAPABILITIES},
    messages::{Privmsg, TwitchMessage, UserState},
    Color, IntoStatic, ParseResult, PingTracker,
};

use crate::{
    repaint::Repaint,
    util::{select2, Either},
};

#[derive(Copy, Clone, Debug, Default)]
pub enum Status {
    #[default]
    NotConnected,
    Connecting,
    Connected,
    Reconnecting {
        when: Instant,
        after: Duration,
    },
}

#[derive(Copy, Clone, Debug, Default)]
pub enum Signal {
    Start,
    #[default]
    Ignore,
}

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
                run(wait, config, repaint, read, write).await
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
        &self.config.name
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

            Event::Join { channel } => return Some(Message::Join { channel }),
            Event::Privmsg { msg } => return Some(Message::Privmsg { msg }),
        };

        None
    }
}

#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum Message {
    Join { channel: String },
    Privmsg { msg: Privmsg<'static> },
    Finished { msg: Privmsg<'static> },
}

#[derive(Clone)]
pub struct Config {
    pub name: String,
    pub token: String,
}

async fn run(
    signal: impl Future<Output = Signal> + Send + 'static,
    config: Config,
    repaint: impl Repaint,
    read: UnboundedSender<Event>,
    mut write: UnboundedReceiver<WriteKind>,
) {
    const RECONNECT: Duration = Duration::from_secs(5);

    let mut active_channels = <HashSet<String>>::new();

    eprintln!("waiting for the start signal");
    if matches!(signal.await, Signal::Ignore) {
        return;
    }
    eprintln!("got start signal");

    'outer: loop {
        #[rustfmt::skip]
        macro_rules! reconnect {
            () => {
                let event = Event::Reconnecting { duration: RECONNECT };
                if read.send(event).is_err() { break; }
                repaint.repaint();
                tokio::time::sleep(RECONNECT).await;
                repaint.repaint();
                continue 'outer;
            };
        }

        while let Ok(msg) = write.try_recv() {
            match msg {
                WriteKind::Join { channel } => active_channels.insert(channel),
                WriteKind::Part { channel } => active_channels.remove(&channel),
                _ => continue 'outer,
            };
        }

        if read.send(Event::Connecting).is_err() {
            break;
        }

        let mut stream =
            match tokio::net::TcpStream::connect(twitch_message::TWITCH_IRC_ADDRESS).await {
                Ok(stream) => stream,
                Err(err) => {
                    eprintln!("cannot connect: {err}");
                    reconnect!();
                }
            };

        let (stream_read, mut stream_write) = stream.split();

        let register = register(&config.name, &config.token, ALL_CAPABILITIES).to_string();
        if let Err(err) = write_all(register, &mut stream_write).await {
            eprintln!("cannot write: {err}");
            reconnect!();
        }

        let mut reader = tokio::io::BufReader::new(stream_read).lines();

        let ping_timeout = Duration::from_secs(30);
        let pt = PingTracker::new(ping_timeout * 2);

        let mut our_name = <Option<String>>::None;
        let start = Instant::now();

        'inner: loop {
            let mut write_fut = std::pin::pin!(write.recv());
            let mut read_fut = std::pin::pin!(reader.next_line());

            let timeout =
                tokio::time::timeout(ping_timeout, select2(&mut write_fut, &mut read_fut));
            match if let Ok(ev) = timeout.await {
                ev
            } else {
                if pt.probably_timed_out() {
                    eprintln!("connection timed out");
                    reconnect!();
                }

                let ping = ping(&start.elapsed().as_secs().to_string()).to_string();
                if write_all(ping, &mut stream_write).await.is_err() {
                    eprintln!("cannot write");
                    reconnect!();
                }
                continue 'inner;
            } {
                Either::Left(Some(write)) => match write {
                    WriteKind::Join { channel } => {
                        active_channels.insert(channel.clone());
                        if let Err(err) =
                            write_all(join(&channel).to_string(), &mut stream_write).await
                        {
                            eprintln!("cannot write: {err}");
                            reconnect!();
                        }
                    }

                    WriteKind::Part { channel } => {
                        active_channels.remove(&channel);
                        if let Err(err) =
                            write_all(part(&channel).to_string(), &mut stream_write).await
                        {
                            eprintln!("cannot write: {err}");
                            reconnect!();
                        }
                    }

                    WriteKind::Privmsg { target, data } => {
                        if let Err(err) =
                            write_all(privmsg(&target, &data).to_string(), &mut stream_write).await
                        {
                            eprintln!("cannot write: {err}");
                            reconnect!();
                        }
                    }
                },

                Either::Right(Ok(Some(line))) => {
                    let msg = match twitch_message::parse(&line) {
                        Ok(ParseResult { message, .. }) => message,
                        Err(err) => {
                            eprintln!("cannot parse '{}' : {err}", line.escape_debug());
                            reconnect!();
                        }
                    };

                    pt.update(&msg);

                    let pong = pt.should_pong();
                    if let Some(pong) = pong {
                        if write_all(pong.to_string(), &mut stream_write)
                            .await
                            .is_err()
                        {
                            eprintln!("cannot write");
                            reconnect!();
                        }
                    }

                    macro_rules! send_event {
                        ($ev:expr) => {
                            if read.send($ev).is_err() {
                                break 'outer;
                            }
                            repaint.repaint();
                        };
                    }

                    eprintln!(">{msg}", msg = msg.raw.escape_debug());

                    match msg.as_enum() {
                        TwitchMessage::Privmsg(msg) => {
                            let msg = msg.into_static();
                            if read.send(Event::Privmsg { msg }).is_err() {
                                break 'outer;
                            }
                            repaint.repaint();
                        }

                        TwitchMessage::Ready(msg) => {
                            let _ = our_name.replace(msg.name.to_string());
                        }

                        TwitchMessage::Join(msg) if Some(&*msg.user) == our_name.as_deref() => {
                            send_event!(Event::Join {
                                channel: msg.channel.to_string()
                            });
                        }

                        TwitchMessage::RoomState(msg) => {
                            send_event!(Event::ChannelId {
                                channel: msg.channel.to_string(),
                                room_id: msg.room_id().expect("room-id attached").to_string(),
                            });
                        }

                        TwitchMessage::UserState(msg) => {
                            send_event!(Event::UserState {
                                msg: msg.into_static(),
                            });
                        }

                        TwitchMessage::GlobalUserState(msg) => {
                            let our_name = our_name.clone().expect("message ordering");
                            let identity = Identity {
                                name: our_name.clone(),
                                display_name: msg.display_name().map(ToString::to_string),
                                user_id: msg
                                    .user_id()
                                    .map(ToString::to_string)
                                    .expect("we should have a user id"),
                                color: msg.color(),
                                emote_sets: msg.emote_sets().map(ToString::to_string).collect(),
                                badge_map: std::iter::once((
                                    our_name,
                                    msg.badges()
                                        .map(|twitch_message::Badge { name, version }| {
                                            (name.to_string(), version.to_string())
                                        })
                                        .collect(),
                                ))
                                .collect(),
                            };

                            send_event!(Event::Connected { identity });

                            for channel in &active_channels {
                                eprintln!("joining: {channel}");
                                let join = join(channel).to_string();
                                if let Err(err) = write_all(join, &mut stream_write).await {
                                    eprintln!("cannot write: {err}");
                                    reconnect!();
                                }
                            }
                        }
                        _ => {}
                    }
                }

                Either::Left(None) => {
                    break 'outer;
                }

                Either::Right(..) => {
                    reconnect!();
                }
            }
        }
    }
}

async fn write_all(
    s: impl AsRef<[u8]> + Send + Sync,
    w: &mut (impl AsyncWrite + Unpin + Send + Sync),
) -> std::io::Result<()> {
    w.write_all(s.as_ref()).await?;
    w.flush().await
}

pub struct Events {
    recv: UnboundedReceiver<Event>,
}

impl Events {
    pub fn poll(&mut self) -> Option<Event> {
        self.recv.try_recv().ok()
    }
}

#[derive(Clone, Debug)]
pub struct Identity {
    pub name: String,
    pub display_name: Option<String>,
    pub user_id: String,
    pub color: Option<Color>,
    pub emote_sets: Vec<String>,
    badge_map: HashMap<String, HashMap<String, String>>,
}

impl Identity {
    pub fn append_badges<'a>(
        &mut self,
        channel: &str,
        badges: impl IntoIterator<Item = twitch_message::Badge<'a>>,
    ) {
        use hashbrown::hash_map::RawEntryMut::*;
        let channel = channel.strip_prefix('#').unwrap_or(channel);

        // TODO keep track of insertions so we can just .first() / .last() to get the best badge
        let map = self.badge_map.entry(channel.to_string()).or_default();
        for (set_id, id) in badges
            .into_iter()
            .map(|twitch_message::Badge { name, version }| (name, version))
        {
            match map.raw_entry_mut().from_key(set_id.as_str()) {
                Vacant(entry) => {
                    entry.insert(set_id.to_string(), id.to_string());
                }
                Occupied(mut entry) => {
                    *entry.get_mut() = id.to_string();
                }
            }
        }
    }

    pub fn get_badges_for(&self, channel: &str) -> impl Iterator<Item = (&str, &str)> {
        let channel = channel.strip_prefix('#').unwrap_or(channel);

        self.badge_map
            .get(channel)
            .into_iter()
            .flat_map(|inner| inner.iter().map(|(k, v)| (k.as_str(), v.as_str())))
    }
}

pub enum Event {
    Connecting,
    Connected { identity: Identity },
    Privmsg { msg: Privmsg<'static> },
    Join { channel: String },
    ChannelId { channel: String, room_id: String },
    UserState { msg: UserState<'static> },
    Reconnecting { duration: Duration },
}

#[derive(Clone)]
pub struct Writer {
    send: UnboundedSender<WriteKind>,
}

enum WriteKind {
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
