#![cfg_attr(debug_assertions, allow(dead_code, unused_variables,))]
use std::{borrow::Cow, time::Duration};

use hashbrown::{HashMap, HashSet};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::{helix, image::Image, repaint::Repaint, resolver, select2, Either};

pub struct GameMap {
    map: resolver::ResolverMap<String, helix::data::Game, Option<helix::data::Game>>,
    helix: helix::Client,
}

impl GameMap {
    pub fn create(helix: helix::Client) -> Self {
        Self {
            map: resolver::ResolverMap::new(),
            helix,
        }
    }

    pub fn get(&mut self, game_id: &str) -> Option<&helix::data::Game> {
        self.map
            .get_or_update(game_id, |game_id| self.helix.get_game(game_id))
    }

    pub fn poll(&mut self) {
        const WIDTH: &str = "144";
        const HEIGHT: &str = "152";

        self.map.poll(|entry, game| {
            if let Some(mut game) = game {
                game.box_art_url = game
                    .box_art_url
                    .replace("{width}", WIDTH)
                    .replace("{height}", HEIGHT);

                entry.set(game.id.clone(), game);
            }
        });
    }
}

pub struct UserMap {
    map: resolver::ResolverMap<String, helix::data::User, Option<(String, helix::data::User)>>,
    helix: helix::Client,
}

impl UserMap {
    pub fn create(helix: helix::Client) -> Self {
        Self {
            map: resolver::ResolverMap::new(),
            helix,
        }
    }

    pub fn get(&mut self, login: &str) -> Option<&helix::data::User> {
        let login = login.strip_prefix('#').unwrap_or(login);
        self.map
            .get_or_update(login, |login| self.helix.get_user(login))
    }

    pub fn poll(&mut self) {
        self.map.poll(|entry, user| {
            if let Some((_name, user)) = user {
                entry.set(user.login.clone(), user);
            }
        });
    }
}

pub enum Action<T> {
    Added(T),
    Removed(T),
}

#[derive(Clone, Debug)]
pub struct StreamStatus {
    pub user_id: String,
}

pub struct StreamCheck {
    map: resolver::ResolverMap<
        String,
        Option<helix::data::Stream>,
        (String, Option<helix::data::Stream>),
    >,

    watching: UnboundedSender<Action<String>>,
    update: UnboundedReceiver<(String, Option<helix::data::Stream>)>,
    send: UnboundedSender<Action<StreamStatus>>,
    events: UnboundedReceiver<Action<StreamStatus>>,
}

impl StreamCheck {
    const STREAM_CHECK_DURATION: Duration = Duration::from_secs(30);
    const BURST_WINDOW: Duration = Duration::from_secs(1);

    pub fn create(helix: helix::Client, repaint: impl Repaint) -> Self {
        let (watching, rx) = unbounded_channel();
        let (resp, update) = unbounded_channel();
        let (send, events) = unbounded_channel();

        tokio::spawn(Self::poll_helix(helix, repaint, rx, resp));

        Self {
            map: resolver::ResolverMap::new(),
            watching,
            update,
            send,
            events,
        }
    }

    pub fn poll(&mut self) {
        while let Ok((id, stream)) = self.update.try_recv() {
            Self::update(&mut self.map.update(), &self.send, id, stream);
        }

        self.map
            .poll(|entry, (id, stream)| Self::update(entry, &self.send, id, stream));

        // TODO maybe poll the event here
    }

    pub fn poll_event(&mut self) -> Option<Action<StreamStatus>> {
        self.events.try_recv().ok()
    }

    pub fn get_or_subscribe(&mut self, user_id: &str) -> Option<&helix::data::Stream> {
        self.map
            .get_or_else(user_id, |user_id| {
                eprintln!("subscribing to events for stream: {user_id}");
                let _ = self.watching.send(Action::Added(user_id.to_string()));
            })?
            .as_ref()
    }

    pub fn unsubscribe(&self, user_id: &str) {
        let _ = self.watching.send(Action::Removed(user_id.to_string()));
    }

