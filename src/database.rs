use std::path::Path;

use chrono::{DateTime, Utc};
use derive_more::{Display, From};
use rusqlite::OptionalExtension;
use serde::Serialize;

#[derive(Debug, From, Display)]
pub enum Error {
    Sql(rusqlite::Error),
    #[display(fmt = "unknown database version: {}", _0)]
    UnknownVersion(u32),
}

type Result<T = ()> = std::result::Result<T, Error>;

#[derive(Debug, Serialize)]
pub struct Feed {
    pub url: String,
    pub link: String,
    pub title: String,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Item {
    pub feed_url: String,
    pub guid: String,
    pub title: String,
    pub link: String,
    pub comments_link: Option<String>,
    pub pub_date: DateTime<Utc>,
    pub is_read: bool,
}

pub struct Database {
    connection: rusqlite::Connection,
}
impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        let connection = rusqlite::Connection::open(path)?;
        let mut database = Self { connection };
        database.run_migrations()?;
        Ok(database)
    }

    fn run_migrations(&mut self) -> Result<()> {
        let user_version: u32 = self.connection.query_row_and_then(
            "PRAGMA user_version",
            rusqlite::NO_PARAMS,
            |row| row.get(0),
        )?;
        match user_version {
            0 => {
                self.connection
                    .execute_batch(include_str!("../resources/create_db.sql"))?;
                Ok(())
            }
            1 => Ok(()),
            version => Err(Error::UnknownVersion(version)),
        }
    }

    pub fn insert_update_feed(&mut self, feed: &Feed) -> Result<()> {
        self.connection.execute(
            "REPLACE INTO feed ( \
             url, \
             link, \
             title, \
             etag, \
             last_modified \
             ) VALUES (?, ?, ?, ?, ?)",
            rusqlite::params![
                feed.url,
                feed.link,
                feed.title,
                feed.etag,
                feed.last_modified
            ],
        )?;
        Ok(())
    }

    pub fn get_feed_by_url(&mut self, url: &str) -> Result<Option<Feed>> {
        Ok(self
            .connection
            .query_row(
                "SELECT \
                 link, \
                 title, \
                 etag, \
                 last_modified \
                 FROM feed WHERE url = ?",
                rusqlite::params![url],
                |row| {
                    Ok(Feed {
                        url: url.to_string(),
                        link: row.get(0)?,
                        title: row.get(1)?,
                        etag: row.get(2)?,
                        last_modified: row.get(3)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn insert_update_item(&mut self, item: &Item) -> Result<()> {
        // is_read is not set if the item already exists.
        self.connection.execute(
            "INSERT INTO item ( \
             feed_url, \
             guid, \
             link, \
             comments_link, \
             title, \
             pub_date, \
             is_read \
             ) VALUES (?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT (feed_url, guid) DO UPDATE SET \
             link = excluded.link, \
             title = excluded.title, \
             pub_date = excluded.pub_date",
            rusqlite::params![
                item.feed_url,
                item.guid,
                item.link,
                item.comments_link,
                item.title,
                item.pub_date,
                item.is_read,
            ],
        )?;
        Ok(())
    }

    pub fn get_unread_items(&mut self, feed_url: &str) -> Result<Vec<Item>> {
        self.connection
            .prepare(
                "SELECT \
                 feed_url, \
                 guid, \
                 link, \
                 comments_link, \
                 title, \
                 pub_date, \
                 is_read \
                 FROM item WHERE \
                 feed_url = ? AND \
                 is_read = 0 \
                 ORDER BY pub_date asc",
            )?
            .query_map(rusqlite::params![feed_url], |row| {
                Ok(Item {
                    feed_url: row.get(0)?,
                    guid: row.get(1)?,
                    link: row.get(2)?,
                    comments_link: row.get(3)?,
                    title: row.get(4)?,
                    pub_date: row.get(5)?,
                    is_read: row.get(6)?,
                })
            })?
            .map(|item| item.map_err(Error::from))
            .collect()
    }

    pub fn mark_all_items_read(&mut self) -> Result<()> {
        // TODO: Avoid marking items as read if they're not currently in the config?
        self.connection
            .execute("UPDATE item SET is_read = 1", rusqlite::params![])?;
        Ok(())
    }
}
