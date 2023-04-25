mod db;
mod helix;
mod image;
mod input;
mod queue;
mod repaint;
mod resolver;
mod runtime;
mod state;
mod util;
mod views;
mod widgets;

pub mod twitch;

mod app;
pub use app::App;

pub(crate) fn default_http_client() -> reqwest::Client {
    use reqwest::header::HeaderName;
    reqwest::ClientBuilder::new()
        .default_headers(
            std::iter::once((
                HeaderName::from_static("user-agent"),
                App::USER_AGENT.parse().unwrap(),
            ))
            .collect(),
        )
        .build()
        .expect("valid client configuration")
}
