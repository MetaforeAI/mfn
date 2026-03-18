#!/usr/bin/env python3
"""
MFN Metrics Collector
Native metrics collection system for the MFN dashboard
Zero external dependencies - uses only Python standard library
"""

import json
import time
import threading
import socket
import os
import sys
import http.server
import socketserver
from datetime import datetime
from collections import deque
from pathlib import Path

class MetricsCollector:
    """Collects metrics from all MFN layers"""

    def __init__(self):
        self.metrics = {
            'system': {
                'qps': 0,
                'latency': 0,
                'memory': 0,
                'connections': 0,
                'error_rate': 0,
                'uptime': 0,
                'start_time': time.time()
            },
            'layers': {
                'layer1': {
                    'name': 'IFR',
                    'status': 'healthy',
                    'latency': 0.5,
                    'memory': 12,
                    'hit_rate': 95,
                    'entries': 10000
                },
                'layer2': {
                    'name': 'DSR',
                    'status': 'healthy',
                    'latency': 30,
                    'memory': 48,
                    'accuracy': 92,
                    'neurons': 100000
                },
                'layer3': {
                    'name': 'ALM',
                    'status': 'healthy',
                    'latency': 160,
                    'memory': 128,
                    'graph_size': 50000,
                    'edges': 500000
                },
                'layer4': {
                    'name': 'CPE',
                    'status': 'degraded',
                    'latency': None,
                    'memory': None,
                    'patterns': None,
                    'accuracy': None
                },
                'layer5': {
                    'name': 'PSR',
                    'status': 'healthy',
                    'latency': 1,
                    'memory': 16,
                    'patterns': 0,
                    'searches': 0
                }
            },
            'history': {
                'qps': deque(maxlen=3600),
                'latency': deque(maxlen=3600),
                'memory': deque(maxlen=3600),
                'errors': deque(maxlen=3600)
            },
            'logs': deque(maxlen=1000)
        }

        self.layer_sockets = {
            'layer1': '/tmp/mfn_test_layer1.sock',
            'layer2': '/tmp/mfn_test_layer2.sock',
            'layer3': '/tmp/mfn_test_layer3.sock',
            'layer4': '/tmp/mfn_test_layer4.sock',
            'layer5': '/tmp/mfn_test_layer5.sock'
        }

        self.running = False
        self.collection_thread = None

    def start(self):
        """Start metrics collection"""
        self.running = True
        self.collection_thread = threading.Thread(target=self._collect_loop)
        self.collection_thread.daemon = True
        self.collection_thread.start()

        self.add_log('info', 'Metrics collection started')

    def stop(self):
        """Stop metrics collection"""
        self.running = False
        if self.collection_thread:
            self.collection_thread.join()

        self.add_log('info', 'Metrics collection stopped')

    def _collect_loop(self):
        """Main collection loop"""
        while self.running:
            try:
                self._collect_system_metrics()
                self._collect_layer_metrics()
                self._update_history()
                time.sleep(1)  # Collect every second
            except Exception as e:
                self.add_log('error', f'Collection error: {str(e)}')

    def _collect_system_metrics(self):
        """Collect system-wide metrics"""
        # Simulate metrics - replace with actual collection
        import random

        self.metrics['system']['qps'] = 95 + random.uniform(-5, 5)
        self.metrics['system']['latency'] = 10 + random.uniform(-2, 2)
        self.metrics['system']['memory'] = 35 + random.uniform(-5, 5)
        self.metrics['system']['connections'] = random.randint(8, 15)
        self.metrics['system']['error_rate'] = random.uniform(0, 0.5)
        self.metrics['system']['uptime'] = time.time() - self.metrics['system']['start_time']

    def _collect_layer_metrics(self):
        """Collect metrics from each layer"""
        for layer_id, socket_path in self.layer_sockets.items():
            if os.path.exists(socket_path):
                # Try to connect to layer socket
                try:
                    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
                        sock.settimeout(0.1)
                        sock.connect(socket_path)

                        # Send metrics request
                        request = json.dumps({'type': 'metrics'}).encode()
                        sock.sendall(request)

                        # Receive response
                        response = sock.recv(4096)
                        metrics = json.loads(response.decode())

                        # Update layer metrics
                        self.metrics['layers'][layer_id].update(metrics)
                        self.metrics['layers'][layer_id]['status'] = 'healthy'

                except (socket.error, json.JSONDecodeError, KeyError):
                    # Layer not responding properly
                    if self.metrics['layers'][layer_id]['status'] == 'healthy':
                        self.metrics['layers'][layer_id]['status'] = 'degraded'
                        self.add_log('warning', f'{layer_id} is not responding')
            else:
                # Socket doesn't exist
                self.metrics['layers'][layer_id]['status'] = 'failed'

    def _update_history(self):
        """Update historical data"""
        timestamp = time.time() * 1000  # Convert to milliseconds

        self.metrics['history']['qps'].append({
            'time': timestamp,
            'value': self.metrics['system']['qps']
        })

        self.metrics['history']['latency'].append({
            'time': timestamp,
            'value': self.metrics['system']['latency']
        })

        self.metrics['history']['memory'].append({
            'time': timestamp,
            'value': self.metrics['system']['memory']
        })

        self.metrics['history']['errors'].append({
            'time': timestamp,
            'value': self.metrics['system']['error_rate']
        })

    def add_log(self, level, message):
        """Add a log entry"""
        entry = {
            'timestamp': datetime.now().isoformat(),
            'level': level,
            'message': message
        }
        self.metrics['logs'].append(entry)

    def get_metrics(self):
        """Get current metrics snapshot"""
        # Convert deques to lists for JSON serialization
        metrics_copy = self.metrics.copy()
        metrics_copy['history'] = {
            'qps': list(self.metrics['history']['qps']),
            'latency': list(self.metrics['history']['latency']),
            'memory': list(self.metrics['history']['memory']),
            'errors': list(self.metrics['history']['errors'])
        }
        metrics_copy['logs'] = list(self.metrics['logs'])

        return metrics_copy


