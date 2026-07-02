use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Writes one LSP message with `Content-Length` framing — the wire format
/// every LSP server/client speaks over stdio, independent of which server.
pub async fn write_message(
    writer: &mut (impl AsyncWrite + Unpin),
    value: &serde_json::Value,
) -> std::io::Result<()> {
    let body = serde_json::to_vec(value)?;
    writer.write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes()).await?;
    writer.write_all(&body).await?;
    writer.flush().await
}

/// Reads one `Content-Length`-framed LSP message, skipping any other
/// headers (e.g. `Content-Type`) until the blank line, per the LSP spec.
pub async fn read_message(reader: &mut (impl AsyncBufRead + Unpin)) -> std::io::Result<serde_json::Value> {
    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "language server closed the stream"));
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some(value) = trimmed.strip_prefix("Content-Length: ") {
            content_length = Some(
                value
                    .parse()
                    .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "bad Content-Length header"))?,
            );
        }
    }
    let len = content_length
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "missing Content-Length header"))?;
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    serde_json::from_slice(&buf).map_err(std::io::Error::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::BufReader;

    #[tokio::test]
    async fn round_trips_a_message_over_an_in_memory_duplex() {
        let (mut client, server) = tokio::io::duplex(4096);
        let mut server_reader = BufReader::new(server);

        let payload = serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"});
        write_message(&mut client, &payload).await.unwrap();

        let read_back = read_message(&mut server_reader).await.unwrap();
        assert_eq!(read_back, payload);
    }

    #[tokio::test]
    async fn read_message_errors_cleanly_on_closed_stream() {
        let (client, server) = tokio::io::duplex(64);
        drop(client);
        let mut server_reader = BufReader::new(server);
        let result = read_message(&mut server_reader).await;
        assert!(result.is_err());
    }
}
