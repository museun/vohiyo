use std::borrow::Cow;

use eframe::CreationContext;
use egui::{FontData, FontDefinitions};
use indexmap::IndexSet;

// TODO this has to be an enumeration so we can reuse the same window
pub enum App {
    Ready { app: ReadyApp },
    NotReady { app: NotReadyApp },
}

pub(self) enum Transition {
    Ready { loaded: Loaded },
    Configuration { loaded: Loaded },
    Stay,
}

impl App {
    pub const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

    pub fn create(cc: &CreationContext) -> Box<dyn eframe::App> {
        cc.egui_ctx.set_pixels_per_point(1.5);
        Self::load_fonts(&cc.egui_ctx);

        let mut loaded = <Option<Loaded>>::default();
        if let Some(storage) = cc.storage {
            if let Some(s) = storage
                .get_string("saved_state")
                .and_then(|s| serde_json::from_str(&s).ok())
            {
                loaded.replace(s);
            }
        };

        struct MaybeOverride {
            client_id: Option<String>,
            client_secret: Option<String>,
            user_name: Option<String>,
            oauth_token: Option<String>,
        }

        impl MaybeOverride {
            fn load_from_env() -> Self {
                Self {
                    client_id: std::env::var("TWITCH_CLIENT_ID").ok(),
                    client_secret: std::env::var("TWITCH_CLIENT_SECRET").ok(),
                    user_name: std::env::var("TWITCH_NAME").ok(),
                    oauth_token: std::env::var("TWITCH_OAUTH").ok(),
                }
            }
        }

        let maybe = MaybeOverride::load_from_env();

        let partial = [
            &maybe.client_id,
            &maybe.client_secret,
            &maybe.user_name,
            &maybe.oauth_token,
        ]
        .into_iter()
        .any(Option::is_some);

        if partial {
            let that = loaded.take().unwrap_or_default();
            loaded.replace(Loaded {
                user_name: maybe.user_name.unwrap_or_default(),
                oauth_token: maybe.oauth_token.unwrap_or_default(),
                client_id: maybe.client_id.unwrap_or_default(),
                client_secret: maybe.client_secret.unwrap_or_default(),
                ..that
            });
        }

        if let Some(loaded) = &mut loaded {
            if Validator::validate(&*loaded).is_err() {
                return Box::new(Self::NotReady {
                    app: NotReadyApp::create(std::mem::take(loaded)),
                });
            }
        }

        Box::new(match loaded {
            Some(loaded) => Self::Ready {
                app: ReadyApp::create(&cc.egui_ctx, loaded),
            },
            None => Self::NotReady {
                app: NotReadyApp::create(Loaded::default()),
            },
        })
    }

    // TODO find system default fonts (kurbo can, so we can too)
    fn load_fonts(ctx: &egui::Context) {
        let mut fonts = FontDefinitions::default();

        macro_rules! load_font {
            ($($font:expr => $entry:expr),*) => {
                $(
                    fonts.font_data.insert($font.into(), FontData::from_static(
                        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/fonts/", $font, ".ttf")))
                    );
                    fonts.families.entry($entry).or_default().push($font.into());
                )*
                ctx.set_fonts(fonts)
            };
        }

        load_font! {
            "Roboto-Regular"     => egui::FontFamily::Proportional,
            "RobotoMono-Regular" => egui::FontFamily::Monospace,
            "RobotoMono-Bold"    => egui::FontFamily::Name("bold".into())
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let mut transition = Transition::Stay;
        match self {
            Self::Ready { app } => app.update(ctx, frame, &mut transition),
            Self::NotReady { app } => app.update(ctx, frame, &mut transition),
        };

        match transition {
            Transition::Ready { loaded } => {
                *self = Self::Ready {
                    app: ReadyApp::create(ctx, loaded),
                }
            }
            Transition::Configuration { loaded } => {
                *self = Self::NotReady {
                    app: NotReadyApp::create(loaded),
                }
            }
            _ => {}
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        match self {
            Self::Ready { app } => app.save(storage),
            Self::NotReady { app } => app.save(storage),
        }
    }

    fn persist_egui_memory(&self) -> bool {
        false
    }
}

pub mod not_ready;
pub(crate) use not_ready::NotReadyApp;

mod ready;
pub(crate) use ready::ReadyApp;

#[derive(Default, serde::Deserialize)]
pub(self) struct Loaded {
    pub active: usize,
    pub user_name: String,
    pub oauth_token: String,
    pub client_id: String,
    pub client_secret: String,
    pub channels: IndexSet<String>,
}

#[derive(Copy, Clone)]
pub(self) enum ValidationError {
    TokenPrefix,
    EmptyInput,
    InvalidLength { have: usize, require: usize },
}

impl ValidationError {
    // TODO make this draw the UI instead
    pub(self) fn as_error(&self) -> Cow<'static, str> {
        match self {
            Self::TokenPrefix => Cow::from("OAuth token must start with `oauth:`"),
            Self::EmptyInput => Cow::from("the input is empty"),
            Self::InvalidLength { have, require } => Cow::from(format!(
                "invalid length:\nrequirement: {require}\nhave: {have}"
            )),
        }
    }
}

pub(self) struct Validator;

impl Validator {
    pub(self) fn validate(loaded: &Loaded) -> Result<(), ValidationError> {
        let Loaded {
            user_name,
            oauth_token,
            client_id,
            client_secret,
            ..
        } = loaded;
        Self::user_name(user_name)?;
        Self::oauth_token(oauth_token)?;
        Self::client_id(client_id)?;
        Self::client_secret(client_secret)?;
        Ok(())
    }

    pub(self) fn user_name(input: &str) -> Result<(), ValidationError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(ValidationError::EmptyInput);
        }

        // TODO more stuff here
        Ok(())
    }

    pub(self) fn oauth_token(input: &str) -> Result<(), ValidationError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(ValidationError::EmptyInput);
        }

        if !input.starts_with("oauth:") {
            return Err(ValidationError::TokenPrefix);
        }

        Self::validate_length(input, 36)?;
        // TODO more stuff here
        Ok(())
    }

    pub(self) fn client_id(input: &str) -> Result<(), ValidationError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(ValidationError::EmptyInput);
        }

        Self::validate_length(input, 30)?;
        // TODO more stuff here
        Ok(())
    }

    pub(self) fn client_secret(input: &str) -> Result<(), ValidationError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(ValidationError::EmptyInput);
        }

        Self::validate_length(input, 30)?;
        // TODO more stuff here
        Ok(())
    }

    fn validate_length(input: &str, max: usize) -> Result<(), ValidationError> {
        if input.len() == max {
            return Ok(());
        }

        Err(ValidationError::InvalidLength {
            have: input.len(),
            require: max,
        })
    }
}

#[derive(serde::Serialize)]
pub(self) struct Saved<'a> {
    pub active: usize,
    pub user_name: &'a str,
    pub oauth_token: &'a str,
    pub client_id: &'a str,
    pub client_secret: &'a str,
    pub channels: &'a IndexSet<String>,
}

pub(self) trait VohiyoApp {
    type Target;

    fn update(
        &mut self,
        ctx: &egui::Context,
        _frame: &mut eframe::Frame,
        target: &mut Self::Target,
    );

    fn save(&mut self, _storage: &mut dyn eframe::Storage);
}
