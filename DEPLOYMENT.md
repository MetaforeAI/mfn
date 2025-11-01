# MFN System Production Deployment Guide

## Overview
The MFN (Memory Flow Network) system is deployed as a single, self-contained Docker container with all four layers, orchestration, persistence, and monitoring built-in.

## Architecture

```
┌─────────────────────────────────────────────────┐
│              MFN Container                       │
├─────────────────────────────────────────────────┤
│  ┌──────────────────────────────────────────┐  │
│  │         Supervisor Process Manager        │  │
│  └──────────────────────────────────────────┘  │
│                      │                          │
│  ┌──────────────────┴────────────────────┐     │
│  │                                        │     │
│  │   Layer 1    Layer 2    Layer 3   Layer 4   │
│  │   (Zig IFR)  (Rust DSR) (Go ALM) (Rust CPE) │
│  │      ↓          ↓          ↓         ↓      │
│  │   Unix       Unix       Unix      Unix      │
│  │   Socket     Socket     Socket    Socket    │
│  │      ↓          ↓          ↓         ↓      │
│  └──────────────────┬────────────────────┘     │
│                     │                           │
│  ┌──────────────────┴────────────────────┐     │
│  │         MFN Orchestrator               │     │
│  │   (Circuit Breakers + Retry Logic)     │     │
│  └──────────────────┬────────────────────┘     │
│                     │                           │
│  ┌──────────────────┴────────────────────┐     │
│  │      API Gateway    Dashboard    Metrics    │
│  │     (Port 8080)    (Port 3000)  (Port 9090) │
│  └──────────────────────────────────────────┘  │
│                                                 │
│  ┌─────────────────────────────────────────┐   │
│  │  Persistence Layer (SQLite + Backups)   │   │
│  └─────────────────────────────────────────┘   │
└─────────────────────────────────────────────────┘
```

## Prerequisites

- Docker 20.10+ and Docker Compose 2.0+
- 8GB RAM minimum (16GB recommended)
- 20GB disk space
- Linux kernel 5.10+ (for optimal performance)

## Quick Start

### 1. Single Command Deployment

```bash
# Clone repository
git clone https://github.com/your-org/telepathy.git
cd telepathy

# Build and run
docker-compose up -d

# Verify deployment
docker-compose ps
docker-compose logs -f
```

### 2. Direct Docker Run (Without Compose)

```bash
# Build container
docker build -t mfn-system:latest .

# Run with persistence
docker run -d \
  --name mfn-production \
  -p 8080:8080 \
  -p 3000:3000 \
  -p 9090:9090 \
  -v $(pwd)/data:/app/data \
  -v $(pwd)/logs:/app/logs \
  -v $(pwd)/backups:/app/backups \
  --restart unless-stopped \
  mfn-system:latest
```

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `MFN_ENV` | production | Environment mode |
| `MFN_LOG_LEVEL` | info | Logging level (debug/info/warn/error) |
| `MFN_API_PORT` | 8080 | API Gateway port |
| `MFN_DASHBOARD_PORT` | 3000 | Dashboard UI port |
| `MFN_DATA_DIR` | /app/data | Data persistence directory |
| `MFN_BACKUP_DIR` | /app/backups | Backup storage directory |

### Volume Mounts

| Mount Point | Purpose | Persistence |
|-------------|---------|-------------|
| `/app/data` | SQLite DB & layer states | Required |
| `/app/logs` | Application logs | Recommended |
| `/app/backups` | System backups | Recommended |
| `/app/config` | Custom configuration | Optional |

## Health Monitoring

### Built-in Health Checks

```bash
# Container health status
docker inspect mfn-production --format='{{.State.Health.Status}}'

# Detailed health check
docker exec mfn-production /app/scripts/health_check.sh

# API health endpoint
curl http://localhost:8080/health

# Metrics endpoint (Prometheus format)
curl http://localhost:9090/metrics
```

### Dashboard Access

Open browser to `http://localhost:3000` for real-time monitoring dashboard.

## Service Management

### Starting Services

```bash
# Start all services
docker-compose up -d

# Start specific service
docker exec mfn-production supervisorctl start layer1_ifr
```

### Stopping Services

```bash
# Graceful shutdown
docker-compose down

# Stop specific layer
docker exec mfn-production supervisorctl stop layer2_dsr
```

### Restarting Services

```bash
# Restart all layers
docker exec mfn-production supervisorctl restart mfn_layers:*

# Restart specific service
docker exec mfn-production supervisorctl restart mfn_api
```

## Persistence & Backup

### Automatic Persistence

- Memory data automatically saved to SQLite
- Layer states checkpointed every 5 minutes
- Full backups every 6 hours
- 7-day retention policy

### Manual Backup

