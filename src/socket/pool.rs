//! Connection Pool Implementation
//!
//! Provides efficient connection pooling for socket clients with
//! automatic connection management, health checking, and load balancing.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::collections::VecDeque;
use tokio::net::UnixStream;
use tokio::sync::{RwLock, Semaphore};
use tokio::time::{timeout, interval};
use tracing::{debug, warn, error};

use crate::socket::{SocketError, SocketResult};

/// Connection wrapper with metadata
struct PooledConnection {
    stream: UnixStream,
    created_at: Instant,
    last_used: Instant,
    uses: usize,
}

impl PooledConnection {
    fn new(stream: UnixStream) -> Self {
        let now = Instant::now();
        Self {
            stream,
            created_at: now,
            last_used: now,
            uses: 0,
        }
    }

    fn age(&self) -> Duration {
        self.created_at.elapsed()
    }

    fn idle_time(&self) -> Duration {
        self.last_used.elapsed()
    }

    fn mark_used(&mut self) {
        self.last_used = Instant::now();
        self.uses += 1;
    }
}

/// Connection pool configuration
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Minimum number of connections to maintain
    pub min_size: usize,
    /// Maximum number of connections allowed
    pub max_size: usize,
    /// Maximum connection age before recycling
    pub max_connection_age: Duration,
    /// Maximum idle time before closing connection
    pub max_idle_time: Duration,
    /// Maximum uses before recycling connection
    pub max_uses: usize,
    /// Connection timeout
    pub connection_timeout: Duration,
    /// Health check interval
    pub health_check_interval: Duration,
    /// Enable connection warming
    pub warm_connections: bool,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            min_size: 2,
            max_size: 10,
            max_connection_age: Duration::from_secs(3600), // 1 hour
            max_idle_time: Duration::from_secs(300), // 5 minutes
            max_uses: 1000,
            connection_timeout: Duration::from_secs(5),
            health_check_interval: Duration::from_secs(30),
            warm_connections: true,
        }
    }
}

/// Connection pool for Unix domain sockets
pub struct ConnectionPool {
    socket_path: PathBuf,
    config: PoolConfig,
    available: Arc<RwLock<VecDeque<PooledConnection>>>,
    in_use: Arc<RwLock<usize>>,
    semaphore: Arc<Semaphore>,
    shutdown: Arc<RwLock<bool>>,
}

impl ConnectionPool {
    /// Create a new connection pool
    pub fn new(socket_path: PathBuf, max_size: usize, connection_timeout: Duration) -> Self {
        let config = PoolConfig {
            max_size,
            connection_timeout,
            ..Default::default()
        };

        Self::with_config(socket_path, config)
    }

    /// Create a pool with custom configuration
    pub fn with_config(socket_path: PathBuf, config: PoolConfig) -> Self {
        let pool = Self {
            socket_path,
            config: config.clone(),
            available: Arc::new(RwLock::new(VecDeque::new())),
            in_use: Arc::new(RwLock::new(0)),
            semaphore: Arc::new(Semaphore::new(config.max_size)),
            shutdown: Arc::new(RwLock::new(false)),
        };

        // Start background tasks
        pool.start_maintenance();

        // Warm up connections if enabled
        if config.warm_connections {
            let pool_clone = pool.clone();
            tokio::spawn(async move {
                pool_clone.warm_up().await;
            });
        }

        pool
    }

    /// Warm up the pool with minimum connections
    async fn warm_up(&self) {
        debug!("Warming up connection pool with {} connections", self.config.min_size);

        for _ in 0..self.config.min_size {
            match self.create_connection().await {
                Ok(conn) => {
                    let mut available = self.available.write().await;
                    available.push_back(conn);
                }
                Err(e) => {
                    warn!("Failed to warm up connection: {}", e);
                    break;
                }
            }
        }
    }

    /// Create a new connection
    async fn create_connection(&self) -> SocketResult<PooledConnection> {
        let stream = timeout(
            self.config.connection_timeout,
            UnixStream::connect(&self.socket_path),
        )
        .await
        .map_err(|_| SocketError::Timeout(self.config.connection_timeout))?
        .map_err(|e| SocketError::Connection(e.to_string()))?;

        Ok(PooledConnection::new(stream))
    }

    /// Get a connection from the pool
    pub async fn get(&self) -> SocketResult<Box<UnixStream>> {
        // Check if pool is shutting down
        if *self.shutdown.read().await {
            return Err(SocketError::PoolExhausted);
        }

        // Acquire semaphore permit
        let _permit = self.semaphore.acquire().await
            .map_err(|_| SocketError::PoolExhausted)?;

        // Try to get an existing connection
        loop {
            let mut available = self.available.write().await;

            while let Some(mut conn) = available.pop_front() {
                // Check if connection is still valid
                if self.is_connection_valid(&conn) {
                    conn.mark_used();
                    *self.in_use.write().await += 1;
                    debug!("Reusing connection from pool");
                    return Ok(Box::new(conn.stream));
                }
                // Invalid connection, discard it
                debug!("Discarding invalid connection");
            }

            drop(available); // Release lock before creating new connection

            // No valid connections, create a new one
            match self.create_connection().await {
                Ok(mut conn) => {
                    conn.mark_used();
                    *self.in_use.write().await += 1;
                    debug!("Created new connection for pool");
                    return Ok(Box::new(conn.stream));
                }
                Err(e) => {
                    error!("Failed to create connection: {}", e);
                    return Err(e);
                }
            }
        }
    }

