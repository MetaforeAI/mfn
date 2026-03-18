# MFN System - Quick Start Guide

**5-Minute Alpha Deployment (Testing/Staging Only)**

> ⚠️ **ALPHA SOFTWARE NOTICE**
> This system is in alpha testing phase (~95% complete). NOT recommended for production use yet.
> Missing: Production monitoring, health check endpoints, circuit breakers.
> See [KNOWN_LIMITATIONS.md](KNOWN_LIMITATIONS.md) for details.

## Prerequisites Check
```bash
# Verify Docker installed
docker --version  # Required: 20.10+
docker-compose --version  # Required: 2.0+

# Verify system resources
free -h  # Required: 8GB+ RAM
df -h    # Required: 20GB+ disk space
```

---

## Fastest Deployment (Single Command)

```bash
# Clone and deploy in one command
git clone https://github.com/NeoTecDigital/telepathy.git && \
cd telepathy && \
make deploy
```

**That's it!** System will:
1. Build Docker container (~10 min first time)
2. Start all services
3. Run health checks
4. Display access URLs

---

## Access Your Deployment

Once deployed, access:

- **Dashboard:** http://localhost:3000
- **API Gateway:** http://localhost:8080
- **Health Check:** http://localhost:8080/health
- **Metrics:** http://localhost:9090/metrics

---

## Verify Deployment

```bash
# Quick health check
make health

# View logs
make logs

# Check all services
docker-compose ps
```

**Expected output:**
```
✓ Layer 1 (IFR) socket exists
✓ Layer 2 (DSR) socket exists
✓ Layer 3 (ALM) socket exists
✓ Layer 4 (CPE) socket exists
✓ API Gateway HTTP endpoint healthy
✓ Dashboard HTTP endpoint healthy
SYSTEM HEALTHY
```

---

## Test the System

### Store a Memory
```bash
curl -X POST http://localhost:8080/api/v1/memories \
  -H "Content-Type: application/json" \
  -d '{
    "content": "My first memory in MFN",
    "tags": ["test", "quickstart"]
  }'
```

### Search Memories
```bash
curl -X POST http://localhost:8080/api/v1/search \
  -H "Content-Type: application/json" \
  -d '{
    "query": "first memory",
    "limit": 10
  }'
```

### Get System Stats
```bash
curl http://localhost:8080/api/v1/stats
```

---

## Common Commands

### Start/Stop
```bash
make run     # Start MFN system
make stop    # Stop MFN system
make restart # Restart MFN system
```

### Monitoring
```bash
make monitor # Open monitoring dashboard
make logs    # View system logs
make health  # Run health check
```

### Maintenance
```bash
make backup  # Create system backup
make clean   # Clean up everything
```

### Access Container
```bash
make shell   # Access container shell
```

---

## Manual Deployment Steps

If you prefer step-by-step control:

### Step 1: Build
```bash
# Build the production container
docker-compose build --no-cache
```

### Step 2: Start
```bash
# Start all services
docker-compose up -d
```

### Step 3: Verify
```bash
# Wait for startup (30-60 seconds)
sleep 60

# Check health
docker exec mfn-production /app/scripts/health_check.sh
```

### Step 4: Monitor
```bash
# Follow logs
docker-compose logs -f
```

---

## Native Development Mode

Run layers natively without Docker:

```bash
# Terminal 1: Start all layers
./scripts/start_all_layers.sh

# Terminal 2: Run tests
cargo test --release --all

# Terminal 3: Monitor logs
tail -f /tmp/layer*.log

# Stop all layers
pkill -f 'mfn_layer|layer[1-4]_'
```

---

## Troubleshooting

### Container won't start
```bash
# Check Docker daemon
sudo systemctl status docker

# Check logs
docker logs mfn-production

# Rebuild from scratch
make clean && make build
```

### Layers not responding
```bash
# Check layer status
docker exec mfn-production supervisorctl status

# Restart failed layer
docker exec mfn-production supervisorctl restart layer1_ifr
```

### High memory usage
```bash
# Check resource usage
docker stats mfn-production

# Optimize database
docker exec mfn-production sqlite3 /app/data/mfn_memories.db "VACUUM; ANALYZE;"
```