class MetricsHTTPHandler(http.server.SimpleHTTPRequestHandler):
    """HTTP handler for serving metrics and dashboard"""

    def __init__(self, *args, collector=None, **kwargs):
        self.collector = collector
        super().__init__(*args, **kwargs)

    def do_GET(self):
        """Handle GET requests"""
        if self.path == '/':
            # Serve dashboard
            self.serve_file('index.html', 'text/html')
        elif self.path == '/dashboard.js':
            # Serve JavaScript
            self.serve_file('dashboard.js', 'application/javascript')
        elif self.path == '/api/metrics':
            # Serve metrics JSON
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            cors_origin = os.environ.get('MFN_CORS_ORIGIN', 'http://localhost:3000')
            self.send_header('Access-Control-Allow-Origin', cors_origin)
            self.end_headers()

            metrics = self.collector.get_metrics()
            self.wfile.write(json.dumps(metrics).encode())
        elif self.path == '/api/logs':
            # Serve logs
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            cors_origin = os.environ.get('MFN_CORS_ORIGIN', 'http://localhost:3000')
            self.send_header('Access-Control-Allow-Origin', cors_origin)
            self.end_headers()

            logs = list(self.collector.metrics['logs'])
            self.wfile.write(json.dumps(logs).encode())
        else:
            self.send_error(404)

    def serve_file(self, filename, content_type):
        """Serve a static file"""
        filepath = Path(__file__).parent / filename
        if filepath.exists():
            self.send_response(200)
            self.send_header('Content-Type', content_type)
            self.end_headers()

            with open(filepath, 'rb') as f:
                self.wfile.write(f.read())
        else:
            self.send_error(404)

    def log_message(self, format, *args):
        """Suppress default logging"""
        pass


class MetricsServer:
    """HTTP server for dashboard and API"""

    def __init__(self, port=8080):
        self.port = port
        self.collector = MetricsCollector()
        self.server = None

    def start(self):
        """Start the metrics server"""
        # Start metrics collection
        self.collector.start()

        # Create HTTP server
        handler = lambda *args, **kwargs: MetricsHTTPHandler(
            *args, collector=self.collector, **kwargs
        )

        self.server = socketserver.TCPServer(('', self.port), handler)
        self.server.allow_reuse_address = True

        print(f"MFN Dashboard running on http://localhost:{self.port}")
        print("Press Ctrl+C to stop")

        try:
            self.server.serve_forever()
        except KeyboardInterrupt:
            self.stop()

    def stop(self):
        """Stop the server"""
        print("\nShutting down...")
        self.collector.stop()
        if self.server:
            self.server.shutdown()


def check_layer_status():
    """Check the status of each MFN layer"""
    print("\nChecking MFN layer status...")

    layers = {
        'Layer 1 (IFR)': '/tmp/mfn_test_layer1.sock',
        'Layer 2 (DSR)': '/tmp/mfn_test_layer2.sock',
        'Layer 3 (ALM)': '/tmp/mfn_test_layer3.sock',
        'Layer 4 (CPE)': '/tmp/mfn_test_layer4.sock',
        'Layer 5 (PSR)': '/tmp/mfn_test_layer5.sock'
    }

    for name, socket_path in layers.items():
        if os.path.exists(socket_path):
            print(f"  ✓ {name}: Socket exists at {socket_path}")
        else:
            print(f"  ✗ {name}: No socket at {socket_path}")

    print()


def main():
    """Main entry point"""
    print("========================================")
    print("  MFN System Dashboard")
    print("  Native monitoring - Zero dependencies")
    print("========================================")
    print()

    # Check layer status
    check_layer_status()

    # Start server
    server = MetricsServer(port=8080)
    server.start()


if __name__ == '__main__':
    main()