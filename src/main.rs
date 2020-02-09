#![warn(clippy::pedantic)]
#![allow(clippy::redundant_closure_for_method_calls)]

use std::cmp::min;
use std::fs::File;
use std::io::{Read, Write};
use std::num::NonZeroU16;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use attohttpc;
use clap::{crate_version, App, Arg, SubCommand};
use derive_more::{Display, From};
use lettre::sendmail::SendmailTransport;
use lettre::{EmailAddress, SendableEmail, Transport};
use lettre_email::Email;
use serde::{Deserialize, Serialize};
use tera::Tera;

mod database;
mod feed;

// Must have ".html" suffix to force tera to do escaping.
const MAIL_TEMPLATE_NAME: &str = "mail.html";

#[derive(Debug, From, Display)]
enum Error {
    #[display(fmt = "failed to parse config: {}", _0)]
    ParseConfig(toml::de::Error),
    #[display(fmt = "failed to read config: {}", _0)]
    ReadConfig(std::io::Error),
    #[display(fmt = "feed not modified")]
    FeedNotModified,
    #[display(fmt = "unexpected status code: {}", _0)]
    UnexpectedStatusCode(u16),
    Http(attohttpc::Error),
    Parse(feed::Error),
    #[display(fmt = "database error: {}", _0)]
    Database(database::Error),
    #[from(ignore)]
    #[display(fmt = "failed to create config directory: {}", _0)]
    CreateConfigDir(std::io::Error),
    #[from(ignore)]
    #[display(fmt = "failed to create config file: {}", _0)]
    CreateConfigFile(std::io::Error),
    #[from(ignore)]
    #[display(fmt = "failed to create database directory: {}", _0)]
    CreateDatabaseDir(std::io::Error),
    #[display(fmt = "sendmail error: {}", _0)]
    Sendmail(lettre::sendmail::error::Error),
}

type Result<T = ()> = std::result::Result<T, Error>;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    feeds: Vec<String>,
    // TODO: EmailAddress should validate itself when deserializing.
    from_email: EmailAddress,
    to_email: EmailAddress,
    concurrency: NonZeroU16,
}
impl Config {
    fn from_path(path: &Path) -> Result<Self> {
        let mut config_file = File::open(path)?;
        let mut config_str = String::new();
        config_file.read_to_string(&mut config_str)?;
        Ok(toml::from_str(&config_str)?)
    }
}
impl std::default::Default for Config {
    fn default() -> Self {
        Self {
            feeds: vec!["https://blog.rust-lang.org/feed.xml".to_string()],
            from_email: EmailAddress::new("squeakmail@example.com".to_string())
                .expect("invalid default"),
            to_email: EmailAddress::new("squeakmail@example.com".to_string())
                .expect("invalid default"),
            concurrency: NonZeroU16::new(1).expect("invalid default"),
        }
    }
}

#[derive(Debug, Serialize)]
struct FeedWithItems {
    feed: database::Feed,
    items: Vec<database::Item>,
}

#[derive(Debug, Serialize)]
struct MailContext {
    subject: String,
    feeds: Vec<FeedWithItems>,
}

/// Create parent directory of path, if it doesn't exist.
fn create_parent_dir(path: &Path) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    if !parent.is_dir() && parent != Path::new("") {
        std::fs::create_dir(parent)
    } else {
        Ok(())
    }
}

/// Create example config file at path if one does not exist.
fn create_example_config_file(path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        let mut file = std::fs::File::create(path)?;
        file.write_all(
            toml::to_string_pretty(&Config::default())
                .expect("default config not serializable")
                .as_bytes(),
        )?;
    }
    Ok(())
}

fn main() {
    std::process::exit(match run() {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("error: {}", e);
            1
        }
    });
}

struct Args {
    config: PathBuf,
    database: PathBuf,
    command: Command,
}

enum Command {
    Fetch,
    Mail { dry: bool },
}

fn get_args() -> Args {
    let proj_dirs = directories::ProjectDirs::from("com", "tomdryer", "squeakmail");
    let default_config_path = proj_dirs
        .as_ref()
        .map_or_else(PathBuf::new, |proj_dirs| {
            proj_dirs.config_dir().to_path_buf()
        })
        .join("squeakmail.toml");
    let default_database_path = proj_dirs
        .as_ref()
        .map_or_else(PathBuf::new, |proj_dirs| {
            proj_dirs.cache_dir().to_path_buf()
        })
        .join("squeakmail.db");
    let matches = App::new("SqueakMail")
        .version(crate_version!())
        .arg(
            Arg::with_name("config")
                .long("config")
                .default_value_os(default_config_path.as_os_str()),
        )
        .arg(
            Arg::with_name("database")
                .long("database")
                .default_value_os(default_database_path.as_os_str()),
        )
        .subcommand(SubCommand::with_name("fetch").about("Fetches feeds"))
        .subcommand(
            SubCommand::with_name("mail").about("Mails feeds").arg(
                Arg::with_name("dry")
                    .long("dry")
                    .help("Print email body instead of sending it"),
            ),
        )
        .get_matches();
    Args {
        config: PathBuf::from(matches.value_of_os("config").expect("impossible none")),
        database: PathBuf::from(matches.value_of_os("database").expect("impossible none")),
        command: match matches.subcommand() {
            ("fetch", Some(_)) => Command::Fetch,
            ("mail", Some(sub_matches)) => Command::Mail {
                dry: sub_matches.is_present("dry"),
            },
            _ => panic!("impossible subcommand"),
        },
    }
}

