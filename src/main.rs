#![cfg_attr(debug_assertions, allow(dead_code, unused_variables,))]

mod app;
use app::App;

mod state;

mod input;

mod views;

mod widgets;

mod queue;

mod twitch;

mod helix;

mod util;
pub use util::{select2, Either, Either::*};

mod resolver;

mod runtime;

mod image;

mod repaint;
use repaint::{ErasedRepaint, Repaint};

mod db;

#[tokio::main]
async fn main() {
    simple_env_load::load_env_from([".dev.env", ".secrets.env"]);
    let config = twitch::Config {
        name: std::env::var("TWITCH_NAME").expect("'TWITCH_NAME' must be set'"),
        token: std::env::var("TWITCH_OAUTH").expect("'TWITCH_OAUTH' must be set'"),
    };

    eframe::run_native(
        &format!("VoHiYo - {name}", name = config.name,),
        eframe::NativeOptions::default(),
        Box::new(|cc| App::create(cc, config)),
    )
    .unwrap();
}
