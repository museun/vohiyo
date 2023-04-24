use twitch_message::messages::Privmsg;
use uuid::Uuid;

pub struct Connection {
    conn: rusqlite::Connection,
}

impl Connection {
    const SCHEMA: &str = "
            create table if not exists history(
                room_id     text not null,
                channel     text not null,
                user_id     text not null,
                msg_id      blob unique not null,
                timestamp   blob not null,
                data        text not null,
                login       text not null,
                raw         text not null,
                deleted     bool
            );
        ";

    pub fn create(db: &str) -> Self {
        let conn = rusqlite::Connection::open(db).expect("open db");
        let this = Self { conn };
        this.ensure_table();
        this
    }

    fn ensure_table(&self) {
        let Self { conn, .. } = self;
        conn.execute_batch(Self::SCHEMA)
            .expect("ensure table schema is valid");
    }

    pub const fn history(&self) -> History<'_> {
        History::new(self)
    }
}

pub struct History<'a> {
    conn: &'a Connection,
}

impl<'a> History<'a> {
    const fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert<'t>(&self, msg: impl Into<InsertMessage<'t>>) {
        let Connection { conn, .. } = self.conn;

        let mut stmt = conn
            .prepare(
                r#"
                    insert into history(
                        room_id, channel, user_id, msg_id, timestamp, data, login, raw, deleted
                    ) values (
                        :room_id, :channel, :user_id, :msg_id, :timestamp, :data, :login, :raw, :deleted
                    );
                "#,
            )
            .expect("valid sql");

        let msg = msg.into();
        let res = stmt.execute(rusqlite::named_params! {
            ":room_id": msg.room_id,
            ":channel": msg.channel,
            ":user_id": msg.user_id,
            ":msg_id": msg.msg_id,
            ":timestamp": time::OffsetDateTime::now_utc(),
            ":data": msg.data,
            ":login": msg.login,
            ":raw": msg.raw,
            ":deleted": false,
        });

        assert!(matches!(res, Ok(1)), "invalid database state")
    }

    pub fn delete(&self, msg_id: Uuid) -> bool {
        let Connection { conn, .. } = self.conn;

        let mut stmt = conn
            .prepare("update history set deleted = true where msg_id = :msg_id")
            .expect("valid sql");

        1 == stmt
            .execute(rusqlite::named_params! {":msg_id": msg_id})
            .expect("valid query")
    }

    pub fn get_by_msg_id(&self, msg_id: Uuid) -> Option<Message> {
        let Connection { conn, .. } = self.conn;

        let mut stmt = conn
            .prepare("select * from history where msg_id = :msg_id;")
            .expect("valid sqlite");

        stmt.query_row(
            rusqlite::named_params! {":msg_id": msg_id},
            Self::message_from_row,
        )
        .ok()
    }

    pub fn get_messages_for_user(
        &self,
        room_id: &str,
        user_id: &str,
        limit: usize,
    ) -> Vec<Message> {
        self.get_many(
            &format!(
                "select * from(
                        select rowid, * from history
                        where room_id = :room_id and user_id = :user_id
                        order by rowid desc
                        limit {limit}
                    ) order by rowid asc;"
            ),
            rusqlite::named_params! {":room_id": room_id, ":user_id": user_id},
            Self::message_from_row,
        )
    }

    pub fn get_room_id_messages(&self, room_id: &str, limit: usize) -> Vec<Message> {
        self.get_many(
            &format!(
                "select * from(
                        select rowid, * from history
                        where room_id = :room_id
                        order by rowid desc
                        limit {limit}
                    ) order by rowid asc;"
            ),
            rusqlite::named_params! {":room_id": room_id},
            Self::message_from_row,
        )
    }

    pub fn get_channel_messages(&self, channel: &str, limit: usize) -> Vec<Message> {
        let channel = channel.strip_prefix('#').unwrap_or(channel);

        self.get_many(
            &format!(
                "select * from(
                        select rowid, * from history
                        where channel = :channel
                        order by rowid desc
                        limit {limit}
                    ) order by rowid asc;"
            ),
            rusqlite::named_params! {":channel": channel},
            Self::message_from_row,
        )
    }

    fn get_many<T>(
        &self,
        sql: &str,
        params: impl rusqlite::Params,
        map: impl Fn(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    ) -> Vec<T> {
        let Connection { conn, .. } = self.conn;
        let mut stmt = conn.prepare(sql).expect("valid sql");
        let resp = stmt.query_map(params, map);

        let Ok(iter) = resp else { return vec![] };
        iter.flatten().collect()
    }

    fn message_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Message> {
        Ok(Message {
            timestamp: row.get("timestamp")?,
            channel: row.get("channel")?,
            msg_id: row.get("msg_id")?,
            user_id: row.get("user_id")?,
            room_id: row.get("room_id")?,
            login: row.get("login")?,
            data: row.get("data")?,
            raw: row.get("raw")?,
            deleted: row.get("deleted")?,
        })
    }
}

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
