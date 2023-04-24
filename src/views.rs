use std::{
    borrow::Cow,
    time::{Duration, Instant},
};

use egui::{
    pos2, vec2, Align2, Area, Button, CentralPanel, Color32, Frame, Grid, Key, Label, Layout,
    Margin, Rect, RichText, Rounding, ScrollArea, Sense, Spinner, TextEdit, TextStyle,
    TopBottomPanel, Vec2,
};
use hashbrown::HashMap;
use twitch_message::{
    builders::{PrivmsgBuilder, TagsBuilder},
    messages::Privmsg,
    Tags,
};

use crate::{
    app::App,
    image::Image,
    input::Input,
    runtime::{EmoteMap, ImageCache},
    state::{MessageOpts, Screen, Span, ViewState},
    twitch,
    widgets::Progress,
};

pub struct InitialView<'a> {
    pub buffer: &'a mut String,
    pub twitch: &'a twitch::Client,
}

impl<'a> InitialView<'a> {
    pub fn display(self, ctx: &egui::Context) {
        Area::new(egui::Id::new("initial-join"))
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let resp = ui.text_edit_singleline(self.buffer);
                    if resp.lost_focus()
                        || ui.input(|i| i.key_pressed(egui::Key::Enter))
                        || ui.button("Join").clicked()
                    {
                        let buf = std::mem::take(self.buffer);
                        let buf = buf.trim();
                        if !buf.is_empty() {
                            self.twitch.writer().join(buf);
                        }
                    }
                    resp.request_focus();
                });
            });

        // fill in the window
        CentralPanel::default().show(ctx, |_ui| {});
    }
}

pub struct MainView<'a> {
    pub app: &'a mut App,
}

impl<'a> MainView<'a> {
    const INACTIVE_GAMMA: f32 = 0.6;

