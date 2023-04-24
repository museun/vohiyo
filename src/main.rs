mod db;
mod helix;
mod image;
mod input;
mod queue;
mod repaint;
mod resolver;
mod runtime;
mod state;
mod twitch;
mod util;
mod views;
mod widgets;

mod app;

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
        Box::new(|cc| app::App::create(cc, config)),
    )
    .unwrap();
}
