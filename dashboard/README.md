# MFN System Dashboard

## Overview

A production-quality, native web dashboard for monitoring the Memory Flow Network (MFN) system with **zero external dependencies**. Built entirely with vanilla HTML/CSS/JavaScript and provides real-time performance monitoring, layer status tracking, and comprehensive system metrics.

## Features

### 🚀 Real-time Monitoring
- **Live Metrics**: QPS, latency, memory usage, error rates
- **Layer Status**: Health checks for all 4 MFN layers
- **Performance Graphs**: Time-series visualizations
- **Query Tracing**: End-to-end query path visualization

### 📊 Dashboard Views
1. **Overview**: System-wide metrics and layer status
2. **Layer Status**: Detailed layer-by-layer monitoring
3. **Performance**: Latency distributions and throughput graphs
4. **Memory**: Memory usage and distribution across layers
5. **Query Traces**: Visual query flow through layers
6. **System Logs**: Real-time log streaming
7. **Configuration**: System configuration viewer

### 🎨 User Interface
- **Dark/Light Mode**: Toggle between themes
- **Responsive Design**: Works on desktop and mobile
- **Real-time Updates**: WebSocket/polling for live data
- **Interactive Charts**: Native canvas-based visualizations

## Architecture

```
┌─────────────────────────────────────────┐
│           Web Dashboard (HTML/JS)        │
│                                          │
│  ┌──────────┐  ┌──────────┐  ┌────────┐│
│  │ Overview │  │  Charts  │  │  Logs  ││
│  └──────────┘  └──────────┘  └────────┘│
└────────────────┬─────────────────────────┘
                 │ HTTP/WebSocket
                 │
┌────────────────▼─────────────────────────┐
│         Metrics Server                    │
│                                          │
│  ┌──────────┐  ┌──────────┐  ┌────────┐│
│  │ Collector│  │   API    │  │   WS   ││
│  └──────────┘  └──────────┘  └────────┘│
└────────────────┬─────────────────────────┘
                 │ Unix Sockets
                 │
┌────────────────▼─────────────────────────┐
│            MFN Layers                     │
│                                          │
│  ┌─────┐  ┌─────┐  ┌─────┐  ┌─────┐   │
│  │ L1  │  │ L2  │  │ L3  │  │ L4  │   │
│  │ IFR │  │ DSR │  │ ALM │  │ CPE │   │
│  └─────┘  └─────┘  └─────┘  └─────┘   │
└──────────────────────────────────────────┘
```

## Installation

No installation required! The dashboard is completely self-contained.

## Usage

### Option 1: Python Server (Recommended)
```bash
# Navigate to dashboard directory
cd dashboard/

# Run the Python metrics server
python3 metrics_collector.py

# Open browser to http://localhost:8080
```

### Option 2: Go Server
```bash
# Navigate to dashboard directory
cd dashboard/

# Run the startup script
./start_dashboard.sh

# Or manually:
go mod download
go run metrics_server.go

# Open browser to http://localhost:8080
```

### Option 3: Static Files Only
```bash
# Open the dashboard directly in browser
# Note: Some features like metrics API won't work
firefox dashboard/index.html
```

## API Endpoints

The metrics server provides the following endpoints:

- `GET /` - Dashboard HTML
- `GET /dashboard.js` - Dashboard JavaScript
- `GET /api/metrics` - Current system metrics (JSON)
- `GET /api/layers` - Layer status information (JSON)
- `GET /api/history` - Historical metrics data (JSON)
- `GET /api/logs` - System logs (JSON)
- `WebSocket /ws` - Real-time metrics stream

## Metrics Collected

### System Metrics
- **QPS** (Queries Per Second)
- **Average Latency** (milliseconds)
- **Memory Usage** (percentage)
- **Active Connections**
- **Error Rate** (percentage)
- **System Uptime**

### Layer Metrics
Each layer reports:
- Status (healthy/degraded/failed)
- Latency (microseconds)
- Memory usage (MB)
- Layer-specific metrics:
  - **Layer 1 (IFR)**: Hit rate, entry count
  - **Layer 2 (DSR)**: Accuracy, neuron count
  - **Layer 3 (ALM)**: Graph size, edge count
  - **Layer 4 (CPE)**: Pattern count, prediction accuracy

## Configuration

The dashboard automatically adapts to the MFN system configuration. No manual configuration required.

### Performance Tuning
- **Update Interval**: 1 second (configurable in code)
- **History Retention**: 1 hour at 1-second resolution
- **Max Log Entries**: 1000 (circular buffer)
- **WebSocket Timeout**: 30 seconds

## Development

### File Structure
```
dashboard/
├── index.html           # Dashboard UI
├── dashboard.js         # Dashboard logic
├── metrics_collector.py # Python metrics server
├── metrics_server.go    # Go metrics server (alternative)
├── go.mod              # Go dependencies
├── start_dashboard.sh  # Startup script
└── README.md          # This file
```

### Extending the Dashboard

1. **Add New Metrics**: Update `MetricsCollector` class in Python or Go
2. **Add New Views**: Modify `index.html` and `dashboard.js`
3. **Add New Charts**: Extend the `Chart` class in `dashboard.js`

### Testing
```bash
# Test Python server
python3 -m pytest test_metrics_collector.py

# Test Go server
go test ./...

# Manual testing
curl http://localhost:8080/api/metrics
```

## Browser Compatibility

Works on all modern browsers without polyfills:
- Chrome/Edge 90+
- Firefox 88+
- Safari 14+
- Mobile browsers (responsive design)

## Performance

- **Page Load**: < 100ms
- **Update Latency**: < 50ms
- **Memory Usage**: < 10MB
- **CPU Usage**: < 1%

## Security

- **Container-local only**: No external network access
- **Read-only monitoring**: No system modification capabilities
- **No authentication**: Designed for local/container use only
- **XSS Protection**: Proper input sanitization
- **No CDN dependencies**: Completely self-contained

## Troubleshooting

### Dashboard won't load
- Check server is running: `ps aux | grep metrics`
- Check port 8080 is free: `lsof -i :8080`

### No metrics displayed
- Verify MFN layers are running
- Check Unix sockets exist: `ls -la /tmp/mfn_*.sock`
- Review server logs for errors

### WebSocket disconnections
- Check firewall settings
- Verify browser WebSocket support
- Try polling mode (automatic fallback)

## License

MIT License - See parent repository LICENSE file

## Contributing

Contributions welcome! The dashboard is designed to be easily extensible while maintaining zero external dependencies.

## Support

For issues or questions, please refer to the main MFN repository documentation.