// MFN Dashboard - Native JavaScript Implementation
// No external dependencies - pure vanilla JS

class MFNDashboard {
    constructor() {
        this.metrics = {
            qps: [],
            latency: [],
            memory: [],
            errors: [],
            connections: 0,
            uptime: 0
        };

        this.layers = {
            layer1: { name: 'IFR', status: 'healthy', latency: 0.5, memory: 12, entries: 10000, hitRate: 95 },
            layer2: { name: 'DSR', status: 'healthy', latency: 30, memory: 48, neurons: 100000, accuracy: 92 },
            layer3: { name: 'ALM', status: 'healthy', latency: 160, memory: 128, graphSize: 50000, edges: 500000 },
            layer4: { name: 'CPE', status: 'degraded', latency: null, memory: null, patterns: null, accuracy: null }
        };

        this.charts = {};
        this.updateInterval = null;
        this.wsConnection = null;
        this.timeRange = '1h';
        this.startTime = Date.now();

        this.init();
    }

    init() {
        this.setupEventListeners();
        this.initCharts();
        this.startMetricsCollection();
        this.connectWebSocket();
        this.updateUI();
    }

    setupEventListeners() {
        // Log filter
        const logFilter = document.getElementById('logFilter');
        if (logFilter) {
            logFilter.addEventListener('change', (e) => this.filterLogs(e.target.value));
        }
    }

    initCharts() {
        // Initialize performance chart
        const perfCanvas = document.getElementById('performanceChart');
        if (perfCanvas) {
            this.charts.performance = new Chart(perfCanvas, {
                labels: [],
                datasets: [
                    { name: 'QPS', color: '#3b82f6', data: [] },
                    { name: 'Latency', color: '#10b981', data: [], yAxis: 'right' }
                ]
            });
        }

        // Initialize latency distribution chart
        const latencyCanvas = document.getElementById('latencyChart');
        if (latencyCanvas) {
            this.charts.latency = new HistogramChart(latencyCanvas, {
                bins: 20,
                color: '#f59e0b'
            });
        }

        // Initialize throughput chart
        const throughputCanvas = document.getElementById('throughputChart');
        if (throughputCanvas) {
            this.charts.throughput = new Chart(throughputCanvas, {
                labels: [],
                datasets: [
                    { name: 'Throughput', color: '#8b5cf6', data: [] }
                ]
            });
        }

        // Initialize memory chart
        const memoryCanvas = document.getElementById('memoryChart');
        if (memoryCanvas) {
            this.charts.memory = new PieChart(memoryCanvas, {
                segments: [
                    { name: 'Layer 1', value: 12, color: '#3b82f6' },
                    { name: 'Layer 2', value: 48, color: '#10b981' },
                    { name: 'Layer 3', value: 128, color: '#f59e0b' },
                    { name: 'Layer 4', value: 0, color: '#ef4444' }
                ]
            });
        }
    }

    startMetricsCollection() {
        // Simulate metrics collection (replace with actual metrics endpoint)
        this.updateInterval = setInterval(() => {
            this.collectMetrics();
            this.updateUI();
        }, 1000); // Update every second
    }

    collectMetrics() {
        // Simulate metric collection - replace with actual data source
        const currentTime = Date.now();

        // Generate simulated metrics
        const qps = 95 + Math.random() * 10;
        const latency = 8 + Math.random() * 4;
        const memory = 30 + Math.random() * 20;
        const errorRate = Math.random() * 0.5;

        // Store metrics with timestamp
        this.metrics.qps.push({ time: currentTime, value: qps });
        this.metrics.latency.push({ time: currentTime, value: latency });
        this.metrics.memory.push({ time: currentTime, value: memory });
        this.metrics.errors.push({ time: currentTime, value: errorRate });

        // Update connections
        this.metrics.connections = Math.floor(10 + Math.random() * 5);

        // Update uptime
        this.metrics.uptime = currentTime - this.startTime;

        // Trim old data based on time range
        this.trimMetrics();

        // Update layer metrics
        this.updateLayerMetrics();
    }

    trimMetrics() {
        const cutoff = Date.now() - this.getTimeRangeMs();

        ['qps', 'latency', 'memory', 'errors'].forEach(metric => {
            this.metrics[metric] = this.metrics[metric].filter(m => m.time > cutoff);
        });
    }

