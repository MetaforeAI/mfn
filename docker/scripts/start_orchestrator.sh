#!/bin/bash
# MFN Orchestrator Startup Script

set -e

# Configuration
export MFN_SOCKET_DIR="${MFN_SOCKET_DIR:-/app/sockets}"
export MFN_DATA_DIR="${MFN_DATA_DIR:-/app/data}"
export MFN_LOG_DIR="${MFN_LOG_DIR:-/app/logs}"

# Ensure socket directory exists
mkdir -p "$MFN_SOCKET_DIR"
chmod 755 "$MFN_SOCKET_DIR"

# Wait for all layers to be ready
echo "Waiting for MFN layers to initialize..."

wait_for_socket() {
    local socket_path=$1
    local layer_name=$2
    local max_wait=30
    local count=0

    while [ ! -S "$socket_path" ] && [ $count -lt $max_wait ]; do
        echo "  Waiting for $layer_name socket: $socket_path"
        sleep 1
        count=$((count + 1))
    done

    if [ -S "$socket_path" ]; then
        echo "  ✓ $layer_name ready"
        return 0
    else
        echo "  ✗ $layer_name failed to start"
        return 1
    fi
}

# Check each layer
wait_for_socket "$MFN_SOCKET_DIR/layer1.sock" "Layer 1 IFR"
wait_for_socket "$MFN_SOCKET_DIR/layer2.sock" "Layer 2 DSR"
wait_for_socket "$MFN_SOCKET_DIR/layer3.sock" "Layer 3 ALM"
wait_for_socket "$MFN_SOCKET_DIR/layer4.sock" "Layer 4 CPE"

echo "All layers ready. Starting MFN Orchestrator..."

# Start the orchestrator with circuit breaker and retry logic
exec python3 - <<'EOF'
import os
import sys
import time
import json
import socket
import threading
import logging
from collections import deque
from datetime import datetime, timedelta

# Add lib to path
sys.path.insert(0, '/app/lib')

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger('MFNOrchestrator')

class CircuitBreaker:
    """Circuit breaker for layer connections"""

    def __init__(self, failure_threshold=5, recovery_timeout=60):
        self.failure_threshold = failure_threshold
        self.recovery_timeout = recovery_timeout
        self.failure_count = 0
        self.last_failure_time = None
        self.state = 'closed'  # closed, open, half-open

    def record_success(self):
        self.failure_count = 0
        self.state = 'closed'

    def record_failure(self):
        self.failure_count += 1
        self.last_failure_time = datetime.now()

        if self.failure_count >= self.failure_threshold:
            self.state = 'open'
            logger.warning(f"Circuit breaker opened after {self.failure_count} failures")

    def can_attempt(self):
        if self.state == 'closed':
            return True

        if self.state == 'open':
            if self.last_failure_time and \
               datetime.now() - self.last_failure_time > timedelta(seconds=self.recovery_timeout):
                self.state = 'half-open'
                logger.info("Circuit breaker entering half-open state")
                return True
            return False

        # half-open state
        return True

class LayerConnection:
    """Managed connection to a layer with retry logic"""

    def __init__(self, layer_name, socket_path):
        self.layer_name = layer_name
        self.socket_path = socket_path
        self.socket = None
        self.circuit_breaker = CircuitBreaker()
        self.lock = threading.Lock()

    def connect(self, retry_count=3):
        """Connect with exponential backoff retry"""
        for attempt in range(retry_count):
            if not self.circuit_breaker.can_attempt():
                logger.warning(f"{self.layer_name}: Circuit breaker is open")
                return False

            try:
                self.socket = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                self.socket.settimeout(5.0)
                self.socket.connect(self.socket_path)
                self.circuit_breaker.record_success()
                logger.info(f"{self.layer_name}: Connected successfully")
                return True

            except Exception as e:
                self.circuit_breaker.record_failure()
                wait_time = 2 ** attempt  # Exponential backoff
                logger.error(f"{self.layer_name}: Connection failed (attempt {attempt + 1}): {e}")

                if attempt < retry_count - 1:
                    time.sleep(wait_time)

        return False

    def send_command(self, command, data=None):
        """Send command to layer with automatic reconnection"""
        with self.lock:
            if not self.socket and not self.connect():
                return None

            try:
                message = {"command": command}
                if data:
                    message["data"] = data

                self.socket.send(json.dumps(message).encode() + b'\n')

                # Read response
                response_data = b''
                while True:
                    chunk = self.socket.recv(4096)
                    if not chunk:
                        break
                    response_data += chunk
                    if b'\n' in response_data:
                        break

                response = json.loads(response_data.decode().strip())
                self.circuit_breaker.record_success()
                return response

            except Exception as e:
                logger.error(f"{self.layer_name}: Command failed: {e}")
                self.circuit_breaker.record_failure()
                self.socket = None
                return None

    def close(self):
        """Close connection"""
        if self.socket:
            try:
                self.socket.close()
            except:
                pass
            self.socket = None

