use eframe::CreationContext;
use egui::{FontData, FontDefinitions, Key};
use reqwest::header::HeaderName;
use twitch_message::builders::{PrivmsgBuilder, TagsBuilder};

use crate::{
    db, helix,
    runtime::{EmoteMap, GameMap, ImageCache, StreamCheck, UserMap},
    state::{Channel, MessageOpts, SavedState, Screen, State, ViewState},
    twitch,
    views::{InitialView, MainView, StartView},
};

pub struct App {
    pub state: State,
    pub screen: Screen,
    pub helix: helix::Client,
    pub twitch: twitch::Client,
    pub stream_check: StreamCheck,
    pub cache: ImageCache,
    pub emote_map: EmoteMap,
    pub user_map: UserMap,
    pub game_map: GameMap,
    pub last: Option<(PrivmsgBuilder, TagsBuilder)>,
    pub conn: db::Connection,
}

impl App {
    pub const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

    pub fn create(cc: &CreationContext, config: twitch::Config) -> Box<dyn eframe::App> {
        cc.egui_ctx.set_pixels_per_point(1.5);
        Self::load_fonts(&cc.egui_ctx);

        let mut state = SavedState::load("vohiyo.toml").unwrap_or_default();

        let http = reqwest::ClientBuilder::new()
            .default_headers(
                std::iter::once((
                    HeaderName::from_static("user-agent"),
                    Self::USER_AGENT.parse().unwrap(),
                ))
                .collect(),
            )
            .build()
            .expect("valid client configuration");

        let helix = helix::Client::create(cc.egui_ctx.clone());
        let mut emote_map = EmoteMap::create(helix.clone(), cc.egui_ctx.clone(), http.clone());

        let conn = db::Connection::create("history.db");
        let history = conn.history();
        for channel in &mut state.channels {
            let messages = history.get_channel_messages(&channel.name, 250);
            if let Some(msg) = messages.last() {
                channel.mark_end_of_history(msg.msg_id);
            }
            channel.messages.populate(messages, &mut emote_map);
        }

        let twitch = twitch::Client::create(config, cc.egui_ctx.clone());

        let mut user_map = UserMap::create(helix.clone());

        for channel in state.channels.iter().map(|c| &c.name) {
            twitch.writer().join(channel);
            user_map.get(channel);
        }

        Box::new(Self {
            screen: Screen::default(),
            stream_check: StreamCheck::create(helix.clone(), cc.egui_ctx.clone()),
            cache: ImageCache::new(http, cc.egui_ctx.clone()),
            emote_map,
            game_map: GameMap::create(helix.clone()),
            user_map,

            state,
            twitch,
            helix,

            last: None,

            conn,
        })
    }

    fn load_fonts(ctx: &egui::Context) {
        let mut fonts = FontDefinitions::empty();

        macro_rules! load_font {
        ($($font:expr => $entry:expr),*) => {
            $(
                fonts.font_data.insert($font.into(), FontData::from_static(
                    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/fonts/", $font, ".ttf")))
                );
                fonts.families.entry($entry).or_default().push($font.into());
            )*
            ctx.set_fonts(fonts)
        };
    }

        load_font! {
            "Roboto-Regular"     => egui::FontFamily::Proportional,
            "RobotoMono-Regular" => egui::FontFamily::Monospace,
            "RobotoMono-Bold"    => egui::FontFamily::Name("bold".into())
        }
    }

    fn fetch_initial_emotes(&mut self) {
        for set in self
            .state
            .identity
            .as_ref()
            .into_iter()
            .flat_map(|s| &s.emote_sets)
        {
            self.emote_map.populate_emote_set(set)
        }
    }

    fn handle_keyboard_input(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.key_released(Key::F12)) {
            ctx.set_debug_on_hover(!ctx.debug_on_hover())
        }
    }

    fn handle_message(&mut self, message: twitch::Message) {
        match message {
            twitch::Message::Join { channel } => {
                if let Some(pos) = self.state.channels.iter().position(|p| {
                    p.name.strip_prefix('#').unwrap_or(&p.name)
                        == channel.strip_prefix('#').unwrap_or(&channel)
                }) {
                    self.state.active = pos;
                } else {
                    let pos = self.state.channels.len();
                    self.state.channels.push(Channel::new(&channel));
                    self.state.active = pos;
                    self.user_map
                        .get(channel.strip_prefix('#').unwrap_or(&channel));
                }
            }

            this @ (twitch::Message::Finished { .. } | twitch::Message::Privmsg { .. }) => {
                let local = matches!(this, twitch::Message::Finished { .. });
                let (twitch::Message::Finished { msg }
                | twitch::Message::Privmsg { msg }) = this
                else { unreachable!() };

                self.conn.history().insert(&msg);

                let channel = self
                    .state
                    .channels
                    .iter_mut()
                    .find(|c| c.name == msg.channel.strip_prefix('#').unwrap_or(&msg.channel))
                    .unwrap_or_else(|| {
                        panic!(
                            "we should be on this channel: {channel}",
                            channel = msg.channel
                        )
                    });

                if !local {
                    channel.push(crate::state::Message::from_pm(
                        &msg,
                        &mut self.emote_map,
                        MessageOpts { old: false, local },
                    ));
                }
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // TODO make this optional (its only needed for smooth image animations)
        ctx.request_repaint_after(std::time::Duration::from_secs_f32(1.0 / 60.0));

        self.handle_keyboard_input(ctx);

        while let Some(event) = self.twitch.poll(&mut self.state.identity, &mut self.last) {
            self.handle_message(event);
        }

        self.stream_check.poll();
        while let Some(_event) = self.stream_check.poll_event() {
            //
        }

        self.game_map.poll();
        self.user_map.poll();
        self.emote_map.poll();
        self.cache.poll();

        match &mut self.screen {
            Screen::Disconnected => {
                StartView {
                    twitch: &mut self.twitch,
                    screen: &mut self.screen,
                }
                .display(ctx);

                if matches!(self.screen, Screen::Connected { .. }) {
                    self.fetch_initial_emotes();
                }
            }

            Screen::Connected { state } => {
                if matches!(state, ViewState::MainView) && self.state.channels.is_empty() {
                    eprintln!("changing view state: empty");
                    *state = ViewState::Empty {
                        buffer: String::new(),
                    }
                } else if !matches!(state, ViewState::MainView) && !self.state.channels.is_empty() {
                    eprintln!("changing view state: mainview");
                    *state = ViewState::MainView
                };

                match state {
                    ViewState::Empty { buffer } => InitialView {
                        buffer,
                        twitch: &self.twitch,
                    }
                    .display(ctx),
                    ViewState::MainView => MainView { app: self }.display(ctx),
                }
            }
        }
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        SavedState { state: &self.state }.save("vohiyo.toml");
    }

    fn persist_egui_memory(&self) -> bool {
        false
    }
}
