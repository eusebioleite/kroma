use anyhow::{Context, Result};

use crate::{database, models::{Attachment, Mail}};

pub async fn update_queue_origin() -> Result<()> {
    let session = database::get_pool()
        .get_session()
        .await
        .context("Failed to get session from pool.")?;

    let sql = "UPDATE a_mail_queue SET amq_origem = 'AVL'";

    let stmt = session
        .prepare(sql)
        .await
        .context("Failed to prepare statement for update_queue_origin")?;

    let rows_affected = stmt
        .execute(())
        .await
        .context("Failed to execute update_queue_origin")?;
        
    session.commit().await.context("Failed to commit session after update_queue_origin")?;

    tracing::info!("Updated amq_origem to 'AVL' for {} rows in A_MAIL_QUEUE.", rows_affected);

    Ok(())
}

pub async fn get_mails() -> Result<Vec<Mail>> {
    let session = database::get_pool()
        .get_session()
        .await
        .context("Failed to get session from pool.")?;

    let sql = r#"
        select
            AMQ_SEQUEN as "code",
            AMQ_E_FROM as "from",
            AMQ_EML_TO as "to",
            AMQ_ASSUNT as "subject",
            AMQ_SNDCNT as "sent",
            AMQ_SNDERR as "error",
            AMQ_EMBODY as "text",
            AMQ_BDYHTM as "html"
        from A_MAIL_QUEUE
        where AMQ_SNDCNT < 10
        FETCH FIRST 100 ROWS ONLY
    "#;

    let stmt = session
        .prepare(sql)
        .await
        .context("Failed to prepare get_queue statement for A_MAIL_QUEUE")?;

    let rows = stmt
        .query(())
        .await
        .context("Failed to query queue records from Oracle A_MAIL_QUEUE")?;

    let mut queue = Vec::new();

    while let Some(row) = rows
        .next()
        .await
        .context("Failed to fetch next row from queue query")?
    {
        let mail = Mail::from_row(&row).await.context("Failed to parse row into Mail model")?;
        queue.push(mail);
    }

    Ok(queue)
}

pub async fn get_attachments(queue_code: i32) -> Result<Vec<Attachment>> {
    let session = database::get_pool()
        .get_session()
        .await
        .context("Failed to get session from pool.")?;

    let sql = r#"
        select 
            AMA_NOMARQ as "name",
            REPLACE(AMA_EXTARQ , '.', '') as "entension",
            AMA_ANEXO  as "content",
            NVL(AMA_INLINE, 0) as "inline"
        from A_MAIL_ANEX
        where AMA_CODFIL = :1
    "#;

    let stmt = session
        .prepare(sql)
        .await
        .context("Failed to prepare get_attachments for A_MAIL_ANEX")?;

    let rows = stmt
        .query(queue_code)
        .await
        .context("Failed to query queue records from Oracle A_MAIL_ANEX")?;

    let mut queue = Vec::new();

    while let Some(row) = rows
        .next()
        .await
        .context("Failed to fetch next row from get_attachments query")?
    {
        let attachment = Attachment::from_row(&row).await.context("Failed to parse row into Attachment model")?;
        queue.push(attachment);
    }

    Ok(queue)
}

pub async fn update_status(
    queue_id: i32,
    status: &str,
    error_msg: Option<&str>,
    recipient_email: &str,
) -> Result<()> {
    let session = database::get_pool()
        .get_session()
        .await
        .context("Failed to get session from pool.")?;

    let sql = "
        BEGIN
            CONSOLIDADO.DEBXMAIL.FINAL_ENVIO(
                pCODQUE => :1,
                pSTATUS => :2,
                pMSGERR => :3,
                pEMLENV => :4
            );
        END;
    ";

    let stmt = session
        .prepare(sql)
        .await
        .context("Failed to prepare statement for update_status")?;

    let truncated_error: Option<String>;
    let error_msg = match error_msg {
        Some(msg) if msg.len() > 4000 => {
            tracing::warn!(
                "error_msg for queue_id {} exceeds 4000 chars ({} bytes), truncating.",
                queue_id,
                msg.len()
            );
            let cut = msg
                .char_indices()
                .map(|(i, _)| i)
                .take_while(|&i| i < 4000)
                .last()
                .map(|i| {
                    msg[i..].chars().next().map_or(i, |c| i + c.len_utf8())
                })
                .unwrap_or_else(|| msg.len().min(4000));
            truncated_error = Some(msg[..cut].to_string());

            truncated_error.as_deref()
        }
        other => other,
    };

    stmt.execute((queue_id, status, error_msg, recipient_email))
        .await
        .with_context(|| {
            format!("Failed to execute update_status for queue_id {}", queue_id)
        })?;

    session.commit().await.with_context(|| {
        format!("Failed to commit session after updating status for queue_id {}", queue_id)
    })?;

    Ok(())
}