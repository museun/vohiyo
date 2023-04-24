use std::time::Duration;

use hashbrown::HashSet;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::{
    helix,
    repaint::Repaint,
    resolver,
    util::{select2, Either},
};

pub enum Action<T> {
    Added(T),
    Removed(T),
}

#[derive(Clone, Debug)]
pub struct StreamStatus {
    pub user_id: String,
}

pub struct StreamCheck {
    map: resolver::ResolverMap<
        String,
        Option<helix::data::Stream>,
        (String, Option<helix::data::Stream>),
    >,

    watching: UnboundedSender<Action<String>>,
    update: UnboundedReceiver<(String, Option<helix::data::Stream>)>,
    send: UnboundedSender<Action<StreamStatus>>,
    events: UnboundedReceiver<Action<StreamStatus>>,
}

impl StreamCheck {
    const STREAM_CHECK_DURATION: Duration = Duration::from_secs(30);
    const BURST_WINDOW: Duration = Duration::from_secs(1);

    pub fn create(helix: helix::Client, repaint: impl Repaint) -> Self {
        let (watching, rx) = unbounded_channel();
        let (resp, update) = unbounded_channel();
        let (send, events) = unbounded_channel();

        tokio::spawn(Self::poll_helix(helix, repaint, rx, resp));

        Self {
            map: resolver::ResolverMap::new(),
            watching,
            update,
            send,
            events,
        }
    }

    pub fn poll(&mut self) {
        while let Ok((id, stream)) = self.update.try_recv() {
            Self::update(&mut self.map.update(), &self.send, id, stream);
        }

        self.map
            .poll(|entry, (id, stream)| Self::update(entry, &self.send, id, stream));

        // TODO maybe poll the event here
    }

    pub fn poll_event(&mut self) -> Option<Action<StreamStatus>> {
        self.events.try_recv().ok()
    }

    pub fn get_or_subscribe(&mut self, user_id: &str) -> Option<&helix::data::Stream> {
        self.map
            .get_or_else(user_id, |user_id| {
                eprintln!("subscribing to events for stream: {user_id}");
                let _ = self.watching.send(Action::Added(user_id.to_string()));
            })?
            .as_ref()
    }

    pub fn unsubscribe(&self, user_id: &str) {
        let _ = self.watching.send(Action::Removed(user_id.to_string()));
    }

    async fn poll_helix(
        helix: helix::Client,
        repaint: impl Repaint,
        mut recv: UnboundedReceiver<Action<String>>,
        send: UnboundedSender<(String, Option<helix::data::Stream>)>,
    ) {
        let mut set = <HashSet<String>>::new();
        let mut queue = vec![];

        macro_rules! batch_send {
            ($set:expr) => {
                let mut delta = <HashSet<&str>>::from_iter($set.map(|s| &**s));
                let Some(streams) = helix.get_many_streams($set).wait().await else { continue };
                for stream in streams {
                    delta.remove(&*stream.user_id);
                    if send.send((stream.user_id.clone(), Some(stream))).is_err() {
                        break;
                    }
                }

                for remaining in delta {
                    if send.send((remaining.to_owned(), None)).is_err() {
                        break;
                    }
                }
            };
        }

        loop {
            let mut sleep = std::pin::pin!(tokio::time::sleep(Self::STREAM_CHECK_DURATION));
            let mut recv = std::pin::pin!(tokio::time::timeout(Self::BURST_WINDOW, recv.recv()));

            match select2(&mut sleep, &mut recv).await {
                Either::Left(_) => {
                    batch_send!(set.iter());
                    if !set.is_empty() {
                        repaint.repaint();
                    }
                }

                Either::Right(Ok(Some(action))) => {
                    let channel = match action {
                        Action::Added(channel) => channel,
                        Action::Removed(channel) => {
                            set.remove(&channel);
                            continue;
                        }
                    };

                    if set.insert(channel.clone()) {
                        queue.push(channel)
                    }
                }

                Either::Right(Err(..)) => {
                    if !queue.is_empty() {
                        batch_send!(queue.iter());
                        queue.clear();
                        repaint.repaint();
                    }
                }

                Either::Right(..) => break,
            }
        }
    }

    fn update(
        entry: &mut resolver::ResolverEntry<String, Option<helix::data::Stream>>,
        sender: &UnboundedSender<Action<StreamStatus>>,
        id: String,
        stream: Option<helix::data::Stream>,
    ) {
        let action = if stream.is_none() {
            Action::Removed
        } else {
            Action::Added
        }(StreamStatus {
            user_id: id.clone(),
        });

        entry.set(id, stream);
        let _ = sender.send(action);
    }
}
