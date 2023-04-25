use std::borrow::Cow;

use egui::{
    vec2, Align2, Area, CentralPanel, Color32, CursorIcon, Grid, RichText, Rounding, Sense,
    TextEdit,
};

use super::{ValidationError, Validator};

// TODO note which fields failed the loop
pub struct NotReadyApp {
    loaded: super::Loaded,
}

impl NotReadyApp {
    pub(super) const fn create(loaded: super::Loaded) -> Self {
        Self { loaded }
    }

    fn values(&mut self) -> [Value<'_>; 4] {
        [
            (
                "User Name",
                &mut self.loaded.user_name,
                false,
                Validator::user_name as fn(&str) -> Result<(), ValidationError>,
            ),
            (
                "OAuth Token",
                &mut self.loaded.oauth_token,
                true,
                Validator::oauth_token,
            ),
            (
                "Client Id",
                &mut self.loaded.client_id,
                false,
                Validator::client_id,
            ),
            (
                "Client Secret",
                &mut self.loaded.client_secret,
                true,
                Validator::client_secret,
            ),
        ]
    }
}

type Value<'a> = (
    &'static str,
    &'a mut String,
    bool,
    fn(&str) -> Result<(), ValidationError>,
);

impl super::VohiyoApp for NotReadyApp {
    type Target = super::Transition;

    fn update(
        &mut self,
        ctx: &egui::Context,
        _frame: &mut eframe::Frame,
        target: &mut Self::Target,
    ) {
        // TODO display a message about an empty config
        // TODO display a ui for setting up a default config
        // TODO validate ten switch to the ReadyApp

        CentralPanel::default().show(ctx, |ui| {
            let ok = self
                .values()
                .iter()
                .map(|(_, v, _, f)| f(v).is_ok())
                .all(std::convert::identity);

            let left = Area::new("heading-text")
                .anchor(Align2::LEFT_TOP, vec2(10.0, 3.0))
                .show(ctx, |ui| {
                    if !ok {
                        ui.label(
                            RichText::new("No valid configuration found")
                                .heading()
                                .color(ui.visuals().error_fg_color),
                        )
                    } else {
                        ui.label(
                            RichText::new("Configuration looks good")
                                .heading()
                                .color(Color32::LIGHT_GREEN),
                        )
                    }
                })
                .inner;

            let right = Area::new("submit-button")
                .anchor(Align2::RIGHT_TOP, vec2(-10.0, 3.0))
                .show(ctx, |ui| {
                    ui.add_enabled(ok, |ui: &mut egui::Ui| ui.button("okay"))
                })
                .inner;

            if right.clicked() {
                *target = Self::Target::Ready {
                    loaded: std::mem::take(&mut self.loaded),
                };
                return;
            }

            let mut rect = (left | right).rect;
            let (_, target) = ui.allocate_space(rect.size());

            rect.extend_with_y(rect.bottom() + 4.0);

            ui.painter().line_segment(
                [rect.left_bottom(), rect.right_bottom()],
                ui.visuals().widgets.noninteractive.bg_stroke,
            );

            Area::new("grid-form")
                .anchor(Align2::CENTER_TOP, vec2(0.0, target.bottom() + 10.0))
                .show(ctx, |ui| {
                    let view = |ui: &mut egui::Ui| {
                        let id = egui::Id::new("config-form");

                        for (k, v, secret, validator) in self.values() {
                            #[derive(Copy, Clone)]
                            struct FormState {
                                err: Option<ValidationError>,
                                show: bool,
                            }

                            let id = id.with(k);
                            let mut state = ui.data_mut(|d| {
                                *d.get_temp_mut_or(
                                    id,
                                    FormState {
                                        err: None,
                                        show: secret,
                                    },
                                )
                            });

                            let key = ui
                                .horizontal_centered(|ui| {
                                    ui.monospace(k);
                                })
                                .response;

                            ui.horizontal(|ui| {
                                let resp = ui.add(TextEdit::singleline(v).password(state.show));
                                let left = resp.rect.left();

                                let mut key_rect = key.rect;
                                key_rect.extend_with_x(left - 8.0);
                                let key_rect = key_rect.expand2(vec2(1.0, 2.0));

                                if let Some(err) = state.err {
                                    ui.painter().rect_stroke(
                                        key_rect,
                                        Rounding::none(),
                                        (1.5, ui.visuals().error_fg_color),
                                    );

                                    ui.interact_with_hovered(
                                        key_rect,
                                        ui.rect_contains_pointer(key_rect),
                                        id.with("hovered"),
                                        Sense::hover(),
                                    )
                                    .on_hover_cursor(CursorIcon::Help)
                                    .on_hover_text(&*err.as_error());
                                }

                                if secret {
                                    state.show = !ui.small_button("👁").is_pointer_button_down_on();
                                }

                                if resp.changed() {
                                    if let Err(err) = validator(v) {
                                        state.err.replace(err);
                                    } else {
                                        state.err.take();
                                    }
                                }

                                ui.data_mut(|d| d.insert_temp(id, state));
                            });

                            ui.end_row();
                        }
                    };

                    Grid::new("do-configuration")
                        .num_columns(2)
                        .striped(true)
                        .show(ui, view);
                });
        });
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {}
}