    pub fn display(self, ctx: &egui::Context) {
        Self::display_tab_bar(ctx, self.app);
        Self::display_topic_bar(ctx, self.app);

        let channel = &self.app.state.channels[self.app.state.active];

        // TODO vertical and horizontal splits
        // TODO refactor this

        CentralPanel::default().show(ctx, |ui| {
            let fid = TextStyle::Body.resolve(ui.style());
            let (w, h) = ui.fonts(|f| (f.glyph_width(&fid, ' '), f.row_height(&fid)));

            ScrollArea::vertical()
                .drag_to_scroll(false)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    let dt = ui.input(|i| i.stable_dt.min(0.1));
                    let marker = channel.marker;

                    for msg in channel.messages.iter() {
                        ui.horizontal_wrapped(|ui| {
                            ui.scope(|ui| {
                                ui.spacing_mut().item_spacing.x = 1.0;
                                // TODO fix this alignment
                                ui.with_layout(Layout::left_to_right(egui::Align::Center), |ui| {
                                    if let Some(twitch_message::Badge { name, version }) =
                                        msg.badges.first()
                                    {
                                        if let Some(url) = self
                                            .app
                                            .emote_map
                                            .get_badge_url(name.as_str(), version.as_str())
                                        {
                                            if let Some(image) = self.app.cache.get_image(url) {
                                                let mut image =
                                                    image.as_egui_image(Vec2::splat(h * 0.6), dt);
                                                if msg.opts.old {
                                                    image = image.tint(
                                                        Color32::WHITE
                                                            .gamma_multiply(Self::INACTIVE_GAMMA),
                                                    )
                                                }

                                                ui.add(image).on_hover_text(name.as_str());
                                            }
                                        }
                                    }

                                    ui.add(Label::new(RichText::new(&msg.sender).color(
                                        if msg.opts.old {
                                            msg.color.gamma_multiply(Self::INACTIVE_GAMMA)
                                        } else {
                                            msg.color
                                        },
                                    )));
                                });
                            });

                            ui.scope(|ui| {
                                ui.spacing_mut().item_spacing.x = w;

                                Self::display_fragments(
                                    ui,
                                    Vec2::splat(h),
                                    dt,
                                    msg,
                                    &mut self.app.emote_map,
                                    &mut self.app.cache,
                                )
                            });
                        });

                        if let Some(marker) = marker {
                            if Some(marker) == msg.id {
                                let rect = ui.available_rect_before_wrap();
                                let mut rect = rect.shrink2(vec2(2.0, h));
                                rect.set_height(1.0);
                                let (rect, response) =
                                    ui.allocate_exact_size(rect.size(), Sense::hover());

                                ui.add(|ui: &mut egui::Ui| {
                                    ui.painter().rect_filled(
                                        rect,
                                        Rounding::none(),
                                        Color32::RED.gamma_multiply(Self::INACTIVE_GAMMA),
                                    );

                                    response
                                });
                            }
                        }
                    }

                    ui.allocate_space(ui.available_size_before_wrap());
                });
        });
    }

    fn display_tab_bar(ctx: &egui::Context, app: &mut App) {
        let style = ctx.style();

        let fid = TextStyle::Body.resolve(&style);
        let height = ctx.fonts(|f| f.row_height(&fid));

        // TODO redo this
        // TODO why is the edit box here?

        TopBottomPanel::bottom("tab_bar")
            .height_range(height * 2.0..=f32::INFINITY)
            .show_separator_line(true)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    let size = vec2(ui.available_size().x, height);

                    let is_empty = app.state.channels.is_empty();

                    let resp = ui.add(|ui: &mut egui::Ui| {
                        let default = "";
                        let (mut a, b);
                        ui.add_sized(size, {
                            let buf: &mut dyn egui::TextBuffer = if is_empty {
                                a = default;
                                &mut a as _
                            } else {
                                b = &mut app.state.channels[app.state.active].buffer;
                                b as _
                            };

                            TextEdit::singleline(buf)
                                // TODO this should use the buffer name
                                .id(egui::Id::new("input_buffer").with(app.state.active))
                                .font(egui::TextStyle::Body)
                                .frame(false)
                                .margin(vec2(0.0, 1.0))
                        })
                    });

                    'ret: {
                        if ui.input(|i| i.key_released(Key::Enter)) {
                            let buf =
                                std::mem::take(&mut app.state.channels[app.state.active].buffer);

                            let buf = buf.trim();
                            if buf.is_empty() {
                                break 'ret;
                            }

                            match Input::parse(buf) {
                                Input::Join { channel } => {
                                    app.twitch.writer().join(channel);
                                }
                                Input::Part { channel } => {
                                    app.twitch.writer().part(channel);
                                    // TODO leave the channel
                                    // TODO shift the buffer over
                                    // TODO change the 'active'
                                }
                                Input::Send { data } => {
                                    let (msg, tags) = Self::create_self_message(app, data);
                                    let pm = msg
                                        .clone()
                                        .tags(tags.clone().finish())
                                        .finish_privmsg()
                                        .expect("valid privmsg");

                                    let send = crate::state::Message::from_pm(
                                        &pm,
                                        &mut app.emote_map,
                                        MessageOpts {
                                            old: false,
                                            local: true,
                                        },
                                    );
                                    app.state.channels[app.state.active].push(send);

                                    app.last.replace((msg, tags));

                                    app.twitch
                                        .writer()
                                        .privmsg(&app.state.channels[app.state.active].name, data)
                                }
                                _ => {}
                            }
                        }
                    }

                    resp.request_focus();

                    ui.painter().line_segment(
                        [resp.rect.left_bottom(), resp.rect.right_bottom()],
                        (0.5, Color32::WHITE),
                    );

                    // if let Some(img) = app.cache.get_image(&user.profile_image_url) {
                    //     let resp = ui.add(img.as_egui_image(Vec2::splat(ui.available_height()), 0.0));
                    //     if let Some(desc) = user.description.as_ref().filter(|c| !c.trim().is_empty()) {
                    //         resp.on_hover_ui(|ui| {
                    //             ui.label(&*desc);
                    //         });
                    //     }
                    // }

                    // TODO a close button on the button
                    // TODO channel icon

                    ui.horizontal_wrapped(|ui| {
                        ui.scope(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::splat(2.0);

                            for (i, channel) in app.state.channels.iter().enumerate() {
                                let active = i == app.state.active;

                                let button = Button::new(&channel.name).small().fill(if active {
                                    ui.visuals().widgets.active.bg_fill
                                } else {
                                    ui.visuals()
                                        .widgets
                                        .active
                                        .weak_bg_fill
                                        .linear_multiply(0.2)
                                });

                                let resp = ui.add(button);

                                if active {
                                    ui.painter().rect_stroke(
                                        resp.rect,
                                        ui.visuals().widgets.active.rounding,
                                        (0.5, Color32::BLUE),
                                    )
                                }

                                if resp.clicked() {
                                    app.state.active = i;
                                }
                            }
                        });
                    });
                });
            });
    }

    fn display_topic_bar(ctx: &egui::Context, app: &mut App) {
        let channel = &app.state.channels[app.state.active];

        let Some(user) = app.user_map.get(&channel.name) else { return };
        let Some(stream) = app.stream_check.get_or_subscribe(&user.id) else { return };

        TopBottomPanel::top(egui::Id::new(&user.id).with("topic-bar")).show(ctx, |ui| {
            // views [img] topic
            ui.horizontal(|ui| {
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());

                ui.painter().circle(
                    rect.center(),
                    rect.width() * 0.5,
                    Color32::RED,
                    (1.5, Color32::BLACK),
                );

                if let Some(started_at) = stream.started_at {
                    resp.on_hover_ui(|ui| {
                        fn format_duration(d: time::Duration) -> String {
                            let s = d.whole_seconds();
                            let (h, m, s) = (s / (60 * 60), (s / 60) % 60, s % 60);
                            if h > 0 {
                                format!("{h:02}:{m:02}:{s:02}")
                            } else {
                                format!("{m:02}:{s:02}")
                            }
                        }

                        let now = time::OffsetDateTime::now_utc();
                        let dt = now - started_at;

                        Grid::new(egui::Id::new(&user.id).with("live-grid"))
                            .striped(true)
                            .num_columns(2)
                            .show(ui, |ui| {
                                ui.label("viewers:");
                                ui.monospace(stream.viewer_count.to_string());
                                ui.end_row();

                                ui.label("uptime:");
                                ui.monospace(format_duration(dt));
                                ui.end_row();
                            });
                    });
                }

                if let Some(game) = app.game_map.get(&stream.game_id) {
                    if let Some(image) = app.cache.get_image(&game.box_art_url) {
                        ui.add(image.as_egui_image(Vec2::splat(ui.available_height()), 0.0))
                            .on_hover_text(&game.name);
                    }
                }

                ui.add(Label::new(&stream.title).wrap(true));
            });
        });
    }

    fn display_fragments(
        ui: &mut egui::Ui,
        image_size: Vec2,
        dt: f32,
        msg: &crate::state::Message,
        emote_map: &mut EmoteMap,
        cache: &mut ImageCache,
    ) {
        ui.scope(|ui| {
            if msg.opts.local {
                ui.visuals_mut().override_text_color = Some(Color32::WHITE);
            }

            for span in &msg.spans {
                match span {
                    Span::Text(text) => {
                        ui.label(text);
                    }

                    Span::Emote((id, name)) => {
                        if let Some(url) = emote_map.get_emote_url(id) {
                            if let Some(image) = cache.get_image(url) {
                                let mut image = image.as_egui_image(image_size, dt);
                                if msg.opts.old {
                                    image = image
                                        .tint(Color32::WHITE.gamma_multiply(Self::INACTIVE_GAMMA));
                                }

                                ui.add(image).on_hover_text(name);
                                continue;
                            }
                        }
                        ui.label(name);
                    }

                    Span::Url(url) => {
                        ui.hyperlink(url);
                    }
                }
            }
        });
    }

    fn create_self_message(app: &mut App, data: &str) -> (PrivmsgBuilder, TagsBuilder) {
        let channel = &app.state.channels[app.state.active].name;
        let identity = app.state.identity.as_ref().expect("we should be connected");

        let mut tags = Tags::builder() //
            .add("color", identity.color.unwrap_or_default().to_string())
            .add("user-id", &identity.user_id)
            .add(
                "room-id",
                &app.user_map.get(channel).expect("on the channel").id,
            );

        if let Some(emotes) = Self::build_emotes(app, data) {
            tags = tags.add("emotes", emotes);
        }

        if let Some((set_id, id)) = app
            .state
            .identity
            .as_ref()
            .and_then(|i| i.get_badges_for(channel).next())
        {
            tags = tags.add("badges", format!("{set_id}/{id}"))
        }

        let pm = Privmsg::builder()
            .sender(&identity.name)
            .channel(channel)
            .data(data);
        (pm, tags)
    }

    fn build_emotes(app: &App, data: &str) -> Option<String> {
        let mut emotes = HashMap::<&str, Vec<(usize, usize)>>::new();

        let mut start = 0;
        let len = data.chars().count();
        for (i, ch) in data.char_indices() {
            if i == len - 1 {
                if let Some(id) = app.emote_map.get_emote_id(&data[start..]) {
                    emotes.entry(id).or_default().push((start, i))
                }
                break;
            }
            if !ch.is_ascii_whitespace() {
                continue;
            }

            if let Some(id) = app.emote_map.get_emote_id(&data[start..i]) {
                emotes.entry(id).or_default().push((start, i))
            }

            start = i + 1;
        }

        let emotes = emotes.into_iter().fold(String::new(), |mut a, (id, list)| {
            if !a.is_empty() {
                a.push('/');
            }
            a.push_str(id);
            a.push(':');
            for (i, (start, end)) in list.into_iter().enumerate() {
                if i != 0 {
                    a.push(',');
                }
                a.push_str(&format!("{start}-{end}"))
            }
            a
        });

        (!emotes.is_empty()).then_some(emotes)
    }
}

