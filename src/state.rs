use std::{
    path::Path,
    time::{Duration, Instant},
};

use egui::Color32;
use indexmap::IndexSet;
use twitch_message::{messages::Privmsg, IntoStatic};
use uuid::Uuid;

use crate::{queue::Queue, runtime::EmoteMap, twitch::Identity};

pub struct Message {
    pub id: Option<Uuid>,
    pub sender: String,
    pub color: Color32,
    pub badges: Vec<twitch_message::Badge<'static>>,
    pub data: String,
    pub spans: Vec<Span>,
    pub opts: MessageOpts,
}

impl Message {
    pub fn from_pm(pm: &Privmsg<'_>, emote_map: &mut EmoteMap, opts: MessageOpts) -> Self {
        fn parse_text(input: &str, spans: &mut Vec<Span>) {
            fn check_for_url(input: &str) -> bool {
                url::Url::parse(input)
                    .ok()
                    .filter(|url| matches!(url.scheme(), "http" | "https"))
                    .is_some()
            }

            let (mut cursor, mut pos) = (0, 0);
            let input = input.trim();
            let mut iter = input.split_ascii_whitespace().peekable();
            while let Some(el) = iter.next() {
                if check_for_url(el) {
                    pos += el.len() + 1;
                    cursor = pos;
                    spans.push(Span::Url(el.to_string()));
                    continue;
                }

                let Some(next) = iter.peek() else { continue };

                if check_for_url(next) {
                    spans.push(Span::Text(input[cursor..pos + el.len()].to_string()));
                    (cursor, pos) = (pos, pos + el.len() + 1);
                    continue;
                }
                pos += el.len() + 1;
            }

            if cursor < input.len() {
                spans.push(Span::Text(input[cursor..].to_string()));
            }
        }

        let mut emotes = pm.emotes().collect::<Vec<_>>();
        let data = &*pm.data;

        emotes.sort_unstable_by_key(|emote| emote.byte_pos);

        let mut spans = vec![];
        let mut cursor = 0;

        for ((emote_id, emote_name), (start, end)) in emotes
            .into_iter()
            .map(|emote| ((emote.id, emote.name), emote.byte_pos))
        {
            if start != cursor {
                let s = &data[cursor..start];
                parse_text(s, &mut spans);
            }

            emote_map.insert_emote(emote_id.as_str(), &emote_name);

            spans.push(Span::Emote((
                emote_id.to_string(),
                data[start..end].to_string(),
            )));

            cursor = end;
        }

        if cursor != data.len() {
            let s = &data[cursor..];
            parse_text(s, &mut spans);
        }

        Self {
            id: pm.msg_id().and_then(|s| Uuid::parse_str(s.as_str()).ok()),
            sender: pm.sender.to_string(),
            color: Self::translate_color(pm.color()),
            data: pm.data.to_string(),
            badges: pm.badges().map(IntoStatic::into_static).collect(),
            opts,
            spans,
        }
    }

    fn translate_color(color: Option<twitch_message::Color>) -> Color32 {
        let twitch_message::Color(r, g, b) = color.unwrap_or_default();
        Color32::from_rgb(r, g, b)
    }
}

pub struct MessageOpts {
    pub old: bool,
    pub local: bool,
}

pub enum Span {
    Text(String),
    Emote((String, String)),
    Url(String),
}

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

pub struct SavedState<'a> {
    pub state: &'a State,
}

impl<'a> SavedState<'a> {
    pub fn save(&self, path: impl AsRef<Path>) {
        #[derive(serde::Serialize)]
        struct Saved<'a> {
            channels: IndexSet<&'a str>,
            active: usize,
        }

        let s = toml::to_string_pretty(&Saved {
            active: self.state.active,
            channels: self.state.channels.iter().map(|s| &*s.name).collect(),
        })
        .expect("valid serialization");

        let _ = std::fs::write(path, s);
    }

    pub fn load(path: impl AsRef<Path>) -> Option<State> {
        let data = std::fs::read_to_string(path).ok()?;
        #[derive(serde::Deserialize)]
        struct Loaded {
            #[serde(default)]
            channels: IndexSet<String>,
            #[serde(default)]
            active: usize,
        }
        toml::from_str::<Loaded>(&data).ok().map(|loaded| State {
            active: loaded.active.min(loaded.channels.len().saturating_sub(1)),
            channels: loaded
                .channels
                .into_iter()
                .map(|ch| Channel::new(&ch))
                .collect(),
            identity: None,
        })
    }
}

#[derive(Default, Debug)]
pub enum Screen {
    #[default]
    Disconnected,
    Reconnecting {
        when: Instant,
        after: Duration,
    },
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
