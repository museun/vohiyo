use eframe::emath::Rot2;
use egui::{
    pos2, vec2, Align2, Color32, Mesh, NumExt, Rect, Rounding, Sense, Stroke, TextStyle, TextureId,
    Vec2,
};

pub struct Progress<'a> {
    pub pos: f32,
    pub text: &'a str,
    pub texture_id: TextureId,
}

impl<'a> Progress<'a> {
    pub fn display(self, ui: &mut egui::Ui) -> egui::Response {
        let fid = TextStyle::Monospace.resolve(ui.style());
        let row_height = ui.fonts(|f| f.row_height(&fid));

        let w = ui.available_size_before_wrap().x.at_least(96.0);
        let h = (ui.spacing().interact_size.y * 0.6).max(row_height);
        let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
        if !ui.is_rect_visible(rect) {
            return resp;
        }

        let (bg, fill, text_color) = {
            let v = ui.visuals();
            (
                v.extreme_bg_color,
                v.selection.bg_fill,
                v.strong_text_color(),
            )
        };

        ui.painter()
            .rect(rect, Rounding::same(5.0), bg, Stroke::NONE);

        let diff = self.pos / 1.0;

        let fill_rect = Rect::from_min_size(rect.min, vec2(rect.width() * diff, rect.height()));
        ui.painter()
            .rect(fill_rect, Rounding::same(5.0), fill, Stroke::NONE);

        let w = ui.fonts(|f| {
            self.text
                .chars()
                .fold(0.0, |a, c| a + f.glyph_width(&fid, c))
        });

        ui.painter().text(
            pos2(
                (rect.width() - w).mul_add(0.5, rect.left_top().x) + 2.0,
                row_height.mul_add(0.5, rect.left_top().y),
            ),
            Align2::LEFT_CENTER,
            self.text,
            fid,
            text_color,
        );

        let rect = Rect::from_center_size(fill_rect.right_center(), Vec2::splat(row_height));
        let center = rect.center();

        let mut mesh = Mesh::with_texture(self.texture_id);
        mesh.add_rect_with_uv(
            rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        mesh.rotate(Rot2::from_angle(std::f32::consts::TAU * 6.0 * diff), center);

        ui.painter().add(mesh);

        resp
    }
}
