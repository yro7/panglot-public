use anyhow::{Context, Result, bail};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{Child, ChildStdin, ChildStdout};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Handle to a long-lived Python sidecar process.
/// Communicates via JSON-lines over stdin/stdout.
pub struct PythonSidecar {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

/// Shared, thread-safe handle to the sidecar.
pub type SharedSidecar = Arc<Mutex<PythonSidecar>>;

impl PythonSidecar {
    /// Spawns the Python sidecar process.
    /// Looks for `scripts/sidecar.py` relative to the current working directory.
    pub fn spawn() -> Result<Self> {
        let script_path = Self::find_script()?;

        let mut child = tokio::process::Command::new("python3")
            .arg(&script_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit()) // sidecar logs go to Rust's stderr
            .spawn()
            .context("Failed to spawn Python sidecar — is python3 installed?")?;

        let stdin = child.stdin.take().expect("stdin was piped");
        let stdout = child.stdout.take().expect("stdout was piped");

        tracing::info!(pid = ?child.id(), "Python sidecar started");

        Ok(Self {
            child,
            stdin: BufWriter::new(stdin),
            stdout: BufReader::new(stdout),
        })
    }

    /// Sends a JSON request and reads back a JSON response (one line).
    async fn request(&mut self, req: &Value) -> Result<Value> {
        let mut line = serde_json::to_string(req)?;
        line.push('\n');

        self.stdin.write_all(line.as_bytes()).await
            .context("Failed to write to sidecar stdin")?;
        self.stdin.flush().await
            .context("Failed to flush sidecar stdin")?;

        let mut response_line = String::new();
        let bytes_read = self.stdout.read_line(&mut response_line).await
            .context("Failed to read from sidecar stdout")?;

        if bytes_read == 0 {
            bail!("Sidecar process closed stdout unexpectedly");
        }

        let resp: Value = serde_json::from_str(response_line.trim())
            .context("Failed to parse sidecar response as JSON")?;

        if resp.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            let err = resp.get("error").and_then(|v| v.as_str()).unwrap_or("unknown error");
            bail!("Sidecar error: {}", err);
        }

        Ok(resp)
    }

    /// Request IPA transcription from the sidecar.
    pub async fn request_ipa(&mut self, lang: &str, text: &str) -> Result<String> {
        let req = serde_json::json!({
            "cmd": "ipa",
            "lang": lang,
            "text": text,
        });

        let resp = self.request(&req).await?;

        resp.get("ipa")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Sidecar IPA response missing 'ipa' field"))
    }

    /// Request TTS audio generation from the sidecar.
    pub async fn request_tts(&mut self, voice: &str, text: &str, output_path: &str) -> Result<String> {
        let req = serde_json::json!({
            "cmd": "tts",
            "voice": voice,
            "text": text,
            "output_path": output_path,
        });

        let resp = self.request(&req).await?;

        resp.get("audio_file")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Sidecar TTS response missing 'audio_file' field"))
    }

    /// Gracefully shuts down the sidecar process.
    pub async fn shutdown(&mut self) -> Result<()> {
        let quit = serde_json::json!({"cmd": "quit"});
        let mut line = serde_json::to_string(&quit)?;
        line.push('\n');

        // Best-effort write — process may already be dead
        let _ = self.stdin.write_all(line.as_bytes()).await;
        let _ = self.stdin.flush().await;
        let _ = self.child.wait().await;

        tracing::info!("Python sidecar stopped");
        Ok(())
    }

    /// Find the sidecar script, checking common locations.
    fn find_script() -> Result<String> {
        let candidates = [
            "scripts/sidecar.py",
            "../scripts/sidecar.py",
        ];
        for path in &candidates {
            if std::path::Path::new(path).exists() {
                return Ok(path.to_string());
            }
        }
        bail!(
            "Could not find scripts/sidecar.py. Looked in: {:?}",
            candidates
        )
    }
}

impl Drop for PythonSidecar {
    fn drop(&mut self) {
        // Best-effort kill if the process is still running
        let _ = self.child.start_kill();
    }
}