### Port conflicts
```bash
# Check what's using ports
sudo netstat -tulpn | grep -E '8080|3000|9090'

# Use different ports
docker run -d \
  -p 8888:8080 \
  -p 3333:3000 \
  -p 9999:9090 \
  mfn-system:latest
```

---

## Performance Tuning

### For High Performance
```yaml
# Edit docker-compose.yml
deploy:
  resources:
    limits:
      cpus: '8.0'
      memory: 16G
```

### For Low Resource Systems
```yaml
deploy:
  resources:
    limits:
      cpus: '2.0'
      memory: 4G
```

---

## Staging/Testing Deployment

### Staging First (Recommended)
```bash
# Deploy to staging
docker-compose -f docker-compose.staging.yml up -d

# Monitor for 24 hours
watch -n 60 'docker exec mfn-staging /app/scripts/health_check.sh'

# Promote to production
docker-compose -f docker-compose.prod.yml up -d
```

### Direct to Production
```bash
# Build production image
docker build --target production -t mfn-system:v1.0.0 .

# Run with production config
docker run -d \
  --name mfn-production \
  -p 8080:8080 \
  -p 3000:3000 \
  -p 9090:9090 \
  -v /data/mfn:/app/data \
  -v /logs/mfn:/app/logs \
  -v /backups/mfn:/app/backups \
  --restart unless-stopped \
  --memory=8g \
  --cpus=4 \
  mfn-system:v1.0.0

# Verify production deployment
curl http://localhost:8080/health
```

---

## Backup & Restore

### Create Backup
```bash
# Automatic backup (every 6 hours)
# No action needed - runs automatically

# Manual backup
make backup

# Or via API
curl -X POST http://localhost:8080/api/v1/backup
```

### Restore from Backup
```bash
# List backups
ls -lh ./backups/

# Restore specific backup
make restore BACKUP_NAME=auto_backup_20251031_120000

# Or manually
docker exec mfn-production python3 /app/scripts/restore_backup.py \
  --backup-name auto_backup_20251031_120000
```

---

## Scaling

### Horizontal Scaling (Multiple Instances)
```bash
# Run multiple instances with load balancer
docker-compose up -d --scale mfn-system=3
```

### Vertical Scaling (More Resources)
```bash
# Increase resources in docker-compose.yml
# Then restart
docker-compose down && docker-compose up -d
```

---

## Monitoring Integration

### Prometheus
```bash
# Start Prometheus monitoring
make start-monitoring

# Access Prometheus
open http://localhost:9091
```

### Grafana
```bash
# Access Grafana dashboard
open http://localhost:3001

# Default credentials:
# Username: admin
# Password: changeme (set in docker-compose.yml)
```

---

## Security Best Practices

### 1. Change Default Passwords
```bash
# Edit docker-compose.yml
environment:
  - GRAFANA_ADMIN_PASSWORD=your_secure_password
```

### 2. Enable TLS
```bash
# Use reverse proxy (nginx/traefik)
# Example nginx config in DEPLOYMENT.md
```

### 3. Network Isolation
```bash
# Create isolated network
docker network create --driver bridge mfn-secure

# Run with isolation
docker run --network mfn-secure ...
```

### 4. Regular Updates
```bash
# Pull latest version
git pull origin main

# Rebuild and redeploy
make clean && make deploy
```

---

## Uninstall

### Clean Shutdown
```bash
# Stop services
make stop

# Remove containers (keep data)
docker-compose down
```

### Complete Removal
```bash
# Remove everything including data
make clean

# Or manually
docker-compose down -v
rm -rf data/ logs/ backups/
```

---

## Next Steps

1. **Read Full Documentation:** See `DEPLOYMENT.md` for detailed guide
2. **Review Architecture:** See `docs/architecture/`
3. **Run Tests:** `cargo test --release --all`
4. **Monitor Metrics:** Open dashboard at http://localhost:3000
5. **Check Logs:** `make logs` for real-time monitoring

---

## Support

**Health Check:** `make health`
**Logs:** `make logs`
**Status:** `docker-compose ps`
**Stats:** `docker stats mfn-production`

For detailed troubleshooting, see `DEPLOYMENT.md` Section 9.

---

**System Status:** 🟡 Alpha Testing
**Version:** 0.1.0
**Last Updated:** 2025-11-04
