use crate::{image::Image, resolver};

#[derive(Clone)]
pub struct ImageFetcher {
    http: reqwest::Client,
    ctx: egui::Context,
}

impl ImageFetcher {
    pub const fn new(http: reqwest::Client, ctx: egui::Context) -> Self {
        Self { http, ctx }
    }

    pub fn get_image(&self, url: &str) -> resolver::Fut<(String, Option<Image>)> {
        let ctx = self.ctx.clone();
        let client = self.http.clone();
        let url = url.to_string();

        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let Ok(resp) = client.get(&url).send().await else { return };
            let true = resp.status().is_success() else {
            let _ = tx.send((url, None));
            return;
        };

            let Ok(data) = resp.bytes().await.map(|data| data.to_vec()) else { return };

            tokio::task::spawn_blocking(move || {
                let Ok(img) = Image::load_rgba_data(&ctx, &url, &data) else { return };
                let _ = tx.send((url, Some(img)));
                ctx.request_repaint();
            });
        });

        resolver::Fut::new(rx)
    }
}
