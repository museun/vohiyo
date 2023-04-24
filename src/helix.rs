use std::sync::Arc;

use reqwest::{header::HeaderName, StatusCode};
use tokio::{sync::Mutex, task::JoinSet};

use crate::{resolver::Fut, ErasedRepaint, Repaint};

pub mod data;

pub struct HelixConfig {
    pub client_id: String,
    pub client_secret: String,
}

impl HelixConfig {
    fn load() -> Self {
        fn get(key: &str) -> String {
            std::env::var(key).unwrap_or_else(|_| panic!("'{key}' is not set"))
        }

        Self {
            client_id: get("TWITCH_CLIENT_ID"),
            client_secret: get("TWITCH_CLIENT_SECRET"),
        }
    }
}

pub static HELIX_CONFIG: once_cell::sync::Lazy<HelixConfig> =
    once_cell::sync::Lazy::new(HelixConfig::load);

#[derive(Clone)]
pub struct Client {
    client: reqwest::Client,
    repaint: ErasedRepaint,
    bearer_token: Arc<Mutex<Option<Arc<String>>>>,
}

impl Client {
    pub fn create(repaint: impl Repaint) -> Self {
        let headers = [
            ("user-agent", crate::app::App::USER_AGENT),
            ("client-id", &*HELIX_CONFIG.client_id),
        ]
        .into_iter()
        .map(|(k, v)| {
            (
                HeaderName::from_static(k),
                v.parse().expect("valid header name"),
            )
        })
        .collect();

        let client = reqwest::ClientBuilder::new()
            .default_headers(headers)
            .build()
            .expect("valid client configuration");

        Self {
            client,
            bearer_token: Arc::default(),
            repaint: repaint.erased(),
        }
    }

    pub fn get_global_emotes(&self) -> Fut<Vec<data::EmoteSet>> {
        self.get_response_fut(
            "https://api.twitch.tv/helix/chat/emotes/global",
            (),
            Self::flatten_result_vec,
        )
    }

    pub fn get_emote_set(&self, id: &str) -> Fut<Vec<data::EmoteSet>> {
        self.get_response_fut(
            "https://api.twitch.tv/helix/chat/emotes/set",
            [("emote_set_id", id.to_string())],
            Self::flatten_result_vec,
        )
    }

    pub fn get_channel_emotes(&self, id: &str) -> Fut<Vec<data::EmoteSet>> {
        self.get_response_fut(
            "https://api.twitch.tv/helix/chat/emotes",
            [("broadcaster_id", id.to_string())],
            Self::flatten_result_vec,
        )
    }

    pub fn get_global_badges(&self) -> Fut<Vec<data::Badge>> {
        self.get_response_fut(
            "https://api.twitch.tv/helix/chat/badges/global",
            (),
            Self::flatten_result_vec,
        )
    }

    pub fn get_channel_badges(&self, id: &str) -> Fut<Vec<data::Badge>> {
        self.get_response_fut(
            "https://api.twitch.tv/helix/chat/badges",
            [("broadcaster_id", id.to_string())],
            Self::flatten_result_vec,
        )
    }

    pub fn get_game(&self, id: &str) -> Fut<Option<data::Game>> {
        self.get_response_fut(
            "https://api.twitch.tv/helix/games",
            [("id", id.to_string())],
            Self::result_vec_single,
        )
    }

    pub fn get_user_from_id(&self, id: &str) -> Fut<Option<data::User>> {
        let id = id.to_string();
        self.get_response_fut("https://api.twitch.tv/helix/users", [("id", id)], {
            move |items| {
                let mut items = items.ok().filter(|c| !c.is_empty())?;
                Some(items.remove(0))
            }
        })
    }

    pub fn get_user(&self, login: &str) -> Fut<Option<(String, data::User)>> {
        let login = login.to_string();
        self.get_response_fut(
            "https://api.twitch.tv/helix/users",
            [("login", login.clone())],
            {
                move |items| {
                    let mut items = items.ok().filter(|c| !c.is_empty())?;
                    Some((login.clone(), items.remove(0)))
                }
            },
        )
    }

    pub fn get_many_users<T>(&self, logins: impl IntoIterator<Item = T>) -> Fut<Vec<data::User>>
    where
        T: ToString,
    {
        let logins = logins
            .into_iter()
            .map(|s| ("login", s.to_string()))
            .collect::<Vec<_>>();

        self.get_many_inner("https://api.twitch.tv/helix/users", logins)
    }

