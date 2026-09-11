use anyhow::{Context, Result};
use sibyl::Row;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Mail {
    pub code: i32,
    pub from: String,
    pub to: String,
    pub subject: String,
    pub sent: i32,
    pub error: Option<String>,
    pub text: String,
    pub html: String,
}

impl Mail {
    pub async fn from_row(row: &Row<'_>) -> Result<Self> {
        let code: i32 = row
            .get(0)
            .context("Failed to read 'code' (column 0) from row")?;

        let from: String = row
            .get(1)
            .context("Failed to read 'from' (column 1) from row")?;

        let to: String = row
            .get(2)
            .context("Failed to read 'to' (column 2) from row")?;
        
        let subject: String = row
            .get(3)
            .context("Failed to read 'subject' (column 3) from row")?;
        
        let sent: i32 = row
            .get(4)
            .context("Failed to read 'sent' (column 4) from row")?;
        
        let error: Option<String> = row
            .get(5)
            .context("Failed to read 'error' (column 5) from row")?;
        
        let text_clob: Option<sibyl::CLOB<'_>> = row
            .get(6)
            .context("Failed to read 'text' (column 6) from row")?;
        
        let text = match text_clob {
            Some(c) => {
                let len = c.len().await.context("Failed to get length of text CLOB")?;
                let mut s = String::with_capacity(len);
                c.read(0, len, &mut s).await.context("Failed to read text CLOB")?;
                s
            }
            None => String::new(),
        };
        
        let html_clob: Option<sibyl::CLOB<'_>> = row
            .get(7)
            .context("Failed to read 'html' (column 7) from row")?;

        let html = match html_clob {
            Some(c) => {
                let len = c.len().await.context("Failed to get length of html CLOB")?;
                let mut s = String::with_capacity(len);
                c.read(0, len, &mut s).await.context("Failed to read html CLOB")?;
                s
            }
            None => String::new(),
        };
        
        Ok(Self {
            code,
            from,
            to,
            subject,
            sent,
            error,
            text,
            html,
        })
    }
}

pub struct Attachment {
    pub name: String,
    pub extension: String,
    pub content: Vec<u8>,
    pub inline: String,
}

impl Attachment {
    pub async fn from_row(row: &Row<'_>) -> anyhow::Result<Self> {
        let name: String = row
            .get(0)
            .context("Failed to read 'name' (column 0) from row")?;

        let extension: String = row
            .get(1)
            .context("Failed to read 'extension' (column 1) from row")?;
        
        let blob: sibyl::BLOB<'_> = row
            .get(2)
            .context("Failed to get Blob descriptor from column 2")?;

        let len = blob.len().await.context("Failed to get BLOB length")?;
        let mut content = Vec::with_capacity(len);
        
        blob.read(0, len, &mut content)
            .await
            .context("Failed to read BLOB payload")?;

        let inline: String = row
            .get(3)
            .context("Failed to read 'inline' (column 3) from row")?;

        Ok(Self {
            name,
            extension,
            content,
            inline,
        })
    }
}
