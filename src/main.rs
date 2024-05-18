use anyhow::Result;
use config_file::FromConfigFile;
use imap;
use native_tls;
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt::Debug;
use std::io::Write;
use zeroize::ZeroizeOnDrop;

#[derive(ZeroizeOnDrop, Default)]
struct Password(String);

impl Debug for Password {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<password>")
    }
}

#[derive(Deserialize)]
struct Config {
    domain: String,
    email: String,
    #[serde(skip)]
    password: Password,
    subject: String,
    fields: Vec<String>,
    separator: Option<String>,
    output_file: String,
}

fn fetch_inbox_top(config: &Config) -> imap::error::Result<Vec<String>> {
    let domain = config.domain.as_str();
    let tls = native_tls::TlsConnector::builder().build()?;

    // we pass in the domain twice to check that the server's TLS
    // certificate is valid for the domain we're connecting to.
    let client = imap::connect((domain, 993), domain, &tls)?;

    // the client we have here is unauthenticated.
    // to do anything useful with the e-mails, we need to log in
    let mut imap_session = client
        .login(&config.email, &config.password.0)
        .map_err(|e| e.0)?;

    // we want to fetch the first email in the INBOX mailbox
    imap_session.select("INBOX")?;
    // fetch message number 1 in this mailbox, along with its RFC822 field.
    // RFC 822 dictates the format of the body of e-mails
    let message_ids = imap_session.search(&format!("SUBJECT \"{}\"", config.subject))?;
    println!("{message_ids:?}");
    let mut bodies = Vec::new();
    for id in message_ids {
        let messages = imap_session.fetch(&format!("{id}"), "RFC822")?;
        for message in &messages {
            if let Some(body) = message.body() {
                let body = String::from_utf8_lossy(body);
                bodies.push(body.to_string());
            }
        }
    }

    // be nice to the server and log outSee the note on unilateral server responses in RFC 3501.
    imap_session.logout()?;

    Ok(bodies)
}

fn main() -> Result<()> {
    let mut config = Config::from_config_file("epar.toml")?;
    print!("Bitte das Passwort für den E-Mail-Server eingeben: ");
    std::io::stdout().flush()?;
    let password = rpassword::read_password()?;
    config.password = Password(password);
    let bodies = fetch_inbox_top(&config)?;

    let mut file = std::fs::File::create(&config.output_file)?;
    let mut is_first = true;
    let mut map = HashMap::new();
    let sep = &config.separator.unwrap_or(",".to_string());
    for field in &config.fields {
        map.insert(field.clone(), String::new());
        if !is_first {
            write!(file, "{sep}")?;
        } else {
            is_first = false;
        }
        write!(file, "\"{}\"", field)?;
    }
    writeln!(file, "")?;

    for b in &bodies {
        is_first = true;
        let lines = b.split('\n');
        map.clear();
        for line in lines {
            for field in &config.fields {
                if line.starts_with(&format!("{field}: ")) {
                    let start = field.len() + 2;
                    let value = line[start..].to_string();
                    map.insert(field.clone(), value.trim().to_owned());
                }
            }
        }
        for field in &config.fields {
            if !is_first {
                write!(file, "{sep}")?;
            } else {
                is_first = false;
            }
            if let Some(value) = map.get(field) {
                write!(file, "\"{value}\"")?;
            }
        }
        writeln!(file, "")?;
    }

    Ok(())
}
