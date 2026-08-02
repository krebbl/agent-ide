use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

pub struct SpawnedServer {
    pub child: Child,
    pub stdin: ChildStdin,
    pub stdout: BufReader<ChildStdout>,
}

pub fn spawn_local(
    program: &std::path::Path,
    args: &[&str],
    cwd: Option<&str>,
) -> Result<SpawnedServer, String> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn {}: {}", program.display(), e))?;
    let stdin = child
        .stdin
        .take()
        .ok_or("Failed to open language server stdin")?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Failed to open language server stdout")?;
    Ok(SpawnedServer {
        child,
        stdin,
        stdout: BufReader::new(stdout),
    })
}

pub async fn read_message<R>(reader: &mut BufReader<R>) -> Result<Option<Value>, String>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut content_length: Option<usize> = None;
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader
            .read_line(&mut line)
            .await
            .map_err(|e| format!("LSP read error: {}", e))?;
        if n == 0 {
            return Ok(None);
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            if content_length.is_some() {
                break;
            }
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            content_length = value.trim().parse::<usize>().ok();
        }
    }
    let len = content_length.ok_or("Missing Content-Length header")?;
    let mut buf = vec![0u8; len];
    reader
        .read_exact(&mut buf)
        .await
        .map_err(|e| format!("LSP body read error: {}", e))?;
    let value = serde_json::from_slice(&buf)
        .map_err(|e| format!("Invalid JSON from language server: {}", e))?;
    Ok(Some(value))
}

pub async fn write_message<W>(writer: &mut W, value: &Value) -> Result<(), String>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    let body = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    writer
        .write_all(header.as_bytes())
        .await
        .map_err(|e| format!("LSP write error: {}", e))?;
    writer
        .write_all(&body)
        .await
        .map_err(|e| format!("LSP write error: {}", e))?;
    writer
        .flush()
        .await
        .map_err(|e| format!("LSP flush error: {}", e))?;
    Ok(())
}

pub fn frame_bytes(value: &Value) -> Result<Vec<u8>, String> {
    let body = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    let mut out = header.into_bytes();
    out.extend_from_slice(&body);
    Ok(out)
}

/// Incremental JSON-RPC frame parser for byte-stream transports (SSH channels).
#[derive(Default)]
pub struct FrameParser {
    buf: Vec<u8>,
}

impl FrameParser {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    pub fn feed(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    pub fn next_message(&mut self) -> Result<Option<Value>, String> {
        let header_end = self.buf.windows(4).position(|w| w == b"\r\n\r\n");
        let Some(pos) = header_end else {
            if self.buf.len() > 1024 * 1024 {
                return Err("LSP header exceeded 1MB without terminator".to_string());
            }
            return Ok(None);
        };
        let header = std::str::from_utf8(&self.buf[..pos])
            .map_err(|_| "Invalid LSP header encoding".to_string())?;
        let mut content_length: Option<usize> = None;
        for line in header.split("\r\n") {
            if let Some(value) = line.strip_prefix("Content-Length:") {
                content_length = value.trim().parse::<usize>().ok();
            }
        }
        let len = content_length.ok_or("Missing Content-Length header")?;
        let body_start = pos + 4;
        if self.buf.len() < body_start + len {
            return Ok(None);
        }
        let body: Vec<u8> = self.buf[body_start..body_start + len].to_vec();
        self.buf.drain(..body_start + len);
        let value = serde_json::from_slice(&body)
            .map_err(|e| format!("Invalid JSON from language server: {}", e))?;
        Ok(Some(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn reads_framed_message() {
        let body = br#"{"jsonrpc":"2.0","id":1,"result":null}"#;
        let frame = format!("Content-Length: {}\r\n\r\n", body.len());
        let mut bytes = frame.into_bytes();
        bytes.extend_from_slice(body);
        let mut reader = BufReader::new(&bytes[..]);
        let msg = read_message(&mut reader).await.unwrap().unwrap();
        assert_eq!(msg["id"], json!(1));
    }

    #[tokio::test]
    async fn eof_returns_none() {
        let bytes: &[u8] = b"";
        let mut reader = BufReader::new(bytes);
        assert!(read_message(&mut reader).await.unwrap().is_none());
    }

    #[test]
    fn frame_parser_handles_split_and_batched_frames() {
        let mut parser = FrameParser::new();
        let first = frame_bytes(&json!({"id": 1})).unwrap();
        let second = frame_bytes(&json!({"id": 2})).unwrap();
        let (head, rest) = first.split_at(10);
        parser.feed(head);
        assert!(parser.next_message().unwrap().is_none());
        parser.feed(rest);
        parser.feed(&second);
        assert_eq!(parser.next_message().unwrap().unwrap()["id"], json!(1));
        assert_eq!(parser.next_message().unwrap().unwrap()["id"], json!(2));
        assert!(parser.next_message().unwrap().is_none());
    }

    #[tokio::test]
    async fn writes_framed_message() {
        let mut buf: Vec<u8> = Vec::new();
        write_message(&mut buf, &json!({"jsonrpc": "2.0", "method": "exit"}))
            .await
            .unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.starts_with("Content-Length: 33\r\n\r\n"));
        assert!(text.ends_with(r#"{"jsonrpc":"2.0","method":"exit"}"#));
    }
}
