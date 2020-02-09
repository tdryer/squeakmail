PRAGMA user_version = 1;

PRAGMA foreign_keys = ON;

CREATE TABLE feed (
    url TEXT CHECK(TYPEOF(url) = 'text'),
    link TEXT CHECK(TYPEOF(link) = 'text'),
    title TEXT CHECK(TYPEOF(title) = 'text'),
    etag TEXT CHECK(TYPEOF(etag) = 'text' OR TYPEOF(etag) = 'null'),
    last_modified TEXT CHECK(TYPEOF(last_modified) = 'text' OR TYPEOF(last_modified) = 'null'),
    PRIMARY KEY (url)
);

CREATE TABLE item (
    feed_url TEXT CHECK(TYPEOF(feed_url) = 'text'),
    guid TEXT CHECK(TYPEOF(guid) = 'text'),
    link TEXT CHECK(TYPEOF(link) = 'text'),
    comments_link TEXT CHECK(TYPEOF(comments_link) = 'text' OR TYPEOF(comments_link) = 'null'),
    title TEXT CHECK(TYPEOF(title) = 'text'),
    pub_date DATETIME CHECK(DATETIME(pub_date) IS NOT NULL),
    is_read BOOLEAN CHECK(is_read = 0 OR is_read = 1),
    PRIMARY KEY (feed_url, guid),
    FOREIGN KEY (feed_url) REFERENCES feed(url)
)