    async fn poll_helix(
        helix: helix::Client,
        repaint: impl Repaint,
        mut recv: UnboundedReceiver<Action<String>>,
        send: UnboundedSender<(String, Option<helix::data::Stream>)>,
    ) {
        let mut set = <HashSet<String>>::new();
        let mut queue = vec![];

        macro_rules! batch_send {
            ($set:expr) => {
                let mut delta = <HashSet<&str>>::from_iter($set.map(|s| &**s));
                let Some(streams) = helix.get_many_streams($set).wait().await else { continue };
                for stream in streams {
                    delta.remove(&*stream.user_id);
                    if send.send((stream.user_id.clone(), Some(stream))).is_err() {
                        break;
                    }
                }

                for remaining in delta {
                    if send.send((remaining.to_owned(), None)).is_err() {
                        break;
                    }
                }
            };
        }

        loop {
            let mut sleep = std::pin::pin!(tokio::time::sleep(Self::STREAM_CHECK_DURATION));
            let mut recv = std::pin::pin!(tokio::time::timeout(Self::BURST_WINDOW, recv.recv()));

            match select2(&mut sleep, &mut recv).await {
                Either::Left(_) => {
                    batch_send!(set.iter());
                    if !set.is_empty() {
                        repaint.repaint();
                    }
                }

                Either::Right(Ok(Some(action))) => {
                    let channel = match action {
                        Action::Added(channel) => channel,
                        Action::Removed(channel) => {
                            set.remove(&channel);
                            continue;
                        }
                    };

                    if set.insert(channel.clone()) {
                        queue.push(channel)
                    }
                }

                Either::Right(Err(..)) => {
                    if !queue.is_empty() {
                        batch_send!(queue.iter());
                        queue.clear();
                        repaint.repaint();
                    }
                }

                Either::Right(..) => break,
            }
        }
    }

    fn update(
        entry: &mut resolver::ResolverEntry<String, Option<helix::data::Stream>>,
        sender: &UnboundedSender<Action<StreamStatus>>,
        id: String,
        stream: Option<helix::data::Stream>,
    ) {
        let action = if stream.is_none() {
            Action::Removed
        } else {
            Action::Added
        }(StreamStatus {
            user_id: id.clone(),
        });

        entry.set(id, stream);
        let _ = sender.send(action);
    }
}

pub struct EmoteMap {
    name_to_id: HashMap<String, String>,
    // TODO id_to_name
    emote_map: HashMap<String, String>,
    emote_fetcher: EmoteFetcher,
    emote_set_map: resolver::ResolverMap<String, String, Vec<helix::data::EmoteSet>>,
    badge_map: resolver::ResolverMap<u64, String, (Option<String>, Vec<helix::data::Badge>)>,
    helix: helix::Client,
}

impl EmoteMap {
    pub fn create(
        helix: helix::Client,
        repaint: impl Repaint,
        http_client: reqwest::Client,
    ) -> Self {
        let mut this = Self {
            name_to_id: HashMap::new(),
            emote_map: HashMap::new(),
            emote_fetcher: EmoteFetcher::create(repaint, http_client),
            emote_set_map: resolver::ResolverMap::new(),
            badge_map: resolver::ResolverMap::new(),
            helix,
        };

        this.populate_global_badges();
        this.populate_global_emotes();
        this
    }

    pub fn populate_global_badges(&mut self) {
        self.badge_map
            .add(self.helix.get_global_badges().wrap(|list| (None, list)))
    }

    pub fn populate_channel_badges(&mut self, id: &str) {
        self.badge_map.add(self.helix.get_channel_badges(id).wrap({
            let id = id.to_string();
            |list| (Some(id), list)
        }))
    }

    pub fn populate_global_emotes(&mut self) {
        self.emote_set_map.add(self.helix.get_global_emotes())
    }

    pub fn populate_channel_emotes(&mut self, id: &str) {
        self.emote_set_map.add(self.helix.get_channel_emotes(id))
    }

    pub fn populate_emote_set(&mut self, id: &str) {
        self.emote_set_map.add(self.helix.get_emote_set(id))
    }

