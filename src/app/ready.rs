use egui::{CentralPanel, Key, RichText};

use twitch_message::builders::{PrivmsgBuilder, TagsBuilder};

use crate::{
    db, helix,
    runtime::{EmoteMap, GameMap, ImageCache, StreamCheck, UserMap},
    state::{Channel, CredentialsKind, MessageOpts, Screen, State, ViewState},
    twitch,
    views::{InitialView, MainView, StartView},
};

use super::Loaded;

pub struct ReadyApp {
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

    helix_error: helix::ClientError,
    loaded: Loaded,
}

impl ReadyApp {
    pub(super) fn create(ctx: &egui::Context, loaded: super::Loaded) -> Self {
        let http = crate::default_http_client();

        let (helix, helix_error) = helix::Client::create(
            ctx.clone(),
            helix::Config {
                client_id: loaded.client_id.clone(),
                client_secret: loaded.client_secret.clone(),
            },
        );

        let mut state = State {
            channels: loaded
                .channels
                .clone()
                .into_iter()
                .map(|s| Channel::new(&s))
                .collect(),
            active: loaded.active,
            identity: None,
        };

        let mut emote_map = EmoteMap::create(helix.clone(), ctx.clone(), http.clone());

        let conn = db::Connection::create("history.db");
        for channel in &mut state.channels {
            let messages = conn.history().get_channel_messages(&channel.name, 250);
            if let Some(msg) = messages.last() {
                channel.mark_end_of_history(msg.msg_id);
            }
            channel.messages.populate(messages, &mut emote_map);
        }

        let twitch = twitch::Client::create(
            twitch::Config {
                user_name: loaded.user_name.clone(),
                oauth_token: loaded.oauth_token.clone(),
            },
            ctx.clone(),
        );

        let mut user_map = UserMap::create(helix.clone());

        for channel in state.channels.iter().map(|c| &c.name) {
            twitch.writer().join(channel);
            user_map.get(channel);
        }

        Self {
            screen: Screen::default(),
            stream_check: StreamCheck::create(helix.clone(), ctx.clone()),
            cache: ImageCache::new(http, ctx.clone()),
            emote_map,
            game_map: GameMap::create(helix.clone()),
            user_map,

            state,
            twitch,
            helix,
            loaded,

            last: None,

            helix_error,
            conn,
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

            twitch::Message::InvalidCredentials => {
                self.screen = Screen::InvalidCredentials {
                    kind: crate::state::CredentialsKind::Twitch,
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

impl super::VohiyoApp for ReadyApp {
    type Target = super::Transition;

    fn update(
        &mut self,
        ctx: &egui::Context,
        _frame: &mut eframe::Frame,
        target: &mut Self::Target,
    ) {
        // TODO make this optional (its only needed for smooth image animations)
        ctx.request_repaint_after(std::time::Duration::from_secs_f32(1.0 / 60.0));

        self.handle_keyboard_input(ctx);

        while let Some(event) = self.twitch.poll(&mut self.state.identity, &mut self.last) {
            self.handle_message(event);
        }

        while let Some(event) = self.helix_error.poll() {
            match event {
                helix::Event::InvalidCredentials => {
                    self.screen = Screen::InvalidCredentials {
                        kind: crate::state::CredentialsKind::Helix,
                    }
                }
            }
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
            Screen::InvalidCredentials { kind } => {
                CentralPanel::default().show(ctx, |ui| {
                    ui.heading(RichText::new("Error").color(ui.visuals().error_fg_color));
                    ui.separator();

                    match kind {
                        CredentialsKind::Twitch => {
                            ui.label("Your user name and OAuth token aren't a valid combination");
                        }
                        CredentialsKind::Helix => {
                            ui.label("Your Twitch Client-ID and Client-Secret aren't in agreement");
                        }
                    }

                    ui.separator();

                    if ui.button("Edit Configuration").clicked() {
                        *target = Self::Target::Configuration {
                            loaded: std::mem::take(&mut self.loaded),
                        }
                    }
                });
            }

            Screen::Disconnected { .. } => {
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

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {}
}
