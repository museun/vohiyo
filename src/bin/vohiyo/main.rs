#[tokio::main]
async fn main() {
    simple_env_load::load_env_from([".dev.env", ".secrets.env"]);
    eframe::run_native(
        "VoHiYo",
        eframe::NativeOptions::default(),
        Box::new(vohiyo::App::create),
    )
    .unwrap();
}