    pub fn get_badge_url(&self, set_id: &str, id: &str) -> Option<&str> {
        let hash = Self::hash_badge("-", set_id, id);
        self.badge_map.try_get(&hash).map(<String>::as_str)
    }

    pub fn get_channel_badge_url(&self, user_id: &str, set_id: &str, id: &str) -> Option<&str> {
        let hash = Self::hash_badge(user_id, set_id, id);
        self.badge_map
            .try_get(&hash)
            .or_else(|| {
                let hash = Self::hash_badge("-", set_id, id);
                self.badge_map.try_get(&hash)
            })
            .map(<String>::as_str)
    }

    pub fn insert_emote(&mut self, id: &str, name: &str) {
        if !self.emote_map.contains_key(id) {
            self.emote_fetcher.lookup(id);
        }
        self.name_to_id.insert(name.to_string(), id.to_string());
    }

    pub fn get_emote_id(&self, name: &str) -> Option<&str> {
        self.name_to_id.get(name).map(<String>::as_str)
    }

    pub fn get_emote_url(&self, id: &str) -> Option<&str> {
        self.emote_set_map
            .try_get(id)
            .or_else(|| self.emote_map.get(id))
            .map(<String>::as_str)
    }

    fn hash_badge(user_id: &str, set_id: &str, id: &str) -> u64 {
        use hashbrown::hash_map::DefaultHashBuilder as H;
        use std::hash::{BuildHasher, Hash, Hasher};
        [user_id, set_id, id]
            .into_iter()
            .fold(H::default().build_hasher(), |mut hasher, val| {
                val.hash(&mut hasher);
                hasher
            })
            .finish()
    }

    pub fn poll(&mut self) {
        fn filter<'a>(
            options: &'a [String],
            k: &str,
            or: impl Into<Option<&'static str>>,
        ) -> &'a str {
            let or_else = || {
                or.into()
                    .map_or_else(|| options.last().unwrap(), std::convert::identity)
            };

            options
                .iter()
                .find_map(|t| (t == k).then_some(t.as_str()))
                .unwrap_or_else(or_else)
        }

        fn make_emote_url(set: &crate::helix::data::EmoteSet) -> String {
            format!(
                "https://static-cdn.jtvnw.net/emoticons/v2/{id}/{format}/{theme_mode}/{scale}",
                id = set.id,
                format = filter(&set.format, "animated", "static"),
                theme_mode = filter(&set.theme_mode, "dark", "light"),
                scale = filter(&set.scale, "1.0", None)
            )
        }

        while let Some((id, url)) = self.emote_fetcher.poll() {
            self.emote_map.insert(id, url);
        }

        self.emote_set_map.poll(|entry, list| {
            for set in list {
                let url = make_emote_url(&set);
                entry.set(set.id.clone(), url);
                self.name_to_id.insert(set.name, set.id);
            }
        });

        self.badge_map.poll(|entry, (cid, list)| {
            let cid = cid.map_or_else(|| Cow::from("-"), Cow::from);
            for set in list {
                for version in set.versions {
                    let hash = Self::hash_badge(&cid, &set.set_id, &version.id);
                    let url = version.image_url_1x;
                    entry.set(hash, url)
                }
            }
        });
    }
}

pub struct ImageCache {
    images: resolver::ResolverMap<String, Image, (String, Option<Image>)>,
    fetcher: ImageFetcher,
}

impl ImageCache {
    pub fn new(http: reqwest::Client, ctx: egui::Context) -> Self {
        Self {
            images: resolver::ResolverMap::new(),
            fetcher: ImageFetcher::new(http, ctx),
        }
    }

    pub fn set(&mut self, url: String, image: Image) {
        self.images.update().set(url, image);
    }

    pub fn get_image(&mut self, url: &str) -> Option<&Image> {
        self.images
            .get_or_update(url, |url| self.fetcher.get_image(url))
    }

    pub fn poll(&mut self) {
        self.images.poll(|entry, (k, v)| match v {
            Some(v) => {
                eprintln!("fetched image: {k}");
                entry.set(k, v);
            }
            None => {
                eprintln!("could not fetch image: {k}")
            }
        });
    }
}

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
