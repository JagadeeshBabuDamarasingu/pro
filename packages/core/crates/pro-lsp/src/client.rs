use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lsp_types::notification::Notification;
use lsp_types::request::Request;
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::io::BufReader;
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};

use crate::transport::{read_message, write_message};

#[derive(Debug, thiserror::Error)]
pub enum LspError {
    #[error("language server binary not found: {0}")]
    ServerNotFound(String),
    #[error("failed to spawn language server: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("request timed out")]
    Timeout,
    #[error("connection to the language server closed")]
    ChannelClosed,
    #[error("language server returned an error: {0}")]
    Protocol(String),
    #[error("failed to (de)serialize an LSP message: {0}")]
    Json(#[from] serde_json::Error),
}

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// A minimal LSP client: spawns a language server subprocess and speaks
/// JSON-RPC over its stdio. Deliberately not a full LSP implementation —
/// only the surface `pro-grounding` actually needs (open a document, ask
/// for its definition, read published diagnostics).
pub struct LspClient {
    to_server: mpsc::UnboundedSender<serde_json::Value>,
    pending: Arc<Mutex<HashMap<i64, oneshot::Sender<serde_json::Value>>>>,
    diagnostics: Arc<Mutex<HashMap<String, Vec<lsp_types::Diagnostic>>>>,
    next_id: AtomicI64,
    _child: Child,
}

impl LspClient {
    /// Spawns `command` and performs the `initialize`/`initialized`
    /// handshake. Returns `LspError::ServerNotFound` immediately (never
    /// hangs) if the binary isn't on `PATH` — callers (the grounding gate)
    /// treat that as "fall back to AST/symbol-table," not a hard failure.
    pub async fn spawn(command: &str, args: &[&str], root_uri: lsp_types::Uri) -> Result<LspClient, LspError> {
        let mut child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|source| {
                if source.kind() == std::io::ErrorKind::NotFound {
                    LspError::ServerNotFound(command.to_string())
                } else {
                    LspError::Spawn(source)
                }
            })?;

        let mut stdin = child.stdin.take().expect("stdin was piped");
        let stdout = child.stdout.take().expect("stdout was piped");

        let (to_server, mut outbox) = mpsc::unbounded_channel::<serde_json::Value>();
        tokio::spawn(async move {
            while let Some(msg) = outbox.recv().await {
                if write_message(&mut stdin, &msg).await.is_err() {
                    break;
                }
            }
        });

        let pending: Arc<Mutex<HashMap<i64, oneshot::Sender<serde_json::Value>>>> = Arc::new(Mutex::new(HashMap::new()));
        let diagnostics: Arc<Mutex<HashMap<String, Vec<lsp_types::Diagnostic>>>> = Arc::new(Mutex::new(HashMap::new()));

        let pending_for_reader = pending.clone();
        let diagnostics_for_reader = diagnostics.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            loop {
                let msg = match read_message(&mut reader).await {
                    Ok(msg) => msg,
                    Err(_) => break,
                };
                if let Some(id) = msg.get("id").and_then(|v| v.as_i64()) {
                    if let Some(sender) = pending_for_reader.lock().unwrap().remove(&id) {
                        let _ = sender.send(msg);
                        continue;
                    }
                }
                if msg.get("method").and_then(|m| m.as_str()) == Some("textDocument/publishDiagnostics") {
                    if let Some(params) = msg.get("params").cloned() {
                        if let Ok(parsed) = serde_json::from_value::<lsp_types::PublishDiagnosticsParams>(params) {
                            diagnostics_for_reader.lock().unwrap().insert(parsed.uri.to_string(), parsed.diagnostics);
                        }
                    }
                }
            }
        });

        let client =
            LspClient { to_server, pending, diagnostics, next_id: AtomicI64::new(1), _child: child };
        client.initialize(root_uri).await?;
        Ok(client)
    }

    async fn request<R: Request>(&self, params: R::Params) -> Result<R::Result, LspError>
    where
        R::Params: Serialize,
        R::Result: DeserializeOwned,
    {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);

        let envelope = serde_json::json!({ "jsonrpc": "2.0", "id": id, "method": R::METHOD, "params": params });
        self.to_server.send(envelope).map_err(|_| LspError::ChannelClosed)?;

        let response = tokio::time::timeout(REQUEST_TIMEOUT, rx)
            .await
            .map_err(|_| LspError::Timeout)?
            .map_err(|_| LspError::ChannelClosed)?;

        if let Some(err) = response.get("error") {
            return Err(LspError::Protocol(err.to_string()));
        }
        let result = response.get("result").cloned().unwrap_or(serde_json::Value::Null);
        Ok(serde_json::from_value(result)?)
    }

    fn notify<N: Notification>(&self, params: N::Params) -> Result<(), LspError>
    where
        N::Params: Serialize,
    {
        let envelope = serde_json::json!({ "jsonrpc": "2.0", "method": N::METHOD, "params": params });
        self.to_server.send(envelope).map_err(|_| LspError::ChannelClosed)
    }

    async fn initialize(&self, root_uri: lsp_types::Uri) -> Result<(), LspError> {
        let workspace_folder =
            lsp_types::WorkspaceFolder { uri: root_uri, name: "workspace".to_string() };
        let params = lsp_types::InitializeParams { workspace_folders: Some(vec![workspace_folder]), ..Default::default() };
        let _: lsp_types::InitializeResult = self.request::<lsp_types::request::Initialize>(params).await?;
        self.notify::<lsp_types::notification::Initialized>(lsp_types::InitializedParams {})
    }

    pub fn did_open(&self, uri: lsp_types::Uri, language_id: &str, text: String) -> Result<(), LspError> {
        let params = lsp_types::DidOpenTextDocumentParams {
            text_document: lsp_types::TextDocumentItem { uri, language_id: language_id.to_string(), version: 1, text },
        };
        self.notify::<lsp_types::notification::DidOpenTextDocument>(params)
    }

    /// The grounding gate's main oracle call: does this position resolve
    /// to a real definition? `None`/empty response means it doesn't.
    pub async fn definition(
        &self,
        uri: lsp_types::Uri,
        position: lsp_types::Position,
    ) -> Result<Option<lsp_types::GotoDefinitionResponse>, LspError> {
        let params = lsp_types::GotoDefinitionParams {
            text_document_position_params: lsp_types::TextDocumentPositionParams {
                text_document: lsp_types::TextDocumentIdentifier { uri },
                position,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };
        self.request::<lsp_types::request::GotoDefinition>(params).await
    }

    /// Polls the latest diagnostics published for `uri`, up to `timeout` —
    /// rust-analyzer indexes asynchronously, so diagnostics for a just-
    /// opened document aren't necessarily available immediately.
    pub async fn wait_for_diagnostics(&self, uri: &lsp_types::Uri, timeout: Duration) -> Vec<lsp_types::Diagnostic> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if let Some(found) = self.diagnostics.lock().unwrap().get(&uri.to_string()).cloned() {
                return found;
            }
            if tokio::time::Instant::now() >= deadline {
                return Vec::new();
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    pub async fn shutdown(&self) -> Result<(), LspError> {
        let _: () = self.request::<lsp_types::request::Shutdown>(()).await?;
        self.notify::<lsp_types::notification::Exit>(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn spawning_a_nonexistent_binary_fails_fast_with_server_not_found() {
        let uri: lsp_types::Uri = "file:///tmp".parse().unwrap();
        let result = LspClient::spawn("pro-lsp-definitely-not-a-real-binary", &[], uri).await;
        assert!(matches!(result, Err(LspError::ServerNotFound(_))));
    }
}