pub struct StartScreen<'a> {
    pub twitch: &'a mut twitch::Client,
    pub screen: &'a mut Screen,
}

impl<'a> StartScreen<'a> {
    fn load_vohiyo(ctx: &egui::Context) -> &'static egui::TextureHandle {
        static VOHIYO_HANDLE: once_cell::sync::OnceCell<egui::TextureHandle> =
            once_cell::sync::OnceCell::new();

        static IMAGE_DATA: &[u8] =
            include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/vohiyo.png"));

        VOHIYO_HANDLE.get_or_init(|| {
            let Image::Static(handle) = Image::load_rgba_data(ctx, "vohiyo.png", IMAGE_DATA).unwrap() else {
                unreachable!()
            };
            handle
        })
    }

    pub fn display(self, ctx: &egui::Context) {
        match self.twitch.status() {
            status @ (twitch::Status::NotConnected | twitch::Status::Connecting) => {
                self.display_start(ctx, matches!(status, twitch::Status::Connecting));
            }
            twitch::Status::Connected => {
                *self.screen = Screen::Connected {
                    state: ViewState::MainView,
                };
            }
            twitch::Status::Reconnecting { when, after } => {
                self.display_reconnecting(ctx, when, after);
            }
        }
    }

    fn display_start(self, ctx: &egui::Context, connecting: bool) {
        let handle = Self::load_vohiyo(ctx);

        let img_size = handle.size_vec2();
        let size = ctx.screen_rect().size() * 0.2;
        let center = ctx.screen_rect().center() - pos2(0.0, size.y * 0.5);

        let image_frame = |ui: &mut egui::Ui| {
            Frame::none()
                .inner_margin(Margin::symmetric(10.0, 0.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let fid = TextStyle::Body.resolve(ui.style());
                        let w = ui.fonts(|fonts| fonts.glyph_width(&fid, ' '));
                        ui.scope(|ui| {
                            ui.spacing_mut().item_spacing.x = w;
                            ui.colored_label(Color32::from_rgb(0x64, 0x41, 0xA5), "Twitch");
                            ui.label("name:");
                            ui.monospace(self.twitch.user_name())
                        });
                    });
                });
        };

        Area::new("start-inlay")
            .anchor(Align2::RIGHT_TOP, Vec2::ZERO)
            .movable(false)
            .show(ctx, image_frame);

        if connecting {
            Area::new("connecting-inlay")
                .anchor(Align2::CENTER_CENTER, vec2(10.0, 0.0))
                .movable(false)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Connecting...");
                        ui.add(Spinner::new().size(ui.text_style_height(&TextStyle::Body)));
                    });
                });

            // fill in the window
            CentralPanel::default().show(ctx, |_ui| {});
            return;
        }

        CentralPanel::default().show(ctx, |ui| {
            let rect = Rect::from_center_size(center.to_pos2(), size);
            let resp = ui
                .put(rect, |ui: &mut egui::Ui| {
                    let widget = egui::Image::new(handle, img_size);
                    ui.add(widget)
                })
                .interact(Sense::click())
                .on_hover_text("Click to connect to Twitch");

            if resp.clicked() {
                self.twitch.connect()
            }
        });
    }

    fn display_reconnecting(self, ctx: &egui::Context, when: Instant, after: Duration) {
        static LABEL: &str = "waiting to reconnect";

        let fid = TextStyle::Monospace.resolve(&ctx.style());
        let width = ctx.fonts(|f| LABEL.chars().fold(0.0, |a, c| a + f.glyph_width(&fid, c)));

        Area::new("reconnect-screen")
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .interactable(true)
            .show(ctx, |ui| {
                let max_rect = ui.available_rect_before_wrap();
                let max_size = (max_rect.size() * 0.5).max(vec2(width * 1.5, 0.0));

                Frame::central_panel(ui.style())
                    .outer_margin(Margin::symmetric(max_size.x * 0.5, 0.0))
                    .show(ui, |ui| {
                        let diff = after.as_secs_f32() - when.elapsed().as_secs_f32();
                        Progress {
                            pos: egui::emath::inverse_lerp(0.0..=after.as_secs_f32(), diff)
                                .unwrap(),
                            text: LABEL,
                            texture_id: Self::load_vohiyo(ui.ctx()).into(),
                        }
                        .display(ui)
                        .on_hover_ui_at_pointer(|ui: &mut egui::Ui| {
                            let label = match diff.ceil() as u16 {
                                ..=1 => Cow::from("less than 1 second remains"),
                                d => Cow::from(format!("{d} seconds remaining")),
                            };
                            ui.monospace(&*label);
                        });
                    });
            });

        // fill in the window
        CentralPanel::default().show(ctx, |_ui| {});
    }
}