    getTimeRangeMs() {
        const ranges = {
            '1h': 3600000,
            '6h': 21600000,
            '24h': 86400000,
            '7d': 604800000
        };
        return ranges[this.timeRange] || ranges['1h'];
    }

    updateLayerMetrics() {
        // Simulate layer metric updates
        if (Math.random() > 0.95) {
            this.layers.layer1.hitRate = Math.max(85, Math.min(100, this.layers.layer1.hitRate + (Math.random() - 0.5) * 5));
        }

        if (Math.random() > 0.95) {
            this.layers.layer2.accuracy = Math.max(85, Math.min(95, this.layers.layer2.accuracy + (Math.random() - 0.5) * 3));
        }

        // Occasionally update layer statuses
        if (Math.random() > 0.99) {
            const statuses = ['healthy', 'degraded', 'failed'];
            const layer = Math.floor(Math.random() * 3) + 1;
            if (layer !== 4) { // Keep layer 4 degraded for demo
                this.layers[`layer${layer}`].status = statuses[Math.floor(Math.random() * 2)]; // Mostly healthy or degraded
            }
        }
    }

    connectWebSocket() {
        // WebSocket connection for real-time updates
        // Replace with actual WebSocket endpoint
        try {
            // For demo, we'll simulate WebSocket with polling
            // In production, use: this.wsConnection = new WebSocket('ws://localhost:8080/metrics');
            this.simulateWebSocket();
        } catch (e) {
            console.error('WebSocket connection failed:', e);
            this.addLog('error', 'Failed to establish WebSocket connection');
        }
    }

    simulateWebSocket() {
        // Simulate WebSocket messages
        setInterval(() => {
            if (Math.random() > 0.7) {
                const messages = [
                    { type: 'info', message: 'Query processed successfully' },
                    { type: 'info', message: 'Memory cache updated' },
                    { type: 'warning', message: 'High latency detected in Layer 3' },
                    { type: 'info', message: 'New connection established' },
                    { type: 'error', message: 'Layer 4 connection timeout' }
                ];

                const msg = messages[Math.floor(Math.random() * messages.length)];
                this.addLog(msg.type, msg.message);
            }
        }, 3000);
    }

    updateUI() {
        // Update metrics
        this.updateMetricCards();

        // Update charts
        this.updateCharts();

        // Update layer status
        this.updateLayerStatus();

        // Update connection status
        this.updateConnectionStatus();

        // Update last update time
        document.getElementById('lastUpdate').textContent = `Last update: ${new Date().toLocaleTimeString()}`;
    }

    updateMetricCards() {
        // QPS
        const currentQPS = this.metrics.qps.length > 0 ?
            this.metrics.qps[this.metrics.qps.length - 1].value : 0;
        const qpsEl = document.getElementById('qps');
        if (qpsEl) qpsEl.textContent = currentQPS.toFixed(1);

        // Average Latency
        const currentLatency = this.metrics.latency.length > 0 ?
            this.metrics.latency[this.metrics.latency.length - 1].value : 0;
        const latencyEl = document.getElementById('avgLatency');
        if (latencyEl) latencyEl.textContent = `${currentLatency.toFixed(1)}ms`;

        // Memory Usage
        const currentMemory = this.metrics.memory.length > 0 ?
            this.metrics.memory[this.metrics.memory.length - 1].value : 0;
        const memEl = document.getElementById('memUsage');
        if (memEl) memEl.textContent = `${currentMemory.toFixed(1)}%`;

        // Connections
        const connEl = document.getElementById('connections');
        if (connEl) connEl.textContent = this.metrics.connections;

        // Error Rate
        const currentErrors = this.metrics.errors.length > 0 ?
            this.metrics.errors[this.metrics.errors.length - 1].value : 0;
        const errorEl = document.getElementById('errorRate');
        if (errorEl) errorEl.textContent = `${currentErrors.toFixed(2)}%`;

        // Uptime
        const uptimeEl = document.getElementById('uptime');
        if (uptimeEl) uptimeEl.textContent = this.formatUptime(this.metrics.uptime);

        // Start time
        const startEl = document.getElementById('startTime');
        if (startEl) startEl.textContent = new Date(this.startTime).toLocaleTimeString();
    }

    formatUptime(ms) {
        const hours = Math.floor(ms / 3600000);
        const minutes = Math.floor((ms % 3600000) / 60000);
        return `${hours}h ${minutes}m`;
    }

