use std::borrow::Cow;

use hashbrown::HashMap;

use crate::{helix, repaint::Repaint, resolver};

use super::EmoteFetcher;

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
