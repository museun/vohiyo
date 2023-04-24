use hashbrown::HashMap;

use twitch_message::Color;

#[derive(Clone, Debug)]
pub struct Identity {
    pub name: String,
    pub display_name: Option<String>,
    pub user_id: String,
    pub color: Option<Color>,
    pub emote_sets: Vec<String>,
    pub(in crate::twitch) badge_map: HashMap<String, HashMap<String, String>>,
}

impl Identity {
    pub fn append_badges<'a>(
        &mut self,
        channel: &str,
        badges: impl IntoIterator<Item = twitch_message::Badge<'a>>,
    ) {
        use hashbrown::hash_map::RawEntryMut::*;
        let channel = channel.strip_prefix('#').unwrap_or(channel);

        // TODO keep track of insertions so we can just .first() / .last() to get the best badge
        let map = self.badge_map.entry(channel.to_string()).or_default();
        for (set_id, id) in badges
            .into_iter()
            .map(|twitch_message::Badge { name, version }| (name, version))
        {
            match map.raw_entry_mut().from_key(set_id.as_str()) {
                Vacant(entry) => {
                    entry.insert(set_id.to_string(), id.to_string());
                }
                Occupied(mut entry) => {
                    *entry.get_mut() = id.to_string();
                }
            }
        }
    }

    pub fn get_badges_for(&self, channel: &str) -> impl Iterator<Item = (&str, &str)> {
        let channel = channel.strip_prefix('#').unwrap_or(channel);

        self.badge_map
            .get(channel)
            .into_iter()
            .flat_map(|inner| inner.iter().map(|(k, v)| (k.as_str(), v.as_str())))
    }
}