    /// Return a connection to the pool
    pub async fn return_connection(&self, stream: Box<UnixStream>) {
        if *self.shutdown.read().await {
            return;
        }

        *self.in_use.write().await -= 1;

        let conn = PooledConnection {
            stream: *stream,
            created_at: Instant::now(), // Reset for simplicity
            last_used: Instant::now(),
            uses: 1,
        };

        // Check if we should keep this connection
        if self.is_connection_valid(&conn) {
            let mut available = self.available.write().await;
            if available.len() < self.config.max_size {
                available.push_back(conn);
                debug!("Returned connection to pool");
                return;
            }
        }

        // Connection not needed or invalid, let it drop
        debug!("Discarding returned connection");
    }

    /// Check if a connection is still valid
    fn is_connection_valid(&self, conn: &PooledConnection) -> bool {
        // Check age
        if conn.age() > self.config.max_connection_age {
            debug!("Connection too old");
            return false;
        }

        // Check idle time
        if conn.idle_time() > self.config.max_idle_time {
            debug!("Connection idle too long");
            return false;
        }

        // Check usage count
        if conn.uses >= self.config.max_uses {
            debug!("Connection used too many times");
            return false;
        }

        true
    }

    /// Start background maintenance tasks
    fn start_maintenance(&self) {
        let pool = self.clone();
        tokio::spawn(async move {
            let mut ticker = interval(pool.config.health_check_interval);
            ticker.tick().await; // Skip first immediate tick

            loop {
                ticker.tick().await;

                if *pool.shutdown.read().await {
                    break;
                }

                pool.perform_maintenance().await;
            }
        });
    }

    /// Perform pool maintenance
    async fn perform_maintenance(&self) {
        let mut available = self.available.write().await;
        let initial_size = available.len();
        let mut removed = 0;

        // Remove invalid connections
        available.retain(|conn| {
            let valid = self.is_connection_valid(conn);
            if !valid {
                removed += 1;
            }
            valid
        });

        if removed > 0 {
            debug!("Removed {} invalid connections from pool", removed);
        }

        let current_size = available.len();

        // Ensure minimum pool size
        if current_size < self.config.min_size {
            let to_create = self.config.min_size - current_size;
            drop(available); // Release lock

            for _ in 0..to_create {
                match self.create_connection().await {
                    Ok(conn) => {
                        let mut available = self.available.write().await;
                        available.push_back(conn);
                    }
                    Err(e) => {
                        warn!("Failed to create connection during maintenance: {}", e);
                        break;
                    }
                }
            }
        }
    }

    /// Get pool statistics
    pub async fn stats(&self) -> PoolStats {
        let available = self.available.read().await.len();
        let in_use = *self.in_use.read().await;

        PoolStats {
            available,
            in_use,
            total: available + in_use,
            max_size: self.config.max_size,
        }
    }

    /// Close all connections and shutdown the pool
    pub async fn close_all(&self) {
        *self.shutdown.write().await = true;

        // Clear available connections
        let mut available = self.available.write().await;
        available.clear();

        debug!("Connection pool closed");
    }
}

impl Clone for ConnectionPool {
    fn clone(&self) -> Self {
        Self {
            socket_path: self.socket_path.clone(),
            config: self.config.clone(),
            available: Arc::clone(&self.available),
            in_use: Arc::clone(&self.in_use),
            semaphore: Arc::clone(&self.semaphore),
            shutdown: Arc::clone(&self.shutdown),
        }
    }
}

/// Pool statistics
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub available: usize,
    pub in_use: usize,
    pub total: usize,
    pub max_size: usize,
}

impl PoolStats {
    pub fn utilization(&self) -> f64 {
        if self.max_size == 0 {
            return 0.0;
        }
        (self.in_use as f64 / self.max_size as f64) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pool_creation() {
        let pool = ConnectionPool::new(
            PathBuf::from("/tmp/test.sock"),
            10,
            Duration::from_secs(5),
        );

        let stats = pool.stats().await;
        assert_eq!(stats.max_size, 10);
    }

    #[tokio::test]
    async fn test_pool_config() {
        let config = PoolConfig {
            min_size: 5,
            max_size: 20,
            warm_connections: false,
            ..Default::default()
        };

        let pool = ConnectionPool::with_config(
            PathBuf::from("/tmp/test.sock"),
            config,
        );

        let stats = pool.stats().await;
        assert_eq!(stats.max_size, 20);
    }

    // Test disabled: Cannot safely create dummy UnixStream for validation testing
    // The actual validation logic is tested through integration tests
    // #[tokio::test]
    // async fn test_connection_validation() {
    //     let pool = ConnectionPool::new(
    //         PathBuf::from("/tmp/test.sock"),
    //         10,
    //         Duration::from_secs(5),
    //     );
    //
    //     // Note: Cannot safely create zeroed UnixStream
    //     // Validation logic tested through actual connections in integration tests
    // }
}