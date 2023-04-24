use super::History;

pub struct Connection {
    pub(in crate::db) conn: rusqlite::Connection,
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
