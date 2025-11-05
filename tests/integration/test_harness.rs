// Test harness for automated integration test setup/teardown
// Automatically starts all layer servers, waits for readiness, and cleans up

use std::path::Path;
use std::process::{Child, Command};
use std::time::{Duration, Instant};
use tokio::net::UnixStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use serde_json::{json, Value};

// Socket paths for all layers
const LAYER1_SOCKET: &str = "/tmp/mfn_layer1.sock";
const LAYER2_SOCKET: &str = "/tmp/mfn_layer2.sock";
const LAYER3_SOCKET: &str = "/tmp/mfn_layer3.sock";
const LAYER4_SOCKET: &str = "/tmp/mfn_layer4.sock";

// Binary paths (relative to workspace root)
const LAYER1_BIN: &str = "./layer1-zig-ifr/zig-out/bin/ifr_socket_server";
const LAYER2_BIN: &str = "./target/release/layer2_socket_server";
const LAYER3_BIN: &str = "./layer3-go-alm/layer3_alm";
const LAYER4_BIN: &str = "./target/release/layer4_socket_server";

// Timeouts
const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
const HEALTH_CHECK_TIMEOUT: Duration = Duration::from_secs(5);

/// Test environment that manages all layer servers
pub struct TestEnvironment {
    layer1_process: Option<Child>,
    layer2_process: Option<Child>,
    layer3_process: Option<Child>,
    layer4_process: Option<Child>,
}

impl TestEnvironment {
    /// Setup test environment - clean sockets, start servers, wait for health
    pub async fn setup() -> Result<Self, String> {
        println!("\n=== Setting up test environment ===");

        // Step 1: Cleanup old sockets
        Self::cleanup_sockets()?;

        // Step 2: Start all layer servers
        let mut env = Self {
            layer1_process: None,
            layer2_process: None,
            layer3_process: None,
            layer4_process: None,
        };
        env.start_all_layers()?;

        // Step 3: Wait for layers to be ready
        env.wait_for_layers_ready().await?;

        // Step 4: Health check all layers
        env.health_check_all().await?;

        println!("=== Test environment ready ===\n");
        Ok(env)
    }

    /// Cleanup sockets from previous runs
    fn cleanup_sockets() -> Result<(), String> {
        println!("  Cleaning up old socket files...");

        for socket_path in &[LAYER1_SOCKET, LAYER2_SOCKET, LAYER3_SOCKET, LAYER4_SOCKET] {
            if Path::new(socket_path).exists() {
                std::fs::remove_file(socket_path)
                    .map_err(|e| format!("Failed to remove {}: {}", socket_path, e))?;
            }
        }

        println!("  ✓ Sockets cleaned");
        Ok(())
    }

    /// Start all layer server processes
    fn start_all_layers(&mut self) -> Result<(), String> {
        println!("  Starting layer servers...");

        // Start Layer 1 (Zig IFR)
        if Path::new(LAYER1_BIN).exists() {
            match Command::new(LAYER1_BIN)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                Ok(child) => {
                    println!("    Layer 1 started (PID: {})", child.id());
                    self.layer1_process = Some(child);
                }
                Err(e) => println!("    ⚠️  Layer 1 failed to start: {}", e),
            }
        } else {
            println!("    ⚠️  Layer 1 binary not found at {}", LAYER1_BIN);
        }

