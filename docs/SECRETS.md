# MFN Secrets Management Guide

## Overview

This document describes how to securely manage secrets, credentials, and sensitive configuration for the MFN (Memory Flow Network) system.

**CRITICAL**: Never commit secrets to version control. All sensitive values must be stored in environment variables or secure secrets management systems.

## Quick Start

### Local Development Setup

1. **Copy the environment template**:
   ```bash
   cp .env.example .env
   ```

2. **Generate secure secrets**:
   ```bash
   # Grafana admin password
   openssl rand -base64 32

   # JWT secret
   openssl rand -hex 32

   # API key
   uuidgen
   ```

3. **Edit .env with your generated secrets**:
   ```bash
   # Use a secure editor
   vim .env  # or nano, vscode, etc.
   ```

4. **Set proper file permissions**:
   ```bash
   chmod 600 .env
   ```

5. **Start the system**:
   ```bash
   docker-compose up -d
   ```

## Required Environment Variables

### Critical Security Variables

| Variable | Description | Example | Required |
|----------|-------------|---------|----------|
| `GRAFANA_ADMIN_PASSWORD` | Grafana admin password | `<random-32-char-string>` | Yes |
| `JWT_SECRET` | JWT signing secret | `<random-hex-64-char>` | If auth enabled |
| `MFN_DASHBOARD_PASSWORD_HASH` | Dashboard password hash | `<bcrypt-hash>` | If dashboard auth enabled |

### Database Variables

| Variable | Description | Default | Required |
|----------|-------------|---------|----------|
| `DATABASE_URL` | SQLite database path | `sqlite:///app/data/mfn_memories.db` | No |
| `DB_PASSWORD` | PostgreSQL password (future) | - | If using PostgreSQL |

### Optional Service Variables

| Variable | Description | Default | Required |
|----------|-------------|---------|----------|
| `REDIS_URL` | Redis connection URL | `redis://localhost:6379` | No |
| `REDIS_PASSWORD` | Redis authentication | - | If Redis auth enabled |
| `SENTRY_DSN` | Sentry error tracking | - | No |
| `AWS_ACCESS_KEY_ID` | AWS S3 backups | - | If S3 backups enabled |
| `AWS_SECRET_ACCESS_KEY` | AWS S3 backups | - | If S3 backups enabled |

## Secret Generation

### Strong Password Generation

```bash
# Method 1: OpenSSL (recommended)
openssl rand -base64 32

# Method 2: /dev/urandom
head -c 32 /dev/urandom | base64

# Method 3: Python
python3 -c "import secrets; print(secrets.token_urlsafe(32))"
```

### JWT Secret Generation

```bash
# Generate 256-bit hex secret
openssl rand -hex 32
```

### Password Hash Generation (bcrypt)

```bash
# Install bcrypt if needed
pip install bcrypt

# Generate hash
python3 -c "import bcrypt; import getpass; pwd = getpass.getpass('Password: ').encode(); print(bcrypt.hashpw(pwd, bcrypt.gensalt()).decode())"
```

### API Key Generation

```bash
# UUID-based API key
uuidgen

# Or random hex string
openssl rand -hex 32
```

## Environment-Specific Configuration

### Development Environment

Create `.env.development`:

```bash
MFN_ENV=development
MFN_LOG_LEVEL=debug
GRAFANA_ADMIN_PASSWORD=dev_password_change_me
MFN_DASHBOARD_AUTH_ENABLED=false
```

### Staging Environment

Create `.env.staging`:

```bash
MFN_ENV=staging
MFN_LOG_LEVEL=info
GRAFANA_ADMIN_PASSWORD=<strong-random-password>
MFN_DASHBOARD_AUTH_ENABLED=true
MFN_TLS_ENABLED=true
```

### Production Environment

**DO NOT use .env files in production!** Use a proper secrets management system.

## Production Secrets Management

### Recommended Solutions

1. **HashiCorp Vault** (recommended for self-hosted)
   - Centralized secrets storage
   - Dynamic secrets generation
   - Audit logging
   - Access control policies

2. **AWS Secrets Manager** (for AWS deployments)
   - Automatic rotation
   - Fine-grained IAM permissions
   - Encryption at rest

3. **Kubernetes Secrets** (for k8s deployments)
   - Native k8s integration
   - RBAC support
   - Can integrate with external secret stores

4. **Docker Secrets** (for Docker Swarm)
   - Encrypted in transit and at rest
   - Only available to authorized containers

### Production Configuration Example (HashiCorp Vault)

```bash
# Store secrets in Vault
vault kv put secret/mfn/production \
  grafana_password="$(openssl rand -base64 32)" \
  jwt_secret="$(openssl rand -hex 32)" \
  db_password="$(openssl rand -base64 32)"

# Retrieve in application startup
export GRAFANA_ADMIN_PASSWORD=$(vault kv get -field=grafana_password secret/mfn/production)
export JWT_SECRET=$(vault kv get -field=jwt_secret secret/mfn/production)
```

### Production Configuration Example (AWS Secrets Manager)