    updateCharts() {
        // Update performance chart
        if (this.charts.performance) {
            this.charts.performance.update(
                this.metrics.qps.map(m => ({ x: m.time, y: m.value })),
                this.metrics.latency.map(m => ({ x: m.time, y: m.value }))
            );
        }

        // Update latency histogram
        if (this.charts.latency && this.metrics.latency.length > 0) {
            const latencyValues = this.metrics.latency.map(m => m.value);
            this.charts.latency.update(latencyValues);
        }

        // Update throughput chart
        if (this.charts.throughput) {
            this.charts.throughput.update(
                this.metrics.qps.map(m => ({ x: m.time, y: m.value }))
            );
        }

        // Update memory pie chart
        if (this.charts.memory) {
            this.charts.memory.update([
                { name: 'Layer 1', value: this.layers.layer1.memory },
                { name: 'Layer 2', value: this.layers.layer2.memory },
                { name: 'Layer 3', value: this.layers.layer3.memory },
                { name: 'Layer 4', value: this.layers.layer4.memory || 0 }
            ]);
        }
    }

    updateLayerStatus() {
        // Update layer 1
        document.getElementById('layer1Status').textContent = this.layers.layer1.status.toUpperCase();
        document.getElementById('layer1Status').className = `layer-status ${this.layers.layer1.status}`;
        document.getElementById('layer1Latency').textContent = `${this.layers.layer1.latency}μs`;
        document.getElementById('layer1HitRate').textContent = `${this.layers.layer1.hitRate.toFixed(1)}%`;
        document.getElementById('layer1Memory').textContent = `${this.layers.layer1.memory}MB`;
        document.getElementById('layer1Entries').textContent = `${(this.layers.layer1.entries / 1000).toFixed(0)}K`;

        // Update layer 2
        document.getElementById('layer2Status').textContent = this.layers.layer2.status.toUpperCase();
        document.getElementById('layer2Status').className = `layer-status ${this.layers.layer2.status}`;
        document.getElementById('layer2Latency').textContent = `${this.layers.layer2.latency}μs`;
        document.getElementById('layer2Accuracy').textContent = `${this.layers.layer2.accuracy.toFixed(1)}%`;
        document.getElementById('layer2Memory').textContent = `${this.layers.layer2.memory}MB`;
        document.getElementById('layer2Neurons').textContent = `${(this.layers.layer2.neurons / 1000).toFixed(0)}K`;

        // Update layer 3
        document.getElementById('layer3Status').textContent = this.layers.layer3.status.toUpperCase();
        document.getElementById('layer3Status').className = `layer-status ${this.layers.layer3.status}`;
        document.getElementById('layer3Latency').textContent = `${this.layers.layer3.latency}μs`;
        document.getElementById('layer3Memory').textContent = `${this.layers.layer3.memory}MB`;
        document.getElementById('layer3GraphSize').textContent = `${(this.layers.layer3.graphSize / 1000).toFixed(0)}K`;
        document.getElementById('layer3Edges').textContent = `${(this.layers.layer3.edges / 1000).toFixed(0)}K`;

        // Update layer 4
        document.getElementById('layer4Status').textContent = this.layers.layer4.status.toUpperCase();
        document.getElementById('layer4Status').className = `layer-status ${this.layers.layer4.status}`;

        // Update system status indicator
        const allHealthy = Object.values(this.layers).every(l => l.status === 'healthy');
        const anyFailed = Object.values(this.layers).some(l => l.status === 'failed');

        const statusIndicator = document.getElementById('systemStatus');
        if (anyFailed) {
            statusIndicator.className = 'status-indicator offline';
        } else if (!allHealthy) {
            statusIndicator.className = 'status-indicator warning';
        } else {
            statusIndicator.className = 'status-indicator online';
        }
    }

    updateConnectionStatus() {
        const statusEl = document.getElementById('connectionStatus');
        if (statusEl) {
            statusEl.textContent = this.wsConnection ? '🟢 Connected' : '🟡 Polling';
        }
    }

    addLog(level, message) {
        const container = document.getElementById('logsContainer');
        if (!container) return;

        const entry = document.createElement('div');
        entry.className = 'log-entry';
        entry.innerHTML = `
            <span class="log-timestamp">${new Date().toISOString()}</span>
            <span class="log-level ${level}">${level.toUpperCase()}</span>
            <span class="log-message">${message}</span>
        `;

        container.insertBefore(entry, container.firstChild);

        // Keep only last 100 logs
        while (container.children.length > 100) {
            container.removeChild(container.lastChild);
        }
    }

