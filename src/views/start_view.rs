use std::{
    borrow::Cow,
    time::{Duration, Instant},
};

use egui::{
    pos2, vec2, Align2, Area, CentralPanel, Color32, CursorIcon, Frame, Margin, Order, Rect, Sense,
    Spinner, TextStyle, Vec2,
};

use crate::{
    image::Image,
    state::{Screen, ViewState},
    twitch,
    widgets::Progress,
};

pub struct StartView<'a> {
    pub twitch: &'a mut twitch::Client,
    pub screen: &'a mut Screen,
}

impl<'a> StartView<'a> {
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

            twitch::Status::Reconnecting { when, after } => {
                self.display_reconnecting(ctx, when, after);
            }

            twitch::Status::Connected => {
                *self.screen = Screen::Connected {
                    state: ViewState::MainView,
                };
            }
        }
    }

    fn display_start(self, ctx: &egui::Context, connecting: bool) {
        let start_inlay = |ui: &mut egui::Ui| {
            ui.horizontal(|ui| {
                let fid = TextStyle::Body.resolve(ui.style());
                let w = ui.fonts(|fonts| fonts.glyph_width(&fid, ' '));
                ui.scope(|ui| {
                    ui.spacing_mut().item_spacing.x = w;
                    ui.colored_label(Color32::from_rgb(0x64, 0x41, 0xA5), "Twitch");
                    ui.label("name:");

                    let resp = ui.monospace(self.twitch.user_name());
                    let resp = ui.allocate_rect(resp.rect, Sense::click());

                    ui.painter().line_segment(
                        [resp.rect.left_bottom(), resp.rect.right_bottom()],
                        (1.0, Color32::WHITE.gamma_multiply(0.6)),
                    );

                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(CursorIcon::Help);
                    }

                    if resp.clicked() {}
                });
            });
        };

        Area::new("start-inlay")
            .anchor(Align2::RIGHT_TOP, vec2(-10.0, 0.0))
            .interactable(true)
            .order(Order::Foreground)
            .show(ctx, start_inlay);

        // if let Some(properties) = properties {
        //     Area::new("connection-properties")
        //         .anchor(Align2::LEFT_TOP, resp.rect.left_bottom().to_vec2())
        //         .show(ctx, |ui| {
        //             Frame::none()
        //                 .inner_margin(Margin::symmetric(10.0, 10.0))
        //                 .stroke(Stroke::new(1.0, Color32::RED))
        //                 .fill(ui.visuals().extreme_bg_color)
        //                 .show(ui, |ui| ui.label("testing"))
        //         });
        // }

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
            let handle = Self::load_vohiyo(ctx);

            let size = ctx.screen_rect().size() * 0.2;
            let center = ctx.screen_rect().center() - pos2(0.0, size.y * 0.5);

            let rect = Rect::from_center_size(center.to_pos2(), size);
            let resp = ui
                .put(rect, egui::Image::new(handle, handle.size_vec2()))
                .interact(Sense::click())
                .on_hover_text("Click to connect to Twitch");

            if resp.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::Help)
            }

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
