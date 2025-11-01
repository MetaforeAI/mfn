# MFN System Makefile
# Production deployment and management commands

.PHONY: help build run stop clean test deploy monitor backup restore logs

# Default target
help:
	@echo "MFN System Management Commands"
	@echo "=============================="
	@echo "  make build       - Build Docker container"
	@echo "  make run         - Run MFN system"
	@echo "  make stop        - Stop MFN system"
	@echo "  make clean       - Clean up containers and volumes"
	@echo "  make test        - Run system tests"
	@echo "  make deploy      - Full production deployment"
	@echo "  make monitor     - Open monitoring dashboard"
	@echo "  make backup      - Create system backup"
	@echo "  make restore     - Restore from backup"
	@echo "  make logs        - View system logs"
	@echo "  make health      - Run health check"
	@echo "  make shell       - Access container shell"

# Build Docker container
build:
	@echo "Building MFN container..."
	docker-compose build --no-cache

# Run MFN system
run:
	@echo "Starting MFN system..."
	docker-compose up -d
	@sleep 5
	@make health

# Stop MFN system
stop:
	@echo "Stopping MFN system..."
	docker-compose down

# Clean up everything
clean:
	@echo "Cleaning up containers and volumes..."
	docker-compose down -v
	rm -rf data/* logs/* backups/*
	@echo "Cleanup complete"

# Run tests
test:
	@echo "Running system tests..."
	docker-compose run --rm mfn-system python3 /app/lib/test_system.py

# Full production deployment
deploy:
	@echo "Deploying MFN system to production..."
	@make build
	@make run
	@echo "Deployment complete!"
	@echo "Dashboard: http://localhost:3000"
	@echo "API: http://localhost:8080"
	@echo "Metrics: http://localhost:9090"

# Open monitoring dashboard
monitor:
	@echo "Opening monitoring dashboard..."
	@open http://localhost:3000 || xdg-open http://localhost:3000 || echo "Open http://localhost:3000 in your browser"

# Create backup
backup:
	@echo "Creating system backup..."
	@docker exec mfn-production /app/venv/bin/python -c "\
		from add_persistence import MFNPersistenceManager; \
		pm = MFNPersistenceManager('/app/data'); \
		backup_dir = pm.create_backup(); \
		print(f'Backup created: {backup_dir}')"

# Restore from backup (requires BACKUP_NAME variable)
restore:
	@if [ -z "$(BACKUP_NAME)" ]; then \
		echo "Error: BACKUP_NAME not specified"; \
		echo "Usage: make restore BACKUP_NAME=auto_backup_20240924_120000"; \
		exit 1; \
	fi
	@echo "Restoring from backup: $(BACKUP_NAME)..."
	@docker exec mfn-production /app/venv/bin/python -c "\
		from add_persistence import MFNPersistenceManager; \
		pm = MFNPersistenceManager('/app/data'); \
		success = pm.restore_from_backup('/app/backups/$(BACKUP_NAME)'); \
		print('Restore successful' if success else 'Restore failed')"

# View logs
logs:
	@echo "Viewing system logs (Ctrl+C to exit)..."
	docker-compose logs -f --tail=100

# Health check
health:
	@echo "Running health check..."
	@docker exec mfn-production /app/scripts/health_check.sh || true

# Access container shell
shell:
	@echo "Accessing container shell..."
	docker exec -it mfn-production /bin/bash

# Development targets
dev-build:
	@echo "Building development container..."
	docker build --target development -t mfn-system:dev .

dev-run:
	@echo "Running in development mode..."
	docker run -it --rm \
		-v $(PWD):/app \
		-p 8080:8080 \
		-p 3000:3000 \
		--name mfn-dev \
		mfn-system:dev

# Performance testing
perf-test:
	@echo "Running performance tests..."
	docker exec mfn-production python3 /app/scripts/performance_test.py

# Security scan
security-scan:
	@echo "Running security scan..."
	docker scan mfn-system:latest || trivy image mfn-system:latest

# Database operations
db-stats:
	@docker exec mfn-production sqlite3 /app/data/mfn_memories.db \
		"SELECT 'Memories:', COUNT(*) FROM memories; \
		 SELECT 'Associations:', COUNT(*) FROM layer3_associations;"

db-optimize:
	@echo "Optimizing database..."
	@docker exec mfn-production sqlite3 /app/data/mfn_memories.db "VACUUM; ANALYZE;"

# Monitoring operations
start-monitoring:
	@echo "Starting monitoring stack..."
	docker-compose up -d prometheus grafana

stop-monitoring:
	@echo "Stopping monitoring stack..."
	docker-compose stop prometheus grafana

# Version info
version:
	@echo "MFN System Version Information"
	@echo "=============================="
	@docker exec mfn-production cat /app/config/version.txt 2>/dev/null || echo "Version: 1.0.0"
	@docker images mfn-system:latest --format "Image: {{.Repository}}:{{.Tag}} ({{.Size}})"