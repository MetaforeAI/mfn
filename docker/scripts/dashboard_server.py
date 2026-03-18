#!/usr/bin/env python3
"""
MFN Dashboard Server
Real-time monitoring and management interface
"""

import os
import sys
import json
import asyncio
import logging
from datetime import datetime
from typing import Dict, List, Any
from pathlib import Path

from fastapi import FastAPI, WebSocket, WebSocketDisconnect
from fastapi.staticfiles import StaticFiles
from fastapi.responses import HTMLResponse
import uvicorn

# Add lib to path
sys.path.insert(0, '/app/lib')

# Configure logging
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

# Dashboard HTML
DASHBOARD_HTML = """
<!DOCTYPE html>
<html>
<head>
    <title>MFN System Dashboard</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: linear-gradient(135deg, #1e3c72 0%, #2a5298 100%);
            color: #fff;
            padding: 20px;
        }
        .container {
            max-width: 1400px;
            margin: 0 auto;
        }
        .header {
            text-align: center;
            margin-bottom: 30px;
        }
        h1 {
            font-size: 2.5em;
            margin-bottom: 10px;
        }
        .status-bar {
            display: flex;
            justify-content: center;
            gap: 20px;
            margin-bottom: 30px;
        }
        .status-item {
            padding: 10px 20px;
            background: rgba(255,255,255,0.1);
            border-radius: 20px;
            backdrop-filter: blur(10px);
        }
        .grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
            gap: 20px;
        }
        .card {
            background: rgba(255,255,255,0.1);
            backdrop-filter: blur(10px);
            border-radius: 10px;
            padding: 20px;
            border: 1px solid rgba(255,255,255,0.2);
        }
        .card h3 {
            margin-bottom: 15px;
            font-size: 1.2em;
            opacity: 0.9;
        }
        .metric {
            display: flex;
            justify-content: space-between;
            padding: 8px 0;
            border-bottom: 1px solid rgba(255,255,255,0.1);
        }
        .metric:last-child {
            border-bottom: none;
        }
        .metric-value {
            font-weight: bold;
        }
        .layer-status {
            display: grid;
            grid-template-columns: repeat(4, 1fr);
            gap: 10px;
            margin-bottom: 20px;
        }
        .layer-card {
            text-align: center;
            padding: 15px;
            background: rgba(255,255,255,0.1);
            border-radius: 8px;
            transition: all 0.3s ease;
        }
        .layer-card.active {
            background: rgba(76, 175, 80, 0.3);
            border: 1px solid #4CAF50;
        }
        .layer-card.inactive {
            background: rgba(244, 67, 54, 0.3);
            border: 1px solid #f44336;
        }
        .status-indicator {
            width: 12px;
            height: 12px;
            border-radius: 50%;
            display: inline-block;
            margin-left: 5px;
        }
        .status-healthy { background: #4CAF50; }
        .status-unhealthy { background: #f44336; }
        .status-degraded { background: #FFC107; }
        .chart-container {
            height: 200px;
            margin-top: 15px;
            position: relative;
        }
        .logs-container {
            background: rgba(0,0,0,0.3);
            border-radius: 5px;
            padding: 10px;
            height: 200px;
            overflow-y: auto;
            font-family: monospace;
            font-size: 0.9em;
        }
        .log-entry {
            padding: 2px 0;
            border-bottom: 1px solid rgba(255,255,255,0.05);
        }
        .log-entry.error { color: #f44336; }
        .log-entry.warning { color: #FFC107; }
        .log-entry.info { color: #2196F3; }
        .actions {
            margin-top: 20px;
            display: flex;
            gap: 10px;
        }
        button {
            padding: 10px 20px;
            background: rgba(255,255,255,0.2);
            border: 1px solid rgba(255,255,255,0.3);
            color: white;
            border-radius: 5px;
            cursor: pointer;
            transition: all 0.3s ease;
        }
        button:hover {
            background: rgba(255,255,255,0.3);
        }
        @keyframes pulse {
            0% { opacity: 1; }
            50% { opacity: 0.5; }
            100% { opacity: 1; }
        }
        .updating {
            animation: pulse 1s infinite;
        }
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>MFN System Dashboard</h1>
            <div id="connection-status">Connecting...</div>
        </div>

        <div class="status-bar">
            <div class="status-item">
                <span>System Status:</span>
                <span id="system-status" class="status-indicator status-healthy"></span>
            </div>
            <div class="status-item">
                <span>Uptime:</span>
                <span id="uptime">0h 0m</span>
            </div>
            <div class="status-item">
                <span>Requests:</span>
                <span id="total-requests">0</span>
            </div>
        </div>

        <div class="layer-status" id="layers">
            <div class="layer-card" id="layer1">
                <h4>Layer 1 - IFR</h4>
                <div class="metric-value">-</div>
            </div>
            <div class="layer-card" id="layer2">
                <h4>Layer 2 - DSR</h4>
                <div class="metric-value">-</div>
            </div>
            <div class="layer-card" id="layer3">
                <h4>Layer 3 - ALM</h4>
                <div class="metric-value">-</div>
            </div>
            <div class="layer-card" id="layer4">
                <h4>Layer 4 - CPE</h4>
                <div class="metric-value">-</div>
            </div>
        </div>

        <div class="grid">
            <div class="card">
                <h3>Performance Metrics</h3>
                <div class="metric">
                    <span>CPU Usage</span>
                    <span class="metric-value" id="cpu-usage">0%</span>
                </div>
                <div class="metric">
                    <span>Memory Usage</span>
                    <span class="metric-value" id="mem-usage">0%</span>
                </div>
                <div class="metric">
                    <span>Disk Usage</span>
                    <span class="metric-value" id="disk-usage">0%</span>
                </div>
                <div class="metric">
                    <span>Avg Response Time</span>
                    <span class="metric-value" id="response-time">0ms</span>
                </div>
            </div>

            <div class="card">
                <h3>Memory Statistics</h3>
                <div class="metric">
                    <span>Total Memories</span>
                    <span class="metric-value" id="total-memories">0</span>
                </div>
                <div class="metric">
                    <span>Associations</span>
                    <span class="metric-value" id="associations">0</span>
                </div>
                <div class="metric">
                    <span>Database Size</span>
                    <span class="metric-value" id="db-size">0 MB</span>
                </div>
                <div class="metric">
                    <span>Last Backup</span>
                    <span class="metric-value" id="last-backup">Never</span>
                </div>
            </div>

            <div class="card">
                <h3>Request Statistics</h3>
                <div class="chart-container" id="request-chart">
                    <canvas id="chart"></canvas>
                </div>
            </div>

            <div class="card">
                <h3>Recent Logs</h3>
                <div class="logs-container" id="logs">
                    <!-- Logs will be inserted here -->
                </div>
            </div>

            <div class="card">
                <h3>System Actions</h3>
                <div class="actions">
                    <button onclick="createBackup()">Create Backup</button>
                    <button onclick="runHealthCheck()">Health Check</button>
                    <button onclick="clearLogs()">Clear Logs</button>
                </div>
                <div id="action-result" style="margin-top: 10px;"></div>
            </div>
        </div>
    </div>

    <script>
        let ws = null;
        let reconnectAttempts = 0;

        function connectWebSocket() {
            const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
            ws = new WebSocket(`${protocol}//${window.location.host}/ws`);

            ws.onopen = () => {
                document.getElementById('connection-status').textContent = 'Connected';
                reconnectAttempts = 0;
            };

            ws.onmessage = (event) => {
                const data = JSON.parse(event.data);
                updateDashboard(data);
            };

            ws.onclose = () => {
                document.getElementById('connection-status').textContent = 'Disconnected - Reconnecting...';
                setTimeout(() => {
                    reconnectAttempts++;
                    if (reconnectAttempts < 10) {
                        connectWebSocket();
                    }
                }, 2000);
            };

            ws.onerror = (error) => {
                console.error('WebSocket error:', error);
            };
        }

        function updateDashboard(data) {
            // Update system status
            if (data.system_status) {
                const statusEl = document.getElementById('system-status');
                statusEl.className = `status-indicator status-${data.system_status}`;
            }

            // Update uptime
            if (data.uptime) {
                const hours = Math.floor(data.uptime / 3600);
                const minutes = Math.floor((data.uptime % 3600) / 60);
                document.getElementById('uptime').textContent = `${hours}h ${minutes}m`;
            }

            // Update layer statuses
            if (data.layers) {
                for (const [layer, status] of Object.entries(data.layers)) {
                    const layerEl = document.getElementById(layer);
                    if (layerEl) {
                        layerEl.className = `layer-card ${status.active ? 'active' : 'inactive'}`;
                        layerEl.querySelector('.metric-value').textContent =
                            `${status.memory_count || 0} memories`;
                    }
                }
            }

            // Update metrics
            if (data.metrics) {
                if (data.metrics.cpu) {
                    document.getElementById('cpu-usage').textContent = `${data.metrics.cpu}%`;
                }
                if (data.metrics.memory) {
                    document.getElementById('mem-usage').textContent = `${data.metrics.memory}%`;
                }
                if (data.metrics.disk) {
                    document.getElementById('disk-usage').textContent = `${data.metrics.disk}%`;
                }
                if (data.metrics.response_time) {
                    document.getElementById('response-time').textContent = `${data.metrics.response_time}ms`;
                }
                if (data.metrics.total_requests) {
                    document.getElementById('total-requests').textContent = data.metrics.total_requests;
                }
            }

            // Update memory statistics
            if (data.memory_stats) {
                document.getElementById('total-memories').textContent = data.memory_stats.total || 0;
                document.getElementById('associations').textContent = data.memory_stats.associations || 0;
                document.getElementById('db-size').textContent = `${data.memory_stats.db_size || 0} MB`;
                if (data.memory_stats.last_backup) {
                    document.getElementById('last-backup').textContent =
                        new Date(data.memory_stats.last_backup).toLocaleString();
                }
            }

            // Update logs
            if (data.logs && data.logs.length > 0) {
                const logsContainer = document.getElementById('logs');
                const newLogs = data.logs.map(log =>
                    `<div class="log-entry ${log.level}">[${log.time}] ${log.message}</div>`
                ).join('');
                logsContainer.innerHTML = newLogs + logsContainer.innerHTML;

                // Keep only last 50 logs
                while (logsContainer.children.length > 50) {
                    logsContainer.removeChild(logsContainer.lastChild);
                }
            }
        }

        async function createBackup() {
            const resultEl = document.getElementById('action-result');
            resultEl.textContent = 'Creating backup...';

            try {
                const response = await fetch('/api/v1/backup', { method: 'POST' });
                const result = await response.json();
                resultEl.textContent = result.success ? 'Backup created successfully' : 'Backup failed';
            } catch (error) {
                resultEl.textContent = 'Error creating backup';
            }
        }

        async function runHealthCheck() {
            const resultEl = document.getElementById('action-result');
            resultEl.textContent = 'Running health check...';

            try {
                const response = await fetch('/health');
                const result = await response.json();
                resultEl.textContent = `Health: ${result.status}`;
            } catch (error) {
                resultEl.textContent = 'Health check failed';
            }
        }

        function clearLogs() {
            document.getElementById('logs').innerHTML = '';
            document.getElementById('action-result').textContent = 'Logs cleared';
        }

        // Initialize
        connectWebSocket();

        // Periodic health check
        setInterval(async () => {
            try {
                const response = await fetch('/api/dashboard/stats');
                const data = await response.json();
                updateDashboard(data);
            } catch (error) {
                console.error('Failed to fetch stats:', error);
            }
        }, 5000);
    </script>
</body>
</html>
"""

