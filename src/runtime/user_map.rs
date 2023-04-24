use crate::{helix, resolver};

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