    filterLogs(filter) {
        const entries = document.querySelectorAll('.log-entry');
        entries.forEach(entry => {
            const level = entry.querySelector('.log-level').textContent.toLowerCase();
            if (filter === 'all' || level === filter) {
                entry.style.display = 'flex';
            } else {
                entry.style.display = 'none';
            }
        });
    }
}

// Simple Chart Implementation (no external dependencies)
class Chart {
    constructor(canvas, config) {
        this.canvas = canvas;
        this.ctx = canvas.getContext('2d');
        this.config = config;
        this.data = config.datasets.map(() => []);

        this.setupCanvas();
    }

    setupCanvas() {
        // Set canvas size
        const rect = this.canvas.getBoundingClientRect();
        this.canvas.width = rect.width * window.devicePixelRatio;
        this.canvas.height = rect.height * window.devicePixelRatio;
        this.ctx.scale(window.devicePixelRatio, window.devicePixelRatio);

        this.width = rect.width;
        this.height = rect.height;
    }

    update(...datasets) {
        this.data = datasets;
        this.draw();
    }

    draw() {
        const ctx = this.ctx;
        const width = this.width;
        const height = this.height;

        // Clear canvas
        ctx.clearRect(0, 0, width, height);

        // Draw grid
        this.drawGrid();

        // Draw datasets
        this.config.datasets.forEach((dataset, index) => {
            if (this.data[index] && this.data[index].length > 0) {
                this.drawDataset(this.data[index], dataset.color);
            }
        });

        // Draw legend
        this.drawLegend();
    }

    drawGrid() {
        const ctx = this.ctx;
        ctx.strokeStyle = 'rgba(255, 255, 255, 0.1)';
        ctx.lineWidth = 1;

        // Horizontal lines
        for (let i = 0; i <= 5; i++) {
            const y = (this.height - 40) * (i / 5) + 20;
            ctx.beginPath();
            ctx.moveTo(40, y);
            ctx.lineTo(this.width - 20, y);
            ctx.stroke();
        }

        // Vertical lines
        for (let i = 0; i <= 10; i++) {
            const x = (this.width - 60) * (i / 10) + 40;
            ctx.beginPath();
            ctx.moveTo(x, 20);
            ctx.lineTo(x, this.height - 20);
            ctx.stroke();
        }
    }

    drawDataset(data, color) {
        if (data.length === 0) return;

        const ctx = this.ctx;
        const width = this.width - 60;
        const height = this.height - 40;

        // Find min/max for scaling
        const values = data.map(d => d.y);
        const minY = Math.min(...values);
        const maxY = Math.max(...values);
        const range = maxY - minY || 1;

        // Draw line
        ctx.strokeStyle = color;
        ctx.lineWidth = 2;
        ctx.beginPath();

        data.forEach((point, index) => {
            const x = 40 + (index / (data.length - 1)) * width;
            const y = 20 + height - ((point.y - minY) / range) * height;

            if (index === 0) {
                ctx.moveTo(x, y);
            } else {
                ctx.lineTo(x, y);
            }
        });

        ctx.stroke();

        // Draw area under curve
        ctx.fillStyle = color + '30'; // Add transparency
        ctx.lineTo(40 + width, 20 + height);
        ctx.lineTo(40, 20 + height);
        ctx.closePath();
        ctx.fill();
    }

    drawLegend() {
        const ctx = this.ctx;
        let x = this.width - 150;
        let y = 10;

        this.config.datasets.forEach((dataset, index) => {
            ctx.fillStyle = dataset.color;
            ctx.fillRect(x, y + index * 20, 10, 10);

            ctx.fillStyle = getComputedStyle(document.body).getPropertyValue('--text-primary');
            ctx.font = '12px sans-serif';
            ctx.fillText(dataset.name, x + 15, y + index * 20 + 8);
        });
    }
}

// Histogram Chart Implementation
class HistogramChart {
    constructor(canvas, config) {
        this.canvas = canvas;
        this.ctx = canvas.getContext('2d');
        this.config = config;
        this.setupCanvas();
    }

