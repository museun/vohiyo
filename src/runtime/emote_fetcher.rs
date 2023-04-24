use std::borrow::Cow;

use hashbrown::HashSet;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::repaint::Repaint;

pub struct EmoteFetcher {
    seen: HashSet<Cow<'static, str>>,
    sender: UnboundedSender<String>,
    ready: UnboundedReceiver<(String, String)>,
}

impl EmoteFetcher {
    pub fn create(repaint: impl Repaint, http: reqwest::Client) -> Self {
        let (tx, ready) = unbounded_channel();
        let (sender, mut rx) = unbounded_channel();

        tokio::spawn(async move {
            while let Some(id) = rx.recv().await {
                struct Emote(String);

                impl Emote {
                    fn animated_url(&self) -> String {
                        format!(
                    "https://static-cdn.jtvnw.net/emoticons/v2/{id}/{format}/{theme_mode}/{scale}",
                    id = self.0,
                    format = "animated",
                    theme_mode = "dark",
                    scale = "1.0"
                )
                    }
                    fn static_url(&self) -> String {
                        format!(
                    "https://static-cdn.jtvnw.net/emoticons/v2/{id}/{format}/{theme_mode}/{scale}",
                    id = self.0,
                    format = "static",
                    theme_mode = "dark",
                    scale = "1.0"
                )
                    }

                    async fn try_get(
                        &mut self,
                        url: String,
                        http: &reqwest::Client,
                        tx: &UnboundedSender<(String, String)>,
                    ) -> bool {
                        if let Ok(resp) = http.get(&url).send().await {
                            if let Ok(_resp) = resp.error_for_status() {
                                let _ = tx.send((std::mem::take(&mut self.0), url));
                                return true;
                            }
                        }
                        false
                    }
                }

                let mut emote = Emote(id);
                if emote.try_get(emote.animated_url(), &http, &tx).await {
                    repaint.repaint();
                    continue;
                }

                if emote.try_get(emote.static_url(), &http, &tx).await {
                    repaint.repaint();
                    continue;
                }

                eprintln!("unknown emote: {id}", id = emote.0);
            }
        });

        Self {
            seen: HashSet::new(),
            ready,
            sender,
        }
    }

    pub fn poll(&mut self) -> Option<(String, String)> {
        self.ready.try_recv().ok()
    }

    pub fn lookup(&mut self, id: &str) {
        // TODO entry
        if self.seen.contains(&Cow::from(id)) {
            return;
        }
        self.seen.insert(Cow::from(id.to_string()));
        let _ = self.sender.send(id.to_string());
    }
}
