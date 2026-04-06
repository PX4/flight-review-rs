#!/usr/bin/env bash
# staging-setup.sh — Provision a fresh Ubuntu 24.04 EC2 as a Flight Review v2 staging server.
#
# Prerequisites:
#   - Ubuntu 24.04 LTS (ARM64 or x86_64)
#   - Run as root or with sudo
#
# Usage:
#   sudo bash scripts/staging-setup.sh
#
# What this does:
#   1. Installs PostgreSQL 16 and creates the flightreview database
#   2. Installs Docker
#   3. Creates data directories
#   4. Builds the flight-review Docker image from the repo
#   5. Sets up a systemd service to run the app
set -euo pipefail

# ── Configuration ────────────────────────────────────────────────────────────
DB_NAME="flightreview"
DB_USER="flightreview"
DB_PASS="${DB_PASSWORD:-$(openssl rand -base64 16)}"
APP_PORT="${APP_PORT:-8080}"
DATA_DIR="/data"
REPO_DIR="$(cd "$(dirname "$0")/.." && pwd)"

echo "=== Flight Review v2 — Staging Setup ==="
echo "Repo:     $REPO_DIR"
echo "DB:       $DB_NAME (user: $DB_USER)"
echo "Data dir: $DATA_DIR"
echo "App port: $APP_PORT"
echo ""

# ── 1. System packages ──────────────────────────────────────────────────────
echo "[1/5] Installing system packages..."
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq --no-install-recommends \
  ca-certificates curl gnupg lsb-release

# ── 2. PostgreSQL 16 ────────────────────────────────────────────────────────
echo "[2/5] Installing PostgreSQL 16..."
if ! command -v psql &>/dev/null; then
  # Add PostgreSQL APT repository
  curl -fsSL https://www.postgresql.org/media/keys/ACCC4CF8.asc \
    | gpg --dearmor -o /usr/share/keyrings/postgresql-keyring.gpg
  echo "deb [signed-by=/usr/share/keyrings/postgresql-keyring.gpg] \
    https://apt.postgresql.org/pub/repos/apt $(lsb_release -cs)-pgdg main" \
    > /etc/apt/sources.list.d/pgdg.list
  apt-get update -qq
  apt-get install -y -qq postgresql-16
fi

# Start and enable PostgreSQL
systemctl enable --now postgresql

# Create database and user
sudo -u postgres psql -tc "SELECT 1 FROM pg_roles WHERE rolname='$DB_USER'" \
  | grep -q 1 || sudo -u postgres psql -c "CREATE USER $DB_USER WITH PASSWORD '$DB_PASS';"
sudo -u postgres psql -tc "SELECT 1 FROM pg_database WHERE datname='$DB_NAME'" \
  | grep -q 1 || sudo -u postgres psql -c "CREATE DATABASE $DB_NAME OWNER $DB_USER;"
sudo -u postgres psql -c "GRANT ALL PRIVILEGES ON DATABASE $DB_NAME TO $DB_USER;"

echo "  Database ready: postgres://$DB_USER:***@localhost/$DB_NAME"

# ── 3. Docker ───────────────────────────────────────────────────────────────
echo "[3/5] Installing Docker..."
if ! command -v docker &>/dev/null; then
  curl -fsSL https://get.docker.com | sh
  usermod -aG docker ubuntu 2>/dev/null || true
fi
systemctl enable --now docker

# ── 4. Data directories ────────────────────────────────────────────────────
echo "[4/5] Creating data directories..."
mkdir -p "$DATA_DIR/files"
chown -R 1000:1000 "$DATA_DIR"

# ── 5. Build and run ────────────────────────────────────────────────────────
echo "[5/5] Building Docker image..."
cd "$REPO_DIR"
docker build -t flight-review:latest .

# Stop existing container if running
docker rm -f flight-review 2>/dev/null || true

# Create systemd service
DB_URL="postgres://$DB_USER:$DB_PASS@host.docker.internal/$DB_NAME"
cat > /etc/systemd/system/flight-review.service <<EOF
[Unit]
Description=Flight Review v2
After=docker.service postgresql.service
Requires=docker.service

[Service]
Restart=always
RestartSec=5
ExecStartPre=-/usr/bin/docker rm -f flight-review
ExecStart=/usr/bin/docker run --rm --name flight-review \
  --add-host=host.docker.internal:host-gateway \
  -p $APP_PORT:8080 \
  -v $DATA_DIR/files:/data/files \
  flight-review:latest \
  serve \
  --db "$DB_URL" \
  --storage "file:///data/files" \
  --port 8080
ExecStop=/usr/bin/docker stop flight-review

[Install]
WantedBy=multi-user.target
EOF

# Allow Docker container to connect to PostgreSQL on the host
PG_HBA="/etc/postgresql/16/main/pg_hba.conf"
if ! grep -q "host.docker.internal" "$PG_HBA" 2>/dev/null; then
  # Allow connections from Docker bridge network
  echo "host    $DB_NAME    $DB_USER    172.16.0.0/12    scram-sha-256" >> "$PG_HBA"
  # Listen on all interfaces (needed for Docker containers)
  sed -i "s/#listen_addresses = 'localhost'/listen_addresses = '*'/" \
    /etc/postgresql/16/main/postgresql.conf
  systemctl restart postgresql
fi

systemctl daemon-reload
systemctl enable --now flight-review

echo ""
echo "=== Setup complete ==="
echo ""
echo "Database URL: postgres://$DB_USER:$DB_PASS@localhost/$DB_NAME"
echo "App running:  http://$(curl -s ifconfig.me 2>/dev/null || echo '<public-ip>'):$APP_PORT"
echo ""
echo "Useful commands:"
echo "  sudo systemctl status flight-review    # Check app status"
echo "  sudo journalctl -u flight-review -f    # Follow app logs"
echo "  sudo systemctl restart flight-review   # Restart the app"
echo "  docker logs flight-review              # Container logs"
echo ""
echo "IMPORTANT: Save this database password somewhere safe:"
echo "  $DB_PASS"