    setupCanvas() {
        const rect = this.canvas.getBoundingClientRect();
        this.canvas.width = rect.width * window.devicePixelRatio;
        this.canvas.height = rect.height * window.devicePixelRatio;
        this.ctx.scale(window.devicePixelRatio, window.devicePixelRatio);

        this.width = rect.width;
        this.height = rect.height;
    }

    update(values) {
        if (values.length === 0) return;

        const bins = this.createBins(values);
        this.draw(bins);
    }

    createBins(values) {
        const min = Math.min(...values);
        const max = Math.max(...values);
        const binCount = this.config.bins;
        const binWidth = (max - min) / binCount;

        const bins = Array(binCount).fill(0).map((_, i) => ({
            start: min + i * binWidth,
            end: min + (i + 1) * binWidth,
            count: 0
        }));

        values.forEach(value => {
            const binIndex = Math.min(Math.floor((value - min) / binWidth), binCount - 1);
            bins[binIndex].count++;
        });

        return bins;
    }

    draw(bins) {
        const ctx = this.ctx;
        const width = this.width - 60;
        const height = this.height - 40;

        ctx.clearRect(0, 0, this.width, this.height);

        const maxCount = Math.max(...bins.map(b => b.count));
        const barWidth = width / bins.length;

        bins.forEach((bin, index) => {
            const barHeight = (bin.count / maxCount) * height;
            const x = 40 + index * barWidth;
            const y = 20 + height - barHeight;

            ctx.fillStyle = this.config.color;
            ctx.fillRect(x + 2, y, barWidth - 4, barHeight);

            // Draw label
            if (index % Math.ceil(bins.length / 10) === 0) {
                ctx.fillStyle = getComputedStyle(document.body).getPropertyValue('--text-secondary');
                ctx.font = '10px sans-serif';
                ctx.fillText(bin.start.toFixed(1), x, this.height - 5);
            }
        });

        // Draw axes
        ctx.strokeStyle = getComputedStyle(document.body).getPropertyValue('--border-color');
        ctx.beginPath();
        ctx.moveTo(40, 20);
        ctx.lineTo(40, 20 + height);
        ctx.lineTo(40 + width, 20 + height);
        ctx.stroke();
    }
}

// Pie Chart Implementation
class PieChart {
    constructor(canvas, config) {
        this.canvas = canvas;
        this.ctx = canvas.getContext('2d');
        this.config = config;
        this.setupCanvas();
    }

    setupCanvas() {
        const rect = this.canvas.getBoundingClientRect();
        this.canvas.width = rect.width * window.devicePixelRatio;
        this.canvas.height = rect.height * window.devicePixelRatio;
        this.ctx.scale(window.devicePixelRatio, window.devicePixelRatio);

        this.width = rect.width;
        this.height = rect.height;
    }

    update(segments) {
        this.draw(segments);
    }

    draw(segments) {
        const ctx = this.ctx;
        const centerX = this.width / 2;
        const centerY = this.height / 2;
        const radius = Math.min(centerX, centerY) - 40;

        ctx.clearRect(0, 0, this.width, this.height);

        const total = segments.reduce((sum, seg) => sum + seg.value, 0);
        let currentAngle = -Math.PI / 2;

        segments.forEach((segment, index) => {
            const angle = (segment.value / total) * 2 * Math.PI;

            // Draw segment
            ctx.beginPath();
            ctx.moveTo(centerX, centerY);
            ctx.arc(centerX, centerY, radius, currentAngle, currentAngle + angle);
            ctx.closePath();

            ctx.fillStyle = segment.color || this.config.segments[index].color;
            ctx.fill();

            // Draw label
            const labelAngle = currentAngle + angle / 2;
            const labelX = centerX + Math.cos(labelAngle) * (radius * 0.7);
            const labelY = centerY + Math.sin(labelAngle) * (radius * 0.7);

            ctx.fillStyle = '#ffffff';
            ctx.font = 'bold 12px sans-serif';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'middle';
            ctx.fillText(segment.name, labelX, labelY);
            ctx.font = '10px sans-serif';
            ctx.fillText(`${segment.value}MB`, labelX, labelY + 15);

            currentAngle += angle;
        });

        // Draw border
        ctx.strokeStyle = getComputedStyle(document.body).getPropertyValue('--border-color');
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.arc(centerX, centerY, radius, 0, 2 * Math.PI);
        ctx.stroke();
    }
}

// Global functions
function toggleTheme() {
    document.body.classList.toggle('light-mode');
}

