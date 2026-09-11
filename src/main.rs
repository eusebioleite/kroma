use anyhow::Context;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Tokio1Executor,
    message::{MessageBuilder, header::ContentType, MultiPart, SinglePart, Attachment as LettreAttachment},
    transport::smtp::authentication::Credentials,
};
use tracing::{error, info};

use crate::{models::Mail, repository::update_status};

mod config;
mod database;
mod log;
mod repository;
mod models;

/// Kroma — processes and sends queued emails from Oracle.
#[derive(argh::FromArgs)]
struct Args {
    /// test the configuration (SMTP connectivity + Oracle session) and exit
    #[argh(switch, short = 't')]
    test: bool,

    /// optional email address to send a test email to when using -t
    #[argh(positional)]
    target: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Args = argh::from_env();

    if args.test {
        config::init().context("Failed to load application configuration from file")?;
        test_config(args.target).await;
        std::process::exit(1);
    } else if args.target.is_some() {
        eprintln!("Error: target can only be specified with the -t flag.");
        std::process::exit(1);
    }

    info!("Setting up logger.");
    let _log_guard = log::init();

    info!("Loading configuration.");
    config::init().context("Failed to load application configuration from file")?;

    info!("Loading database.");
    database::init_pool()
        .await
        .context("Failed to build Oracle connection pool")?;

    info!(
        "Starting routine (Interval: {}s, Throttle: {}ms).",
        config::get().service.interval,
        config::get().service.throttle
    );

    let cfg = config::get();
    let credentials = Credentials::new(
        cfg.credentials.user.clone(),
        cfg.credentials.password.clone(),
    );
    let builder = if cfg.server.port == 465 {
        AsyncSmtpTransport::<Tokio1Executor>::relay(&cfg.server.host).context("Failed to create SMTP relay builder")?
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&cfg.server.host).context("Failed to create STARTTLS SMTP relay builder")?
    };
    let mailer: AsyncSmtpTransport<Tokio1Executor> = builder
        .credentials(credentials)
        .port(cfg.server.port)
        .build();


    let mut interval = tokio::time::interval(std::time::Duration::from_secs(
        config::get().service.interval,
    ));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        interval.tick().await;

        let mails = match repository::get_mails().await {
            Ok(mails) => mails,
            Err(e) => {
                error!("Failed to fetch mails: {:#}", e);
                continue;
            }
        };

        if mails.is_empty() {
            continue;
        }

        info!("Found {} mails in the queue.", mails.len());

        info!("Updating A_MAIL_QUEUE origin...");
        if let Err(e) = repository::update_queue_origin().await {
            error!("Failed to update queue origin: {:#}", e);
            std::process::exit(1);
        }

        for mail in mails {
            let attachments = match repository::get_attachments(mail.code).await {
                Ok(atts) => atts,
                Err(e) => {
                    error!("Failed to fetch attachments for {}: {:#}", mail.code, e);
                    continue;
                }
            };

            info!(
                code = mail.code,
                to = mail.to,
                subject = mail.subject,
                sent = mail.sent,
                files = attachments.len(),
                "Sending mail."
            );

            match send_mail(&mailer, &mail, Some(attachments)).await {
                Ok(()) => {
                    info!("Mail sent successfully: {}", mail.code);
                    if let Err(e) = update_status(mail.code, "S", None, &mail.to).await {
                        error!("Failed to update status for {}: {:#}", mail.code, e);
                    }
                }
                Err(e) => {
                    error!("Failed to send mail {}: {:#}", mail.code, e);
                    if let Err(ue) = update_status(mail.code, "N", Some(&e.to_string()), &mail.to).await {
                        error!("Failed to update error status for {}: {:#}", mail.code, ue);
                    }
                }
            }

            if config::get().service.throttle > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(
                    config::get().service.throttle,
                ))
                .await;
            }
        }

        info!("Batch processing finished.");
    }
}

