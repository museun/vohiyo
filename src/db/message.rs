use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct Message {
    pub timestamp: time::OffsetDateTime,
    pub msg_id: Uuid,
    pub channel: Box<str>,
    pub user_id: Box<str>,
    pub room_id: Box<str>,
    pub login: Box<str>,
    pub data: Box<str>,
    pub raw: Box<str>,
    pub deleted: bool,
}
