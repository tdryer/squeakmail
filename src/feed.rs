use std::io::Read;
use std::slice::Iter;
use std::str::FromStr;

use atom_syndication as atom;
use chrono::{DateTime, FixedOffset, Utc};
use derive_more::{Display, From};

pub struct Item {
    pub guid: String,
    pub title: String,
    pub link: String,
    pub comments_link: Option<String>,
    pub pub_date: DateTime<Utc>,
}
impl From<&rss::Item> for Item {
    fn from(item: &rss::Item) -> Self {
        Self {
            guid: item.guid().map_or("", |guid| guid.value()).to_string(),
            title: item.title().unwrap_or("Untitled").to_string(),
            link: item.link().unwrap_or("https://example.com").to_string(),
            comments_link: item.comments().map(|s| s.to_string()),
            pub_date: item.pub_date().map_or_else(Utc::now, |date_str| {
                DateTime::parse_from_rfc2822(date_str)
                    .unwrap_or_else(|_| Utc::now().with_timezone(&FixedOffset::east(0)))
                    .with_timezone(&Utc)
            }),
        }
    }
}
impl From<&atom::Entry> for Item {
    fn from(entry: &atom::Entry) -> Self {
        Self {
            guid: entry.id().to_string(),
            title: entry.title().to_string(),
            link: entry
                .links()
                .first()
                .map_or("https://example.com", |link| link.href())
                .to_string(),
            comments_link: None,
            pub_date: entry
                .published()
                .map_or_else(Utc::now, |dt| dt.with_timezone(&Utc)),
        }
    }
}

#[derive(Debug, Display, From)]
pub enum Error {
    Io(std::io::Error),
    #[display(fmt = "parse failed")]
    Parse,
}

type Result<T = ()> = std::result::Result<T, Error>;

pub enum Feed {
    Rss(Box<rss::Channel>),
    Atom(Box<atom::Feed>),
}
impl Feed {
    pub fn read_from<B: Read>(mut reader: B) -> Result<Self> {
        let mut body = String::new();
        reader.read_to_string(&mut body)?;
        match rss::Channel::from_str(&body) {
            Ok(channel) => Ok(Self::Rss(Box::new(channel))),
            Err(_) => match atom::Feed::from_str(&body) {
                Ok(feed) => Ok(Self::Atom(Box::new(feed))),
                Err(_) => Err(Error::Parse),
            },
        }
    }
    pub fn title(&self) -> &str {
        match self {
            Self::Rss(channel) => channel.title(),
            Self::Atom(feed) => feed.title(),
        }
    }
    pub fn link(&self) -> &str {
        match self {
            Self::Rss(channel) => channel.link(),
            Self::Atom(feed) => feed.links().first().map_or("Untitled", |link| link.href()),
        }
    }
    pub fn items(&self) -> Items {
        match self {
            Self::Rss(channel) => Items::Rss(channel.items().iter()),
            Self::Atom(feed) => Items::Atom(feed.entries().iter()),
        }
    }
}

pub enum Items<'a> {
    Rss(Iter<'a, rss::Item>),
    Atom(Iter<'a, atom::Entry>),
}
impl Iterator for Items<'_> {
    type Item = Item;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Rss(iter) => iter.next().map(Item::from),
            Self::Atom(iter) => iter.next().map(Item::from),
        }
    }
}
