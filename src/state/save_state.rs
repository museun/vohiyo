use std::path::Path;

use indexmap::IndexSet;

use super::{Channel, State};

pub struct SavedState<'a> {
    pub state: &'a State,
}

impl<'a> SavedState<'a> {
    pub fn save(&self, path: impl AsRef<Path>) {
        #[derive(serde::Serialize)]
        struct Saved<'a> {
            channels: IndexSet<&'a str>,
            active: usize,
        }

        let s = toml::to_string_pretty(&Saved {
            active: self.state.active,
            channels: self.state.channels.iter().map(|s| &*s.name).collect(),
        })
        .expect("valid serialization");

        let _ = std::fs::write(path, s);
    }

    pub fn load(path: impl AsRef<Path>) -> Option<State> {
        let data = std::fs::read_to_string(path).ok()?;
        #[derive(serde::Deserialize)]
        struct Loaded {
            #[serde(default)]
            channels: IndexSet<String>,
            #[serde(default)]
            active: usize,
        }
        toml::from_str::<Loaded>(&data).ok().map(|loaded| State {
            active: loaded.active.min(loaded.channels.len().saturating_sub(1)),
            channels: loaded
                .channels
                .into_iter()
                .map(|ch| Channel::new(&ch))
                .collect(),
            identity: None,
        })
    }
}