        // Start Layer 2 (Rust DSR)
        if Path::new(LAYER2_BIN).exists() {
            match Command::new(LAYER2_BIN)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                Ok(child) => {
                    println!("    Layer 2 started (PID: {})", child.id());
                    self.layer2_process = Some(child);
                }
                Err(e) => println!("    ⚠️  Layer 2 failed to start: {}", e),
            }
        } else {
            println!("    ⚠️  Layer 2 binary not found at {}", LAYER2_BIN);
        }

        // Start Layer 3 (Go ALM)
        if Path::new(LAYER3_BIN).exists() {
            match Command::new(LAYER3_BIN)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                Ok(child) => {
                    println!("    Layer 3 started (PID: {})", child.id());
                    self.layer3_process = Some(child);
                }
                Err(e) => println!("    ⚠️  Layer 3 failed to start: {}", e),
            }
        } else {
            println!("    ⚠️  Layer 3 binary not found at {}", LAYER3_BIN);
        }

        // Start Layer 4 (Rust CPE)
        if Path::new(LAYER4_BIN).exists() {
            match Command::new(LAYER4_BIN)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                Ok(child) => {
                    println!("    Layer 4 started (PID: {})", child.id());
                    self.layer4_process = Some(child);
                }
                Err(e) => println!("    ⚠️  Layer 4 failed to start: {}", e),
            }
        } else {
            println!("    ⚠️  Layer 4 binary not found at {}", LAYER4_BIN);
        }

        // Check if at least one layer started
        let started_count = [
            &self.layer1_process,
            &self.layer2_process,
            &self.layer3_process,
            &self.layer4_process,
        ]
        .iter()
        .filter(|p| p.is_some())
        .count();

        if started_count == 0 {
            return Err("No layers could be started. Check if binaries are built.".to_string());
        }

        println!("  ✓ {} layer(s) started", started_count);
        Ok(())
    }

    /// Wait for layer sockets to be created and accepting connections
    async fn wait_for_layers_ready(&self) -> Result<(), String> {
        println!("  Waiting for layers to be ready...");

        let start = Instant::now();
        let expected_sockets = vec![
            (LAYER1_SOCKET, self.layer1_process.is_some()),
            (LAYER2_SOCKET, self.layer2_process.is_some()),
            (LAYER3_SOCKET, self.layer3_process.is_some()),
            (LAYER4_SOCKET, self.layer4_process.is_some()),
        ];

        // Wait for all sockets to be created
        loop {
            if start.elapsed() > STARTUP_TIMEOUT {
                return Err(format!(
                    "Timeout waiting for layers to start ({}s)",
                    STARTUP_TIMEOUT.as_secs()
                ));
            }

            let all_ready = expected_sockets
                .iter()
                .filter(|(_, should_exist)| *should_exist)
                .all(|(socket_path, _)| Path::new(socket_path).exists());

            if all_ready {
                break;
            }

            tokio::time::sleep(Duration::from_millis(200)).await;
        }

        // Give layers a moment to fully initialize and accept connections
        tokio::time::sleep(Duration::from_millis(1000)).await;

        println!("  ✓ All sockets created in {:.2}s", start.elapsed().as_secs_f64());
        Ok(())
    }

    /// Health check all running layers
    async fn health_check_all(&self) -> Result<(), String> {
        println!("  Running health checks...");

        let mut checks_passed = 0;
        let mut checks_failed = 0;

        // Health check Layer 1 (newline-delimited JSON)
        if self.layer1_process.is_some() && Path::new(LAYER1_SOCKET).exists() {
            match self.health_check_layer1().await {
                Ok(_) => {
                    println!("    Layer 1: ✓ HEALTHY");
                    checks_passed += 1;
                }
                Err(e) => {
                    println!("    Layer 1: ✗ FAILED - {}", e);
                    checks_failed += 1;
                }
            }
        }

        // Health check Layer 2 (length-prefixed binary) - retry once on failure
        if self.layer2_process.is_some() && Path::new(LAYER2_SOCKET).exists() {
            match self.health_check_layer2().await {
                Ok(_) => {
                    println!("    Layer 2: ✓ HEALTHY");
                    checks_passed += 1;
                }
                Err(e) => {
                    // Retry once after brief delay
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    match self.health_check_layer2().await {
                        Ok(_) => {
                            println!("    Layer 2: ✓ HEALTHY (retry)");
                            checks_passed += 1;
                        }
                        Err(e2) => {
                            println!("    Layer 2: ✗ FAILED - {} (retry: {})", e, e2);
                            checks_failed += 1;
                        }
                    }
                }
            }
        }

        // Health check Layer 3 (length-prefixed binary)
        if self.layer3_process.is_some() && Path::new(LAYER3_SOCKET).exists() {
            match self.health_check_layer3().await {
                Ok(_) => {
                    println!("    Layer 3: ✓ HEALTHY");
                    checks_passed += 1;
                }
                Err(e) => {
                    println!("    Layer 3: ✗ FAILED - {}", e);
                    checks_failed += 1;
                }
            }
        }

        // Health check Layer 4 (length-prefixed binary)
        if self.layer4_process.is_some() && Path::new(LAYER4_SOCKET).exists() {
            match self.health_check_layer4().await {
                Ok(_) => {
                    println!("    Layer 4: ✓ HEALTHY");
                    checks_passed += 1;
                }
                Err(e) => {
                    println!("    Layer 4: ✗ FAILED - {}", e);
                    checks_failed += 1;
                }
            }
        }

        if checks_passed == 0 {
            return Err("All health checks failed - no layers available".to_string());
        }

        if checks_failed > 0 {
            println!("  ⚠️  {} layer(s) healthy, {} failed (continuing with available layers)", checks_passed, checks_failed);
        } else {
            println!("  ✓ All {} layer(s) healthy", checks_passed);
        }

        Ok(())
    }

    /// Health check Layer 1 (Zig IFR) - uses newline-delimited JSON
    async fn health_check_layer1(&self) -> Result<(), String> {
        let mut stream = tokio::time::timeout(
            HEALTH_CHECK_TIMEOUT,
            UnixStream::connect(LAYER1_SOCKET),
        )
        .await
        .map_err(|_| "Connection timeout".to_string())?
        .map_err(|e| format!("Connection failed: {}", e))?;

        let ping = json!({
            "type": "ping",
            "request_id": "health_check"
        });

        // Send newline-delimited JSON
        let mut msg_bytes = serde_json::to_vec(&ping)
            .map_err(|e| format!("Serialization failed: {}", e))?;
        msg_bytes.push(b'\n');

        stream.write_all(&msg_bytes).await
            .map_err(|e| format!("Write failed: {}", e))?;

        // Read response until newline
        let mut resp_buf = Vec::new();
        let mut byte_buf = [0u8; 1];

        loop {
            stream.read_exact(&mut byte_buf).await
                .map_err(|e| format!("Read failed: {}", e))?;

            if byte_buf[0] == b'\n' {
                break;
            }
            resp_buf.push(byte_buf[0]);

            if resp_buf.len() > 10000 {
                return Err("Response too large".to_string());
            }
        }

        let _response: Value = serde_json::from_slice(&resp_buf)
            .map_err(|e| format!("Parse failed: {}", e))?;

        Ok(())
    }

    /// Health check Layer 2 (Rust DSR) - uses length-prefixed binary
    async fn health_check_layer2(&self) -> Result<(), String> {
        let mut stream = tokio::time::timeout(
            HEALTH_CHECK_TIMEOUT,
            UnixStream::connect(LAYER2_SOCKET),
        )
        .await
        .map_err(|_| "Connection timeout".to_string())?
        .map_err(|e| format!("Connection failed: {}", e))?;

        let ping = json!({
            "Ping": {
                "request_id": "health_check"
            }
        });

        self.send_and_receive_binary(&mut stream, ping).await
    }

    /// Health check Layer 3 (Go ALM) - uses length-prefixed binary
    async fn health_check_layer3(&self) -> Result<(), String> {
        let mut stream = tokio::time::timeout(
            HEALTH_CHECK_TIMEOUT,
            UnixStream::connect(LAYER3_SOCKET),
        )
        .await
        .map_err(|_| "Connection timeout".to_string())?
        .map_err(|e| format!("Connection failed: {}", e))?;

        let ping = json!({
            "type": "ping",
            "request_id": "health_check"
        });

        self.send_and_receive_binary(&mut stream, ping).await
    }

    /// Health check Layer 4 (Rust CPE) - uses length-prefixed binary
    async fn health_check_layer4(&self) -> Result<(), String> {
        let mut stream = tokio::time::timeout(
            HEALTH_CHECK_TIMEOUT,
            UnixStream::connect(LAYER4_SOCKET),
        )
        .await
        .map_err(|_| "Connection timeout".to_string())?
        .map_err(|e| format!("Connection failed: {}", e))?;

        let ping = json!({
            "type": "Ping",
            "request_id": "health_check"
        });

        self.send_and_receive_binary(&mut stream, ping).await
    }

    /// Helper: Send and receive using length-prefixed binary protocol
    async fn send_and_receive_binary(&self, stream: &mut UnixStream, message: Value) -> Result<(), String> {
        let msg_bytes = serde_json::to_vec(&message)
            .map_err(|e| format!("Serialization failed: {}", e))?;

        let len = msg_bytes.len() as u32;

        stream.write_all(&len.to_le_bytes()).await
            .map_err(|e| format!("Write length failed: {}", e))?;
        stream.write_all(&msg_bytes).await
            .map_err(|e| format!("Write message failed: {}", e))?;

        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await
            .map_err(|e| format!("Read length failed: {}", e))?;
        let resp_len = u32::from_le_bytes(len_buf) as usize;

        if resp_len == 0 || resp_len > 10_000_000 {
            return Err(format!("Invalid response length: {}", resp_len));
        }

        let mut resp_buf = vec![0u8; resp_len];
        stream.read_exact(&mut resp_buf).await
            .map_err(|e| format!("Read message failed: {}", e))?;

        let _response: Value = serde_json::from_slice(&resp_buf)
            .map_err(|e| format!("Parse failed: {}", e))?;

        Ok(())
    }

    /// Teardown test environment - stop servers, clean sockets
    pub fn teardown(&mut self) {
        println!("\n=== Tearing down test environment ===");

        // Stop all layer processes
        if let Some(mut child) = self.layer1_process.take() {
            let _ = child.kill();
            println!("  Stopped Layer 1");
        }
        if let Some(mut child) = self.layer2_process.take() {
            let _ = child.kill();
            println!("  Stopped Layer 2");
        }
        if let Some(mut child) = self.layer3_process.take() {
            let _ = child.kill();
            println!("  Stopped Layer 3");
        }
        if let Some(mut child) = self.layer4_process.take() {
            let _ = child.kill();
            println!("  Stopped Layer 4");
        }

        // Clean up socket files
        let _ = Self::cleanup_sockets();

        println!("=== Teardown complete ===\n");
    }
}

// Implement Drop for emergency cleanup
impl Drop for TestEnvironment {
    fn drop(&mut self) {
        // Emergency cleanup if teardown wasn't called
        if self.layer1_process.is_some()
            || self.layer2_process.is_some()
            || self.layer3_process.is_some()
            || self.layer4_process.is_some()
        {
            println!("\n⚠️  Emergency cleanup triggered");
            self.teardown();
        }
    }
}