# Create FastAPI app
app = FastAPI(title="MFN Dashboard")

# WebSocket connections
connected_clients = set()

@app.get("/", response_class=HTMLResponse)
async def dashboard():
    """Serve dashboard HTML"""
    return DASHBOARD_HTML

@app.websocket("/ws")
async def websocket_endpoint(websocket: WebSocket):
    """WebSocket endpoint for real-time updates"""
    await websocket.accept()
    connected_clients.add(websocket)

    try:
        while True:
            # Send updates every 2 seconds
            await asyncio.sleep(2)

            # Gather system stats
            stats = await gather_system_stats()

            # Send to client
            await websocket.send_json(stats)

    except WebSocketDisconnect:
        connected_clients.remove(websocket)
    except Exception as e:
        logger.error(f"WebSocket error: {e}")
        if websocket in connected_clients:
            connected_clients.remove(websocket)

async def gather_system_stats() -> Dict[str, Any]:
    """Gather current system statistics"""
    try:
        import psutil

        # Get system metrics
        cpu_percent = psutil.cpu_percent(interval=1)
        memory = psutil.virtual_memory()
        disk = psutil.disk_usage('/app/data')

        # Get process info
        layers = {}
        for i in range(1, 5):
            socket_path = f"/app/sockets/layer{i}.sock"
            layers[f"layer{i}"] = {
                "active": os.path.exists(socket_path),
                "memory_count": 0  # Would query actual count
            }

        # Get logs (last 10 entries)
        logs = []
        log_file = "/app/logs/orchestrator.log"
        if os.path.exists(log_file):
            with open(log_file, 'r') as f:
                lines = f.readlines()[-10:]
                for line in lines:
                    if "ERROR" in line:
                        level = "error"
                    elif "WARNING" in line:
                        level = "warning"
                    else:
                        level = "info"

                    logs.append({
                        "time": datetime.now().strftime("%H:%M:%S"),
                        "level": level,
                        "message": line.strip()[:100]
                    })

        return {
            "system_status": "healthy" if all(l["active"] for l in layers.values()) else "degraded",
            "uptime": time.time() - psutil.boot_time(),
            "layers": layers,
            "metrics": {
                "cpu": round(cpu_percent, 1),
                "memory": round(memory.percent, 1),
                "disk": round(disk.percent, 1),
                "response_time": None,  # Requires real metrics collection
                "total_requests": None  # Requires real metrics collection
            },
            "memory_stats": {
                "total": None,  # Requires real metrics collection
                "associations": None  # Requires real metrics collection
                "db_size": round(disk.used / (1024 * 1024), 1),
                "last_backup": datetime.now().isoformat()
            },
            "logs": logs
        }

    except Exception as e:
        logger.error(f"Failed to gather stats: {e}")
        return {}

@app.get("/api/dashboard/stats")
async def get_dashboard_stats():
    """API endpoint for dashboard statistics"""
    return await gather_system_stats()

if __name__ == "__main__":
    port = int(os.environ.get("MFN_DASHBOARD_PORT", 3000))

    uvicorn.run(
        app,
        host="0.0.0.0",
        port=port,
        log_level="info"
    )