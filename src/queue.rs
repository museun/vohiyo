use std::collections::VecDeque;

use twitch_message::messages::Privmsg;

use crate::{runtime::EmoteMap, state::MessageOpts};

pub struct Queue<T> {
    inner: VecDeque<T>,
    max: usize,
}

impl<T> Queue<T> {
    pub fn with_capacity(max: usize) -> Self {
        assert!(max > 0, "max cannot be zero");
        Self {
            inner: VecDeque::with_capacity(max),
            max,
        }
    }

    pub fn push(&mut self, item: T) {
        while self.inner.len() >= self.max {
            self.inner.pop_front();
        }
        self.inner.push_back(item);
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> + ExactSizeIterator {
        self.inner.iter()
    }
}

impl Queue<crate::state::Message> {
    pub fn populate(
        &mut self,
        iter: impl IntoIterator<Item = crate::db::Message>,
        emote_map: &mut EmoteMap,
    ) {
        self.inner.extend(iter.into_iter().map(|msg| {
            let msg = twitch_message::parse_as::<Privmsg>(&msg.raw).unwrap();
            crate::state::Message::from_pm(
                &msg,
                emote_map,
                MessageOpts {
                    old: true,
                    local: false,
                },
            )
        }));

        let len = self.inner.len();
        if len >= self.max {
            self.inner.drain(..len - self.max);
        }
    }
}