    pub fn get_many_streams<T>(&self, ids: impl IntoIterator<Item = T>) -> Fut<Vec<data::Stream>>
    where
        T: ToString,
    {
        let ids = ids
            .into_iter()
            .map(|s| ("user_id", s.to_string()))
            .collect::<Vec<_>>();
        self.get_many_inner("https://api.twitch.tv/helix/streams", ids)
    }

    fn flatten_result_vec<T>(result: anyhow::Result<Vec<T>>) -> Vec<T> {
        Result::unwrap_or_default(result)
    }

    fn result_vec_single<T>(result: anyhow::Result<Vec<T>>) -> Option<T> {
        result.unwrap_or_default().pop()
    }

    fn get_many_inner<T>(
        &self,
        ep: &'static str,
        query: Vec<impl serde::Serialize + Clone + Send + Sync + 'static>,
    ) -> Fut<Vec<T>>
    where
        for<'de> T: serde::Deserialize<'de> + Send + Sync + 'static,
    {
        let this = self.clone();
        let fut = async move {
            let mut set = JoinSet::new();

            for chunk in query.chunks(100) {
                let query = chunk.to_vec();
                let this = this.clone();
                set.spawn(async move {
                    this.get_response::<T>(ep, query)
                        .await
                        .ok()
                        .unwrap_or_default()
                });
            }

            let mut out = Vec::with_capacity(query.len());
            while let Some(item) = set.join_next().await {
                out.extend(item.into_iter().flatten())
            }
            out.shrink_to_fit();
            out
        };

        Fut::spawn(fut)
    }

    fn get_response_fut<T, U>(
        &self,
        ep: &'static str,
        query: impl serde::Serialize + Send + 'static,
        map: impl Fn(anyhow::Result<Vec<T>>) -> U + Send + 'static,
    ) -> Fut<U>
    where
        U: Send + 'static,
        T: for<'de> serde::Deserialize<'de> + Send + 'static,
    {
        let this = self.clone();
        let fut = async move {
            let result = this.get_response(ep, query).await;
            map(result)
        };

        Fut::spawn(fut)
    }

    async fn get_response<T>(
        &self,
        ep: &str,
        query: impl serde::Serialize + Send,
    ) -> anyhow::Result<Vec<T>>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        // TODO exponential backoff (or atleast add some jitter)
        let resp = loop {
            let token = self.fetch_bearer_token().await;
            let req = self
                .client
                .get(ep)
                .header("authorization", &*token)
                .query(&query)
                .build()?;

            let resp = self.client.execute(req).await?;
            if resp.status() != StatusCode::UNAUTHORIZED {
                break resp;
            }

            eprintln!("fetching a new OAuth token");
            let _ = self.bearer_token.lock().await.take();
        };

        #[derive(serde::Deserialize)]
        struct Resp<T> {
            data: Vec<T>,
        }

        let Resp { data } = resp.json().await?;
        (self.repaint)();
        Ok(data)
    }

    async fn fetch_bearer_token(&self) -> Arc<String> {
        let mut token = self.bearer_token.lock().await;
        if let Some(token) = &mut *token {
            return Arc::clone(token);
        }

        let HelixConfig {
            client_id,
            client_secret,
        } = &*HELIX_CONFIG;

        let bearer_token = Self::get_oauth(client_id, client_secret)
            .await
            // TODO make this fallible
            .unwrap_or_else(|err| panic!("cannot update bearer token: {err}"));

        Arc::clone(token.insert(Arc::from(bearer_token)))
    }

    async fn get_oauth(client_id: &str, client_secret: &str) -> anyhow::Result<String> {
        #[derive(serde::Serialize)]
        struct Query<'a> {
            client_id: &'a str,
            client_secret: &'a str,
            grant_type: &'a str,
        }

        #[derive(serde::Deserialize)]
        struct Response {
            access_token: String,
        }

        let Response { access_token } = reqwest::Client::new()
            .post("https://id.twitch.tv/oauth2/token")
            .query(&Query {
                client_id,
                client_secret,
                grant_type: "client_credentials",
            })
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        Ok(format!("Bearer {access_token}"))
    }
}
