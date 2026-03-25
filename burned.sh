#!/usr/bin/env sh
set -eu

ROOT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PID_FILE="$ROOT_DIR/.burned.pid"
LAUNCHER_PID_FILE="$ROOT_DIR/.burned.launcher.pid"
LOG_FILE="$ROOT_DIR/.burned.log"

read_pid_file() {
  cat "$1"
}

read_pid() {
  read_pid_file "$PID_FILE"
}

read_launcher_pid() {
  read_pid_file "$LAUNCHER_PID_FILE"
}

is_pid_running() {
  pid=${1:-}
  [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null
}

cleanup_pid_files() {
  rm -f "$PID_FILE" "$LAUNCHER_PID_FILE"
}

find_service_pid() {
  launcher_pid=${1:-}
  if [ -z "$launcher_pid" ]; then
    return 1
  fi

  for child_pid in $(pgrep -P "$launcher_pid" 2>/dev/null || true); do
    command=$(ps -p "$child_pid" -o command= 2>/dev/null || true)
    case "$command" in
      *burned-web*)
        echo "$child_pid"
        return 0
        ;;
    esac
  done

  return 1
}

has_started_marker() {
  [ -f "$LOG_FILE" ] && grep -q "Burned dashboard is running at http://127.0.0.1:" "$LOG_FILE"
}

is_running() {
  if [ ! -f "$PID_FILE" ]; then
    rm -f "$LAUNCHER_PID_FILE"
    return 1
  fi

  pid=$(read_pid 2>/dev/null || true)
  if [ -z "${pid:-}" ]; then
    cleanup_pid_files
    return 1
  fi

  if is_pid_running "$pid"; then
    return 0
  fi

  cleanup_pid_files
  return 1
}

start_burned() {
  if is_running; then
    pid=$(read_pid)
    echo "Burned is already running (PID $pid)."
    echo "Log: $LOG_FILE"
    return 0
  fi

  nohup "$ROOT_DIR/burned" >"$LOG_FILE" 2>&1 &
  launcher_pid=$!
  echo "$launcher_pid" >"$LAUNCHER_PID_FILE"

  attempt=0
  service_pid=""
  while [ "$attempt" -lt 60 ]; do
    service_pid=$(find_service_pid "$launcher_pid" || true)
    if [ -n "$service_pid" ] && is_pid_running "$service_pid" && has_started_marker; then
      echo "$service_pid" >"$PID_FILE"
      echo "$launcher_pid" >"$LAUNCHER_PID_FILE"
      echo "Burned started (PID $service_pid)."
      if [ "$launcher_pid" != "$service_pid" ] && is_pid_running "$launcher_pid"; then
        echo "Launcher PID: $launcher_pid"
      fi
      echo "Log: $LOG_FILE"
      return 0
    fi

    if ! is_pid_running "$launcher_pid" && [ -z "$service_pid" ]; then
      break
    fi

    attempt=$((attempt + 1))
    sleep 1
  done

  if [ -n "$service_pid" ] && is_pid_running "$service_pid"; then
    kill -TERM "$service_pid" 2>/dev/null || true
  fi
  if is_pid_running "$launcher_pid"; then
    pkill -TERM -P "$launcher_pid" 2>/dev/null || true
    kill -TERM "$launcher_pid" 2>/dev/null || true
  fi

  cleanup_pid_files
  echo "Burned failed to start. Check $LOG_FILE." >&2
  return 1
}

stop_burned() {
  service_pid=$(read_pid 2>/dev/null || true)
  launcher_pid=$(read_launcher_pid 2>/dev/null || true)

  if ! is_pid_running "$service_pid" && ! is_pid_running "$launcher_pid"; then
    cleanup_pid_files
    echo "Burned is not running."
    echo "Log: $LOG_FILE"
    return 0
  fi

  if is_pid_running "$launcher_pid"; then
    pkill -TERM -P "$launcher_pid" 2>/dev/null || true
    kill -TERM "$launcher_pid" 2>/dev/null || true
  fi
  if is_pid_running "$service_pid"; then
    kill -TERM "$service_pid" 2>/dev/null || true
  fi

  attempt=0
  while is_pid_running "$service_pid" || is_pid_running "$launcher_pid"; do
    attempt=$((attempt + 1))
    if [ "$attempt" -ge 20 ]; then
      if is_pid_running "$launcher_pid"; then
        pkill -KILL -P "$launcher_pid" 2>/dev/null || true
        kill -KILL "$launcher_pid" 2>/dev/null || true
      fi
      if is_pid_running "$service_pid"; then
        kill -KILL "$service_pid" 2>/dev/null || true
      fi
      break
    fi
    sleep 1
  done

  cleanup_pid_files
  echo "Burned stopped."
}

status_burned() {
  if is_running; then
    pid=$(read_pid)
    echo "Burned is running (PID $pid)."
    echo "Log: $LOG_FILE"
    return 0
  fi

  echo "Burned is not running."
  return 1
}

case "${1:-restart}" in
  start)
    start_burned
    ;;
  stop)
    stop_burned
    ;;
  restart)
    stop_burned
    start_burned
    ;;
  status)
    status_burned
    ;;
  *)
    echo "Usage: ./burned.sh [start|stop|restart|status]" >&2
    exit 1
    ;;
esac