class MFNOrchestrator:
    """Central orchestrator for MFN system"""

    def __init__(self):
        self.socket_dir = os.environ.get('MFN_SOCKET_DIR', '/app/sockets')
        self.data_dir = os.environ.get('MFN_DATA_DIR', '/app/data')

        # Initialize layer connections
        self.layers = {
            'layer1': LayerConnection('Layer1-IFR', f'{self.socket_dir}/layer1.sock'),
            'layer2': LayerConnection('Layer2-DSR', f'{self.socket_dir}/layer2.sock'),
            'layer3': LayerConnection('Layer3-ALM', f'{self.socket_dir}/layer3.sock'),
            'layer4': LayerConnection('Layer4-CPE', f'{self.socket_dir}/layer4.sock')
        }

        # Performance metrics
        self.metrics = {
            'requests_processed': 0,
            'errors': 0,
            'layer_latencies': {name: deque(maxlen=100) for name in self.layers},
            'start_time': datetime.now()
        }

        # Start health check thread
        self.running = True
        self.health_thread = threading.Thread(target=self.health_check_loop, daemon=True)
        self.health_thread.start()

    def health_check_loop(self):
        """Periodic health checks"""
        while self.running:
            for name, layer in self.layers.items():
                try:
                    start = time.time()
                    response = layer.send_command('health')
                    latency = time.time() - start

                    if response and response.get('status') == 'healthy':
                        self.metrics['layer_latencies'][name].append(latency)
                        logger.debug(f"{name}: Health check OK (latency: {latency:.3f}s)")
                    else:
                        logger.warning(f"{name}: Health check failed")

                except Exception as e:
                    logger.error(f"{name}: Health check error: {e}")

            time.sleep(30)  # Check every 30 seconds

    def process_memory(self, memory_data):
        """Process memory through all layers with parallel execution"""
        results = {}
        threads = []

        def process_layer(name, layer, memory_data):
            try:
                start = time.time()
                result = layer.send_command('add_memory', memory_data)
                latency = time.time() - start

                if result:
                    results[name] = {
                        'success': True,
                        'data': result,
                        'latency': latency
                    }
                    self.metrics['layer_latencies'][name].append(latency)
                else:
                    results[name] = {'success': False, 'error': 'No response'}

            except Exception as e:
                results[name] = {'success': False, 'error': str(e)}
                self.metrics['errors'] += 1

        # Start parallel processing
        for name, layer in self.layers.items():
            thread = threading.Thread(
                target=process_layer,
                args=(name, layer, memory_data)
            )
            thread.start()
            threads.append(thread)

        # Wait for all layers to complete
        for thread in threads:
            thread.join(timeout=10.0)

        self.metrics['requests_processed'] += 1
        return results

    def get_metrics(self):
        """Get system metrics"""
        uptime = (datetime.now() - self.metrics['start_time']).total_seconds()

        avg_latencies = {}
        for name, latencies in self.metrics['layer_latencies'].items():
            if latencies:
                avg_latencies[name] = sum(latencies) / len(latencies)
            else:
                avg_latencies[name] = 0

        return {
            'uptime_seconds': uptime,
            'requests_processed': self.metrics['requests_processed'],
            'errors': self.metrics['errors'],
            'error_rate': self.metrics['errors'] / max(1, self.metrics['requests_processed']),
            'average_latencies': avg_latencies,
            'layer_states': {
                name: layer.circuit_breaker.state
                for name, layer in self.layers.items()
            }
        }

    def restore_from_persistence(self):
        """Restore system state from persistence"""
        try:
            from add_persistence import MFNPersistenceManager

            persistence = MFNPersistenceManager(self.data_dir)
            memories = persistence.load_all_memories()

            logger.info(f"Restoring {len(memories)} memories from persistence")

            for memory in memories:
                memory_data = {
                    'id': memory.id,
                    'content': memory.content,
                    'tags': memory.tags,
                    'metadata': memory.metadata,
                    'embedding': memory.embedding
                }
                self.process_memory(memory_data)

            logger.info("System state restored successfully")
            return True

        except Exception as e:
            logger.error(f"Failed to restore from persistence: {e}")
            return False

    def run(self):
        """Main orchestrator loop"""
        logger.info("MFN Orchestrator started")

        # Attempt to restore state
        self.restore_from_persistence()

        # Keep running
        try:
            while self.running:
                time.sleep(1)

                # Log metrics periodically
                if self.metrics['requests_processed'] % 100 == 0 and self.metrics['requests_processed'] > 0:
                    metrics = self.get_metrics()
                    logger.info(f"Metrics: {json.dumps(metrics, indent=2)}")

        except KeyboardInterrupt:
            logger.info("Shutting down orchestrator")
            self.shutdown()

    def shutdown(self):
        """Graceful shutdown"""
        self.running = False

        # Close all layer connections
        for layer in self.layers.values():
            layer.close()

        logger.info("MFN Orchestrator shutdown complete")

if __name__ == "__main__":
    orchestrator = MFNOrchestrator()
    orchestrator.run()
EOF