```bash
# Create backup via API
curl -X POST http://localhost:8080/api/v1/backup

# Create backup via script
docker exec mfn-production python3 /app/scripts/create_backup.py

# Backup to external location
docker cp mfn-production:/app/backups ./external-backups
```

### Restore from Backup

```bash
# List available backups
ls -la ./backups/

# Restore specific backup
docker exec mfn-production python3 /app/scripts/restore_backup.py \
  --backup-name auto_backup_20240924_120000
```

## Performance Tuning

### Resource Allocation

```yaml
# docker-compose.yml
deploy:
  resources:
    limits:
      cpus: '8.0'      # Increase for better parallelism
      memory: 16G      # Increase for larger datasets
```

### Socket Optimization

```bash
# Increase socket buffer sizes
echo 'net.core.rmem_max=134217728' >> /etc/sysctl.conf
echo 'net.core.wmem_max=134217728' >> /etc/sysctl.conf
sysctl -p
```

### Layer Tuning

```bash
# Adjust layer-specific parameters
docker exec mfn-production bash -c "
  export LAYER1_HASH_SIZE=1048576
  export LAYER2_RESERVOIR_SIZE=10000
  export LAYER3_ASSOCIATION_LIMIT=1000
  export LAYER4_CONTEXT_WINDOW=100
  supervisorctl restart mfn_layers:*
"
```

## Troubleshooting

### Common Issues

#### 1. Layers Not Responding

```bash
# Check layer status
docker exec mfn-production supervisorctl status

# Check socket files
docker exec mfn-production ls -la /app/sockets/

# Restart failed layer
docker exec mfn-production supervisorctl restart layer1_ifr
```

#### 2. High Memory Usage

```bash
# Check memory consumption
docker stats mfn-production

# Clear cache
docker exec mfn-production python3 -c "
from add_persistence import MFNPersistenceManager
pm = MFNPersistenceManager()
pm._optimize_database()
"
```

#### 3. API Gateway Issues

```bash
# Check API logs
docker exec mfn-production tail -f /app/logs/api.log

# Test API directly
docker exec mfn-production curl http://localhost:8080/health

# Restart API gateway
docker exec mfn-production supervisorctl restart mfn_api
```

### Debug Mode

```bash
# Enable debug logging
docker run -d \
  --name mfn-debug \
  -e MFN_LOG_LEVEL=debug \
  -e RUST_LOG=debug \
  -p 8080:8080 \
  mfn-system:latest

# Access container shell
docker exec -it mfn-debug /bin/bash

# View real-time logs
docker logs -f mfn-debug
```

## Security Hardening

### Network Security

```bash
# Create isolated network
docker network create --driver bridge \
  --subnet=172.28.0.0/16 \
  --ip-range=172.28.5.0/24 \
  mfn-secure

# Run with network isolation
docker run -d \
  --name mfn-production \
  --network mfn-secure \
  --cap-drop ALL \
  --cap-add NET_BIND_SERVICE \
  --security-opt no-new-privileges:true \
  mfn-system:latest
```

### Access Control

```nginx
# nginx.conf for reverse proxy
upstream mfn_api {
    server localhost:8080;
}

server {
    listen 443 ssl http2;
    server_name mfn.example.com;

    ssl_certificate /etc/ssl/certs/mfn.crt;
    ssl_certificate_key /etc/ssl/private/mfn.key;

    location /api/ {
        proxy_pass http://mfn_api;
        proxy_set_header X-Real-IP $remote_addr;

        # Rate limiting
        limit_req zone=api burst=20 nodelay;
    }

    location /dashboard/ {
        auth_basic "MFN Dashboard";
        auth_basic_user_file /etc/nginx/.htpasswd;
        proxy_pass http://localhost:3000;
    }
}
```

## Monitoring Integration

### Prometheus Configuration

```yaml
# docker/monitoring/prometheus.yml
global:
  scrape_interval: 15s

scrape_configs:
  - job_name: 'mfn-system'
    static_configs:
      - targets: ['mfn-system:9090']
    metrics_path: '/metrics'
```

### Grafana Dashboard

Import the included dashboard from `docker/monitoring/grafana/dashboards/mfn-dashboard.json`

## Production Checklist

- [ ] Configure environment variables
- [ ] Set up persistent volumes
- [ ] Configure backup schedule
- [ ] Set resource limits
- [ ] Enable health checks
- [ ] Configure monitoring
- [ ] Set up log rotation
- [ ] Implement access control
- [ ] Test disaster recovery
- [ ] Document custom configurations

## Support

For issues, monitoring, and updates:
- Dashboard: http://localhost:3000
- API Docs: http://localhost:8080/docs
- Health Check: http://localhost:8080/health
- Metrics: http://localhost:9090/metrics

## License

Copyright (c) 2024 - All Rights Reserved