```bash
# Create secret
aws secretsmanager create-secret \
  --name mfn/production/credentials \
  --secret-string '{
    "grafana_password": "...",
    "jwt_secret": "...",
    "db_password": "..."
  }'

# Application retrieves at runtime using AWS SDK
```

## Security Best Practices

### Secret Storage

1. **Never commit secrets to version control**
   - Use `.gitignore` to exclude all `.env*` files (except `.env.example`)
   - Audit git history for accidentally committed secrets
   - If secrets are committed, rotate them immediately

2. **Use environment variables**
   - Prefer environment variables over config files for secrets
   - Use system environment variables in production (not .env files)

3. **Encrypt secrets at rest**
   - Use encrypted filesystems for secret storage
   - Use secrets management tools with encryption

4. **Limit access**
   - File permissions: `chmod 600 .env`
   - Principle of least privilege for secret access
   - Use separate secrets for different environments

### Secret Rotation

1. **Regular rotation schedule**:
   - Production secrets: every 90 days minimum
   - Development secrets: every 6 months
   - Immediately after employee departure

2. **Rotation procedure**:
   ```bash
   # 1. Generate new secret
   NEW_PASSWORD=$(openssl rand -base64 32)

   # 2. Update secret in secrets manager
   vault kv put secret/mfn/production grafana_password="$NEW_PASSWORD"

   # 3. Update application configuration (rolling update)
   kubectl rollout restart deployment/mfn-system

   # 4. Verify new secret works

   # 5. Revoke old secret
   ```

3. **Emergency rotation**:
   - If secret is compromised, rotate immediately
   - Audit access logs for unauthorized usage
   - Document incident for security review

### Access Control

1. **Development**:
   - Developers have access to development secrets only
   - Use separate credentials from staging/production

2. **Staging**:
   - Limited team access
   - Separate credentials from production

3. **Production**:
   - Strictly controlled access (SRE, security team)
   - All access logged and audited
   - Use service accounts, not personal credentials

### Audit and Monitoring

1. **Secret access logging**:
   - Log all secret retrievals
   - Monitor for unusual access patterns
   - Alert on unauthorized access attempts

2. **Regular security audits**:
   ```bash
   # Check for accidentally committed secrets
   git log -p | grep -i "password\|secret\|api_key" | head -20

   # Scan codebase for hardcoded secrets
   grep -r "password.*=" --include="*.yml" --include="*.yaml" --include="*.json"

   # Check file permissions
   ls -la .env*
   ```

3. **Secret scanning tools**:
   - Use GitHub secret scanning (if using GitHub)
   - Run `git-secrets` or `truffleHog` in CI/CD
   - Use SAST tools to detect hardcoded secrets

## Current Security Status

### Fixed Issues

✅ **Removed hardcoded Grafana password** from `docker-compose.yml`
- Changed from: `GF_SECURITY_ADMIN_PASSWORD=mfn_admin`
- Changed to: `GF_SECURITY_ADMIN_PASSWORD=${GRAFANA_ADMIN_PASSWORD:-changeme}`

✅ **Updated dashboard authentication** in `docker/config/mfn_config.json`
- Now uses environment variables for all auth settings

✅ **Enhanced .gitignore** to prevent accidental commits
- Blocks all `.env*` files except `.env.example`

✅ **Created .env.example** template
- Comprehensive template with all required variables
- Security notes and generation commands

### Remaining Security Tasks

🔄 **No secrets found in git history** (verified)
- Git log scan shows no committed passwords/secrets
- Repository is clean

⚠️ **Production deployment checklist**:
1. [ ] Generate strong production secrets
2. [ ] Set up secrets management system (Vault/AWS Secrets Manager)
3. [ ] Enable TLS/SSL for all services
4. [ ] Enable authentication on all services
5. [ ] Configure automated secret rotation
6. [ ] Set up secret access monitoring
7. [ ] Document secret recovery procedures
8. [ ] Test secret rotation procedures

## Troubleshooting

### Issue: docker-compose fails to start

**Cause**: Missing .env file

**Solution**:
```bash
cp .env.example .env
# Edit .env with your secrets
docker-compose up -d
```

### Issue: Cannot access Grafana

**Cause**: Default password not set

**Solution**:
```bash
# Set password in .env
echo "GRAFANA_ADMIN_PASSWORD=your_secure_password" >> .env

# Restart Grafana
docker-compose restart grafana
```

### Issue: "Permission denied" reading .env

**Cause**: Incorrect file permissions

**Solution**:
```bash
chmod 600 .env
```

## Contact

For security issues or questions:
- Report security vulnerabilities privately (not in public issues)
- Contact the security team before disclosing vulnerabilities
- Follow responsible disclosure practices

## References

- [OWASP Secrets Management Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Secrets_Management_Cheat_Sheet.html)
- [HashiCorp Vault Documentation](https://www.vaultproject.io/docs)
- [AWS Secrets Manager Best Practices](https://docs.aws.amazon.com/secretsmanager/latest/userguide/best-practices.html)
- [12-Factor App: Config](https://12factor.net/config)
