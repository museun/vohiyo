#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct EmoteSet {
    pub format: Vec<String>,
    pub id: String,
    pub name: String,
    pub scale: Vec<String>,
    pub theme_mode: Vec<String>,

    #[serde(default)]
    pub emote_set_id: String,
    #[serde(default)]
    pub emote_type: String,
    #[serde(default)]
    pub owner_id: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Badge {
    pub set_id: String,
    pub versions: Vec<BadgeVersion>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BadgeVersion {
    pub id: String,
    pub description: String,
    pub image_url_1x: String,
    pub image_url_2x: String,
    pub image_url_4x: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Stream {
    pub game_name: String,
    pub game_id: String,
    pub id: String,
    #[serde(with = "time::serde::iso8601::option", default)]
    pub started_at: Option<time::OffsetDateTime>,
    pub title: String,
    #[serde(rename = "type")]
    pub stream_type: Option<String>, // TODO enum
    pub user_id: String,
    pub user_login: String,
    pub viewer_count: i64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct User {
    pub display_name: String,
    pub id: String,
    pub login: String,
    pub description: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
    pub profile_image_url: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Game {
    pub box_art_url: String,
    pub id: String,
    pub igdb_id: String,
    pub name: String,
}
