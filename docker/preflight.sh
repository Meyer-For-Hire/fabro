#!/usr/bin/env bash
set -u

errors=0
required_docker_version="${REQUIRED_DOCKER_VERSION:-24.0.0}"
required_cores="${REQUIRED_CORES:-2}"
required_ram_mb="${REQUIRED_RAM_MB:-4096}"
required_disk_gb="${REQUIRED_DISK_GB:-20}"
fabro_port="${FABRO_PORT:-32276}"

fail() {
  printf 'FAIL: %s\n' "$1"
  errors=$((errors + 1))
}

ok() {
  printf 'OK: %s\n' "$1"
}

version_ge() {
  local minimum="$1"
  local actual="$2"

  if command -v sort >/dev/null 2>&1 && sort -V </dev/null >/dev/null 2>&1; then
    printf '%s\n%s\n' "$minimum" "$actual" | sort -V -C
  else
    [ "$minimum" = "$actual" ] || [ "$(printf '%s\n%s\n' "$minimum" "$actual" | sort | head -n1)" = "$minimum" ]
  fi
}

if ! command -v docker >/dev/null 2>&1; then
  fail "Docker is not installed."
else
  docker_version="$(docker version --format '{{.Client.Version}}' 2>/dev/null || printf '0.0.0')"
  if version_ge "$required_docker_version" "$docker_version"; then
    ok "Docker version $docker_version"
  else
    fail "Docker version $docker_version is below required minimum $required_docker_version."
  fi

  if docker compose version >/dev/null 2>&1; then
    ok "$(docker compose version)"
  else
    fail "Docker Compose v2 is not available. Install the Docker Compose plugin."
  fi

  if docker ps >/dev/null 2>&1; then
    ok "Docker daemon is accessible"
  else
    fail "Current user cannot access the Docker daemon."
  fi
fi

available_cores="$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || printf '0')"
if [ "$available_cores" -lt "$required_cores" ]; then
  fail "Only $available_cores CPU core(s) available; need at least $required_cores."
else
  ok "$available_cores CPU core(s)"
fi

if [ -f /proc/meminfo ]; then
  total_ram_mb="$(awk '/MemTotal/ { print int($2 / 1024) }' /proc/meminfo)"
elif command -v sysctl >/dev/null 2>&1; then
  total_ram_bytes="$(sysctl -n hw.memsize 2>/dev/null || printf '0')"
  total_ram_mb=$((total_ram_bytes / 1024 / 1024))
else
  total_ram_mb=0
fi

if [ "$total_ram_mb" -lt "$required_ram_mb" ]; then
  fail "Only ${total_ram_mb}MB RAM available; need at least ${required_ram_mb}MB."
else
  ok "${total_ram_mb}MB RAM"
fi

available_kb="$(df -Pk /var/lib/docker 2>/dev/null | awk 'NR == 2 { print $4 }')"
if [ -z "$available_kb" ]; then
  available_kb="$(df -Pk / 2>/dev/null | awk 'NR == 2 { print $4 }')"
fi
available_gb=$((available_kb / 1024 / 1024))

if [ "$available_gb" -lt "$required_disk_gb" ]; then
  fail "Only ${available_gb}GB free disk space; need at least ${required_disk_gb}GB."
else
  ok "${available_gb}GB free disk space"
fi

if command -v ss >/dev/null 2>&1; then
  if ss -ltn | awk '{ print $4 }' | grep -Eq "[:.]${fabro_port}$"; then
    fail "Port ${fabro_port} is already in use."
  else
    ok "Port ${fabro_port} appears available"
  fi
elif command -v lsof >/dev/null 2>&1; then
  if lsof -nP -iTCP:"$fabro_port" -sTCP:LISTEN >/dev/null 2>&1; then
    fail "Port ${fabro_port} is already in use."
  else
    ok "Port ${fabro_port} appears available"
  fi
else
  ok "Skipped port availability check; neither ss nor lsof is installed"
fi

if command -v curl >/dev/null 2>&1; then
  ghcr_status="$(curl -sS -o /dev/null -w '%{http_code}' --max-time 10 https://ghcr.io/v2/ 2>/dev/null || printf '000')"
  if [ "$ghcr_status" != "000" ]; then
    ok "ghcr.io is reachable"
  else
    fail "Cannot reach ghcr.io for pulling Fabro images."
  fi
else
  fail "curl is not installed."
fi

printf '\n'
if [ "$errors" -gt 0 ]; then
  printf 'Pre-flight checks failed with %s error(s). Please fix them before deploying Fabro.\n' "$errors"
  exit 1
fi

printf 'All Fabro pre-flight checks passed.\n'