fn run() -> Result<()> {
    let args = get_args();

    create_parent_dir(&args.config).map_err(Error::CreateConfigDir)?;
    create_example_config_file(&args.config).map_err(Error::CreateConfigFile)?;
    let config = Config::from_path(&args.config)?;

    create_parent_dir(&args.database).map_err(Error::CreateDatabaseDir)?;
    let mut database = database::Database::open(&args.database)?;

    match args.command {
        Command::Fetch => {
            fetch_feeds(config, database);
        }
        Command::Mail { dry } => {
            let mail = render_mail(&config, &mut database)?;
            if dry {
                println!(
                    "{}",
                    mail.message_to_string()
                        .expect("message cannot be converted to string")
                );
            } else {
                eprintln!("Sending mail...");
                SendmailTransport::new().send(mail)?;
                database.mark_all_items_read()?;
            }
        }
    };
    Ok(())
}

fn fetch_feeds(config: Config, database: database::Database) {
    let num_threads = min(config.concurrency.get() as usize, config.feeds.len());
    let database = Arc::new(Mutex::new(database));
    let queue = Arc::new(Mutex::new(config.feeds));
    let mut handles = vec![];
    for _ in 0..num_threads {
        let queue = queue.clone();
        let database = database.clone();
        handles.push(thread::spawn(move || {
            // Clippy fails to account for lifetime of MutexGuard
            #[allow(clippy::while_let_loop)]
            loop {
                let feed_url = match queue
                    .lock()
                    .expect("thread panicked while holding queue mutex")
                    .pop()
                {
                    Some(feed_url) => feed_url,
                    None => break,
                };
                match fetch_feed(&feed_url, &database) {
                    Ok(()) => {}
                    Err(e) => eprintln!("Failed to fetch feed: {}", e),
                };
            }
        }));
    }
    for handle in handles {
        handle.join().expect("thread panicked");
    }
}

fn fetch_feed(feed_url: &str, database: &Mutex<database::Database>) -> Result<()> {
    let feed = database
        .lock()
        .expect("thread panicked while holding database mutex")
        .get_feed_by_url(feed_url)?;
    eprintln!("Fetching {}...", feed_url);
    let mut builder = attohttpc::get(feed_url)
        .header(attohttpc::header::USER_AGENT, env!("CARGO_PKG_NAME"))
        .timeout(Duration::from_secs(30));
    if let Some(feed) = feed {
        if let Some(etag) = feed.etag {
            builder = builder.header(attohttpc::header::IF_NONE_MATCH, etag);
        }
        if let Some(last_modified) = feed.last_modified {
            builder = builder.header(attohttpc::header::IF_MODIFIED_SINCE, last_modified);
        }
    }
    let resp = builder.send()?;
    if resp.status() == attohttpc::StatusCode::NOT_MODIFIED {
        return Err(Error::FeedNotModified);
    } else if !resp.status().is_success() {
        return Err(Error::UnexpectedStatusCode(resp.status().as_u16()));
    }
    let etag = resp
        .headers()
        .get(attohttpc::header::ETAG)
        .and_then(|header_value| header_value.to_str().ok())
        .map(|header_str| header_str.to_string());
    let last_modified = resp
        .headers()
        .get(attohttpc::header::LAST_MODIFIED)
        .and_then(|header_value| header_value.to_str().ok())
        .map(|header_str| header_str.to_string());
    let feed = feed::Feed::read_from(resp.text_reader())?;

    database
        .lock()
        .expect("thread panicked while holding database mutex")
        .insert_update_feed(&database::Feed {
            url: feed_url.to_string(),
            link: feed.link().to_string(),
            title: feed.title().to_string(),
            etag,
            last_modified,
        })?;
    for item in feed.items() {
        database
            .lock()
            .expect("thread panicked while hold database mutex")
            .insert_update_item(&database::Item {
                feed_url: feed_url.to_string(),
                guid: item.guid,
                title: item.title,
                link: item.link,
                comments_link: item.comments_link,
                pub_date: item.pub_date,
                is_read: false,
            })?;
    }
    Ok(())
}

fn render_mail(config: &Config, database: &mut database::Database) -> Result<SendableEmail> {
    let subject = format!("SqueakMail for {}", chrono::Local::now().format("%c"));
    let mut feeds_with_items = Vec::new();
    for feed_url in &config.feeds {
        // skips feed that don't exist in database
        if let Some(feed) = database.get_feed_by_url(feed_url)? {
            feeds_with_items.push(FeedWithItems {
                feed,
                items: database.get_unread_items(feed_url)?,
            })
        }
    }
    let context = MailContext {
        subject: subject.to_string(),
        feeds: feeds_with_items,
    };
    let mut tera = Tera::default();
    tera.add_raw_template(MAIL_TEMPLATE_NAME, include_str!("../resources/mail.html"))
        .expect("invalid mail template");
    let context = tera::Context::from_serialize(context).expect("failed to build tera context");
    let html_content = tera
        .render(MAIL_TEMPLATE_NAME, &context)
        .expect("failed to render mail from template");
    Ok(Email::builder()
        // TODO: Convert directly from EmailAddress to Mailbox in next version of lettre.
        .to(config.to_email.to_string())
        .from(config.from_email.to_string())
        .subject(subject)
        .html(html_content)
        .build()
        .expect("failed to build email")
        .into())
}