async fn test_config(target: Option<String>) {
    let cfg = config::get();

    println!("email: {}", cfg.credentials.user);

    print!("server:      {}:{}  ", cfg.server.host, cfg.server.port);
    let smtp_result: anyhow::Result<bool> = async {
        let credentials = Credentials::new(
            cfg.credentials.user.clone(),
            cfg.credentials.password.clone(),
        );
        let builder = if cfg.server.port == 465 {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&cfg.server.host)?
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&cfg.server.host)?
        };
        let mailer: AsyncSmtpTransport<Tokio1Executor> = builder
            .credentials(credentials.clone())
            .port(cfg.server.port)
            .build();
            
        if let Some(email_addr) = &target {
            let attachment = LettreAttachment::new(String::from("test.txt"))
                .body(b"This is a test attachment from Kroma.".to_vec(), ContentType::parse("text/plain").unwrap());
            
            let multipart = MultiPart::mixed()
                .singlepart(SinglePart::plain(String::from("This is a test email from Kroma.")))
                .singlepart(attachment);

            let email = MessageBuilder::new()
                .from(cfg.credentials.user.parse().context("Error parsing 'from' (using credentials user).")?)
                .to(email_addr.parse().context("Error parsing 'to'.")?)
                .subject("Kroma Test Email")
                .multipart(multipart)
                .context("Error building test email.")?;
            mailer.send(email).await?;
            Ok(true)
        } else {
            Ok(mailer.test_connection().await?)
        }
    }
    .await;

    match smtp_result {
        Ok(true)  => println!("ok"),
        Ok(false) => println!("FAIL — server refused the connection"),
        Err(e)    => println!("FAIL — {e}"),
    }

    print!(
        "database:    {}@{}:{}/{}  ",
        cfg.database.user, cfg.database.host, cfg.database.port, cfg.database.sid
    );
    let db_result: anyhow::Result<()> = async {
        database::init_pool().await?;
        database::get_pool().get_session().await?;
        Ok(())
    }
    .await;

    match db_result {
        Ok(())  => println!("ok"),
        Err(e)  => println!("FAIL — {e}"),
    }
}

async fn send_mail(mailer: &AsyncSmtpTransport<Tokio1Executor>, mail: &Mail, attachments: Option<Vec<crate::models::Attachment>>) -> anyhow::Result<()> {
    let mut email = MessageBuilder::new()
        .from(mail.from.parse().context("Error parsing 'from'.")?)
        .subject(&mail.subject);

    for recipient in mail.to.split(';') {
        let recipient = recipient.trim();
        if !recipient.is_empty() {
            email = email.to(recipient.parse().with_context(|| format!("Error parsing 'to' address: {}", recipient))?);
        }
    }

    let mut multipart = MultiPart::mixed()
        .singlepart(SinglePart::html(mail.html.clone()));

    if let Some(atts) = attachments {
        for att in atts {
            let mime_str = match att.extension.to_lowercase().as_str() {
                "pdf" => "application/pdf",
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "txt" => "text/plain",
                "html" | "htm" => "text/html",
                "csv" => "text/csv",
                "xml" => "application/xml",
                _ => "application/octet-stream",
            };
            let content_type = ContentType::parse(mime_str).unwrap_or_else(|_| ContentType::parse("application/octet-stream").unwrap());
            
            let file_name = if att.name.to_lowercase().ends_with(&format!(".{}", att.extension.to_lowercase())) {
                att.name.clone()
            } else {
                format!("{}.{}", att.name, att.extension)
            };
            
            let lettre_att = if att.inline == "1" {
                LettreAttachment::new_inline(file_name.clone())
                    .body(att.content, content_type)
            } else {
                LettreAttachment::new(file_name)
                    .body(att.content, content_type)
            };
            
            multipart = multipart.singlepart(lettre_att);
        }
    }

    let email = email
        .multipart(multipart)
        .context("Error building email.")?;

    mailer.send(email)
        .await
        .context("Error sending email.")?;

    Ok(())
}
