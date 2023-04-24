use twitch_message::messages::Privmsg;
use uuid::Uuid;

pub struct InsertMessage<'a> {
    pub msg_id: Uuid,
    pub channel: &'a str,
    pub user_id: &'a str,
    pub room_id: &'a str,
    pub login: &'a str,
    pub data: &'a str,
    pub raw: &'a str,
}

impl<'a> From<&'a Privmsg<'static>> for InsertMessage<'a> {
    fn from(value: &'a Privmsg<'static>) -> Self {
        Self {
            msg_id: value
                .msg_id()
                .map(|id| Uuid::parse_str(id.as_str()))
                .transpose()
                .ok()
                .flatten()
                .expect("msg-id"),
            channel: value.channel.strip_prefix('#').unwrap_or(&*value.channel),
            user_id: value
                .user_id()
                .map(<twitch_message::messages::UserIdRef>::as_str)
                .expect("user-id"),
            room_id: value.room_id().expect("room-id"),
            login: value.sender.as_str(),
            data: &*value.data,
            raw: &*value.raw,
        }
    }
}