function showView(viewName) {
    // Hide all views
    document.querySelectorAll('.view').forEach(v => v.style.display = 'none');

    // Show selected view
    const view = document.getElementById(viewName);
    if (view) view.style.display = 'block';

    // Update nav
    document.querySelectorAll('.nav-item').forEach(n => n.classList.remove('active'));
    event.target.classList.add('active');

    // Special handling for different views
    if (viewName === 'layers') {
        showLayersDetail();
    } else if (viewName === 'config') {
        showConfigDetail();
    }
}

function showLayersDetail() {
    const container = document.getElementById('layersDetail');
    container.innerHTML = `
        <div class="layer-grid">
            ${Object.entries(dashboard.layers).map(([key, layer]) => `
                <div class="layer-card">
                    <div class="layer-header">
                        <span class="layer-title">${layer.name}</span>
                        <span class="layer-status ${layer.status}">${layer.status.toUpperCase()}</span>
                    </div>
                    <div class="layer-content">
                        <h4>Performance Metrics</h4>
                        <div class="layer-metrics">
                            <div class="layer-metric">
                                <span class="layer-metric-label">Latency</span>
                                <span class="layer-metric-value">${layer.latency || '--'}μs</span>
                            </div>
                            <div class="layer-metric">
                                <span class="layer-metric-label">Memory</span>
                                <span class="layer-metric-value">${layer.memory || '--'}MB</span>
                            </div>
                        </div>
                        <h4 style="margin-top: 1rem;">Configuration</h4>
                        <pre style="background: var(--bg-tertiary); padding: 0.5rem; border-radius: 4px; font-size: 0.75rem;">
{
  "enabled": ${layer.status !== 'failed'},
  "maxConnections": 100,
  "timeout": 5000,
  "retryPolicy": "exponential"
}
                        </pre>
                    </div>
                </div>
            `).join('')}
        </div>
    `;
}

function showConfigDetail() {
    const container = document.getElementById('configDetail');
    container.innerHTML = `
        <div class="chart-container">
            <h3>System Configuration</h3>
            <pre style="background: var(--bg-tertiary); padding: 1rem; border-radius: 4px;">
{
  "system": {
    "name": "Memory Flow Network",
    "version": "1.0.0",
    "environment": "development"
  },
  "layers": {
    "layer1": {
      "type": "IFR",
      "implementation": "Zig",
      "socket": "/tmp/mfn_layer1.sock",
      "targetLatency": "1μs"
    },
    "layer2": {
      "type": "DSR",
      "implementation": "Rust",
      "socket": "/tmp/mfn_layer2.sock",
      "targetLatency": "50μs"
    },
    "layer3": {
      "type": "ALM",
      "implementation": "Go",
      "socket": "/tmp/mfn_layer3.sock",
      "targetLatency": "10μs"
    },
    "layer4": {
      "type": "CPE",
      "implementation": "Rust",
      "socket": "/tmp/mfn_layer4.sock",
      "targetLatency": "100μs"
    }
  },
  "monitoring": {
    "metricsPort": 9090,
    "dashboardPort": 8080,
    "logLevel": "info",
    "retentionDays": 7
  },
  "performance": {
    "targetQPS": 1000,
    "maxLatency": "20ms",
    "memoryLimit": "1GB",
    "connectionPool": 100
  }
}
            </pre>
        </div>
    `;
}

function setTimeRange(range) {
    dashboard.timeRange = range;

    // Update UI
    document.querySelectorAll('.time-selector').forEach(btn => {
        btn.classList.remove('active');
    });
    event.target.classList.add('active');

    // Refresh data
    dashboard.trimMetrics();
    dashboard.updateUI();
}

// Initialize dashboard on page load
let dashboard;
document.addEventListener('DOMContentLoaded', () => {
    dashboard = new MFNDashboard();

    // Add initial logs
    dashboard.addLog('info', 'Dashboard initialized successfully');
    dashboard.addLog('info', 'Connected to MFN system');
    dashboard.addLog('warning', 'Layer 4 (CPE) is in degraded state');
    dashboard.addLog('info', 'Metrics collection started');
});

// Cleanup on page unload
window.addEventListener('beforeunload', () => {
    if (dashboard) {
        clearInterval(dashboard.updateInterval);
        if (dashboard.wsConnection) {
            dashboard.wsConnection.close();
        }
    }
});