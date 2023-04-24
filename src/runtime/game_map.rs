use crate::{helix, resolver};

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
