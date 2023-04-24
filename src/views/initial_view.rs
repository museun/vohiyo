use egui::{Align2, Area, CentralPanel, Vec2};

use crate::twitch;

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
