use anyhow::Context;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Tokio1Executor,
    message::{MessageBuilder, header::ContentType},
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

    info!("Updating A_MAIL_QUEUE origin...");
    if let Err(e) = repository::update_queue_origin().await {
        error!("Failed to update queue origin: {:#}", e);
        // Depending on requirements, we can either exit here or continue.
        // Assuming we should stop if initialization fails.
        std::process::exit(1);
    }

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

        for mail in mails {
            info!(
                code = mail.code,
                to = mail.to,
                subject = mail.subject,
                sent = mail.sent,
                "Sending mail."
            );

            match send_mail(&mailer, &mail).await {
                Ok(()) => {
                    info!("Mail sent successfully: {}", mail.code);
                    // NOTE: at-least-once delivery — if SMTP succeeds but `update_status` fails
                    // (e.g. transient Oracle error), the mail stays in the queue (AMQ_SNDCNT = 0)
                    // and will be resent on the next interval tick. This is acceptable for this
                    // service's requirements; idempotency at the recipient level is not guaranteed.
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

/// Runs `-t` mode: tests SMTP connectivity and Oracle session, then exits.
/// Passwords are never printed. Output goes directly to stdout (no logging).
async fn test_config(target: Option<String>) {
    let cfg = config::get();

    // --- credentials (informational — SMTP password can't be verified without sending) ---
    println!("credentials: {} / [hidden]", cfg.credentials.user);

    // --- SMTP server connectivity (TCP + EHLO handshake) ---
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
            let email = MessageBuilder::new()
                .from(cfg.credentials.user.parse().context("Error parsing 'from' (using credentials user).")?)
                .to(email_addr.parse().context("Error parsing 'to'.")?)
                .subject("Kroma Test Email")
                .header(ContentType::TEXT_PLAIN)
                .body(String::from("This is a test email from Kroma."))
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

    // --- Oracle session ---
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

async fn send_mail(mailer: &AsyncSmtpTransport<Tokio1Executor>, mail: &Mail) -> anyhow::Result<()> {
    let mut email = MessageBuilder::new()
        .from(mail.from.parse().context("Error parsing 'from'.")?)
        .subject(&mail.subject)
        .header(ContentType::TEXT_HTML);

    for recipient in mail.to.split(';') {
        let recipient = recipient.trim();
        if !recipient.is_empty() {
            email = email.to(recipient.parse().with_context(|| format!("Error parsing 'to' address: {}", recipient))?);
        }
    }

    let email = email
        .body(mail.html.clone())
        .context("Error building email.")?;

    mailer.send(email)
        .await
        .context("Error sending email.")?;

    Ok(())
}
