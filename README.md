# SqueakMail

SqueakMail is an RSS/Atom feed reader that sends email digests.

## Build

SqueakMail is written in Rust, so you'll need to install the [Rust toolchain]
first.

To build SqueakMail:

```
$ git clone https://github.com/tdryer/squeakmail.git
$ cd squeakmail
$ cargo build --release
$ target/release/squeakmail --version
SqueakMail 0.1.0
```

[Rust toolchain]: https://rustup.rs/

## Setup

The first time you run SqueakMail, it will create a default config file in
`~/.config/squeakmail/squeakmail.toml`. Use this file to configure the list of
feeds you want to fetch, and the `To` and `From` addresses for emails.

SqueakMail requires a `sendmail` command to send email. If your system isn't
set up to send email, [msmtp] is a simple option.

[msmtp]: https://marlam.de/msmtp/

## Usage

Use the `fetch` subcommand to fetch feeds:

```
$ squeakmail fetch
```

Use the `mail` subcommand to send an email containing all items fetched since
the last email:

```
$ squeakmail mail
```

To run SqueakMail automatically, use a job scheduler like `crontab`. For
example, the following jobs will fetch feeds at 55 minutes past each hour, and
send an email at 7am in the morning:

```
55 * * * * squeakmail fetch
0 7 * * * squeakmail mail
```
