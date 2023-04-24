use std::{
    collections::HashSet,
    future::Future,
    time::{Duration, Instant},
};

use tokio::{
    io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt},
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
};
use twitch_message::{
    encode::{join, part, ping, privmsg, register, ALL_CAPABILITIES},
    messages::{Privmsg, TwitchMessage},
    IntoStatic, ParseResult, PingTracker,
};

use crate::{
    repaint::Repaint,
    twitch::writer::WriteKind,
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

mod identity;
pub use identity::Identity;

mod events;
pub use events::{Event, Events};

mod writer;
pub use writer::Writer;

mod client;
pub use client::Client;
