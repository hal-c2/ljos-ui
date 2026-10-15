#!/usr/bin/env bash
# A Ljós app in a headless sandbox, off the user's screen: its own sway with XWayland, its own
# session bus and AT-SPI registry, and cua-driver (https://cua.ai) to drive it from outside the
# way assistive technology would. The user's display and bus are never touched. Linux only.
#
#   cua-sandbox.sh start [<ljos run args>]   (re)launch `ljos run` there
#   cua-sandbox.sh exec <command…>           run any command there (mise run gallery:run <id> …)
#   cua-sandbox.sh call <tool> [<json>]      a cua-driver tool, on the app start launched
#   cua-sandbox.sh env                       print the sandbox's sockets and directories as KEY=VALUE lines
#   cua-sandbox.sh stop
#
# start and exec bring up what is not running. call fills in the app's pid, a shared session
# label and foreground delivery where the tool takes them. Files go under LJOS_CUA_HOME,
# default <target>/cua.
set -euo pipefail

die() {
  echo "desktop:cua: $*" >&2
  exit 1
}

root="$(cd "$(dirname "$0")/.." && pwd)"
target="${CARGO_TARGET_DIR:-$root/target}"
home="${LJOS_CUA_HOME:-$target/cua}"
# Socket paths have a short length limit: they go in the user's runtime dir, keyed by home.
runtime="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
[[ -d $runtime ]] || die "no XDG_RUNTIME_DIR for the sandbox's sockets"
run="$runtime/ljos-cua-$(printf %s "$home" | cksum | cut -d' ' -f1)"
session=ljos

# A pid is recorded with its start time, so a stale file never names another process.
track() { echo "$1 $(sed 's/.*) //' "/proc/$1/stat" | cut -d' ' -f20)" > "$home/$2.pid"; }
pid_of() {
  local pid start
  read -r pid start 2> /dev/null < "$home/$1.pid" || return 1
  [[ $(sed 's/.*) //' "/proc/$pid/stat" 2> /dev/null | cut -d' ' -f20) == "$start" ]] && echo "$pid"
}
stop_one() {
  local pid alive
  if pid=$(pid_of "$1"); then
    # A setsid'd process leads its own group; take what it started with it.
    if kill -- "-$pid" 2> /dev/null; then
      alive=(kill -0 -- "-$pid")
    else
      kill "$pid"
      alive=(pid_of "$1")
    fi
    for _ in $(seq 100); do
      "${alive[@]}" > /dev/null 2>&1 || break
      sleep 0.1
    done
  fi
  rm -f "$home/$1.pid"
}

# wait_for <seconds> <what> <command...>: until the command succeeds.
wait_for() {
  local deadline=$((SECONDS + $1)) what=$2
  shift 2
  until "$@"; do
    ((SECONDS < deadline)) || die "timed out waiting for $what"
    sleep 0.2
  done
}

user_config="${XDG_CONFIG_HOME:-$HOME/.config}"
user_data="${XDG_DATA_HOME:-$HOME/.local/share}"
user_state="${XDG_STATE_HOME:-$HOME/.local/state}"

# What a driven app saves (Settings, LocalStorage) stays in the sandbox: in the user's own
# directories it would outlive the run and change what the next run or render shows. The
# user's fonts and font configuration come along, so text looks as it does outside.
own_dirs() {
  local xdg="$home/xdg"
  mkdir -p "$xdg/config" "$xdg/data" "$xdg/state"
  if [[ -d $user_config/fontconfig ]]; then ln -sfn "$user_config/fontconfig" "$xdg/config/fontconfig"; fi
  if [[ -d $user_data/fonts ]]; then ln -sfn "$user_data/fonts" "$xdg/data/fonts"; fi
}

sandbox_env=()
load_sandbox() {
  [[ -s $home/display && -s $home/bus ]] || die "the sandbox is not running: mise run desktop:cua"
  own_dirs
  # mise's own directories follow XDG unless pinned, and a command run there needs its tools
  # and trusted configs.
  sandbox_env=(env -u WAYLAND_DISPLAY -u SWAYSOCK -u AT_SPI_BUS_ADDRESS -u HYPRLAND_INSTANCE_SIGNATURE
    DISPLAY="$(< "$home/display")" XDG_RUNTIME_DIR="$run" DBUS_SESSION_BUS_ADDRESS="$(< "$home/bus")"
    XDG_CONFIG_HOME="$home/xdg/config" XDG_DATA_HOME="$home/xdg/data" XDG_STATE_HOME="$home/xdg/state"
    MISE_CONFIG_DIR="${MISE_CONFIG_DIR:-$user_config/mise}" MISE_DATA_DIR="${MISE_DATA_DIR:-$user_data/mise}"
    MISE_STATE_DIR="${MISE_STATE_DIR:-$user_state/mise}"
    CUA_DRIVER_RS_HOME="$home/cua" CUA_DRIVER_RS_TELEMETRY_ENABLED=0 CUA_DRIVER_RS_UPDATE_CHECK=0)
}

start_sway() {
  if pid_of sway > /dev/null; then
    load_sandbox
    return
  fi
  command -v sway > /dev/null || die "needs sway, with Xwayland, for the headless display"
  command -v dbus-run-session > /dev/null || die "needs dbus-run-session (dbus)"
  # What ran on a previous sandbox's display and bus goes with it.
  stop_one app
  stop_one cua
  stop_one registry
  mkdir -p "$run" && chmod 700 "$run"
  rm -f "$home/display" "$home/bus" "$home/sway.raw"
  # The stock session bus listens in /tmp; this one's socket goes with the sandbox's others.
  cat > "$home/bus.config" << EOF
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <keep_umask/>
  <listen>unix:dir=$run</listen>
  <auth>EXTERNAL</auth>
  <standard_session_servicedirs/>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
EOF
  cat > "$home/sway.config" << EOF
output HEADLESS-1 resolution 1600x1000
default_border none
# Windows keep the size they ask for, as on a desktop that does not tile.
for_window [all] floating enable
xwayland enable
exec printenv DISPLAY > "$home/display"
EOF
  # shellcheck disable=SC2016 # expanded by the sandbox's shell
  (cd "$home" && exec env -u WAYLAND_DISPLAY -u DISPLAY -u SWAYSOCK -u DBUS_SESSION_BUS_ADDRESS \
    XDG_RUNTIME_DIR="$run" WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman \
    setsid dbus-run-session --config-file="$home/bus.config" -- sh -c 'echo "$DBUS_SESSION_BUS_ADDRESS" > bus; echo $$ > sway.raw; exec sway -c sway.config' \
    > "$home/sway.log" 2>&1) &
  wait_for 20 "sway (see $home/sway.log)" test -s "$home/sway.raw"
  track "$(< "$home/sway.raw")" sway
  wait_for 20 "Xwayland (see $home/sway.log; mise run desktop:cua:stop cleans up)" test -s "$home/display"
  load_sandbox
}

start_registry() {
  pid_of registry > /dev/null && return
  local bin
  for bin in /usr/lib/at-spi2-registryd /usr/libexec/at-spi2-registryd /usr/lib/at-spi2-core/at-spi2-registryd /usr/lib/*/at-spi2-registryd; do
    [[ -x $bin ]] && break
  done
  [[ -x $bin ]] || die "needs at-spi2-core (at-spi2-registryd) for the accessibility tree"
  # Its D-Bus activation goes through systemd, which the private bus cannot reach.
  "${sandbox_env[@]}" setsid "$bin" --use-gnome-session=false > "$home/registry.log" 2>&1 &
  track $! registry
}

start_cua() {
  pid_of cua > /dev/null && return
  # cua-driver is optional: without it the sandbox still keeps windows off the user's screen,
  # and the app's own inspector (--mcp) drives it.
  command -v cua-driver > /dev/null || return 0
  rm -f "$run/cua.sock"
  "${sandbox_env[@]}" setsid cua-driver serve --socket "$run/cua.sock" > "$home/cua.log" 2>&1 &
  track $! cua
  wait_for 20 "cua-driver (see $home/cua.log)" test -S "$run/cua.sock"
  # Its agent cursor is a window over the whole screen that takes the clicks meant for the app.
  "${sandbox_env[@]}" cua-driver call --socket "$run/cua.sock" set_agent_cursor_enabled \
    "{\"session\":\"$session\",\"enabled\":false}" > /dev/null
}

up() {
  [[ $(uname -s) == Linux ]] || die "runs on Linux only"
  mkdir -p "$home"
  start_sway
  start_registry
  start_cua
}

app_window() {
  local pid
  pid=$(pid_of app) || die "the app quit; see $home/app.log"
  pid_of cua > /dev/null || return 0
  "${sandbox_env[@]}" cua-driver call --socket "$run/cua.sock" list_windows "{\"session\":\"$session\"}" 2> /dev/null |
    grep -qE "\"pid\": *${pid}[^0-9]"
}
start() {
  (($#)) || die "usage: mise run desktop:cua <file.qml> [<ljos run args>]"
  up
  stop_one app
  cargo build --quiet --manifest-path "$root/Cargo.toml" -p ljos_cli
  # X11 through XWayland: cua-driver's input and capture are X11's.
  "${sandbox_env[@]}" setsid "$target/debug/ljos" run "$@" > "$home/app.log" 2>&1 &
  track $! app
  wait_for 60 "the app's window (see $home/app.log)" app_window
  cat << EOF
ljos run (pid $(pid_of app)) is up in the sandbox; its output is in $home/app.log.
$(command -v cua-driver > /dev/null && echo "  mise run desktop:cua:call get_window_state '{\"screenshot_out_file\":\"$home/shot.png\"}'")
mise run desktop:cua:stop ends it.
EOF
}

exec_in() {
  (($#)) || die "usage: mise run desktop:cua:exec <command…>"
  up
  exec "${sandbox_env[@]}" "$@"
}

# What a program outside the sandbox needs to reach it: the compositor's Wayland and IPC
# sockets, the X display and the bus, and the directories its applications save into. `mise run bench` starts its applications from the
# user's session (systemd-run needs the user's service manager) and hands them these.
print_env() {
  up
  local wayland sock
  for wayland in "$run"/wayland-*; do
    [[ -S $wayland ]] && break
  done
  for sock in "$run"/sway-ipc.*.sock; do
    [[ -S $sock ]] && break
  done
  [[ -S $wayland && -S $sock ]] || die "the sandbox's sway has no sockets in $run"
  printf '%s\n' "WAYLAND_DISPLAY=$wayland" "SWAYSOCK=$sock" "DISPLAY=$(< "$home/display")" \
    "DBUS_SESSION_BUS_ADDRESS=$(< "$home/bus")" "XDG_RUNTIME_DIR=$run" \
    "XDG_CONFIG_HOME=$home/xdg/config" "XDG_DATA_HOME=$home/xdg/data" "XDG_STATE_HOME=$home/xdg/state"
}

# Adds "key":value to the json object unless it has the key or the tool does not take it.
with() {
  [[ $json == *"\"$1\""* || $schema != *"\"$1\""* ]] && return
  if [[ $json =~ ^[[:space:]]*\{[[:space:]]*\}[[:space:]]*$ ]]; then
    json="{\"$1\":$2}"
  else
    json="{\"$1\":$2,${json#*\{}"
  fi
}
call() {
  (($#)) || die "usage: mise run desktop:cua:call <tool> [<json>]   (cua-driver list-tools names them)"
  pid_of cua > /dev/null || die "cua-driver is not running in the sandbox: mise run desktop:cua (needs cua-driver on the PATH)"
  load_sandbox
  local tool=$1 app
  json=${2:-'{}'}
  schema=$(cua-driver describe "$tool") || exit
  # Screenshot-relative coordinates need the session the screenshot was taken in.
  with session "\"$session\""
  app=$(pid_of app) && with pid "$app"
  with delivery_mode '"foreground"'
  # A pointer tool shows the agent cursor again, and its overlay would take the next click.
  [[ $tool == *agent_cursor* ]] || "${sandbox_env[@]}" cua-driver call --socket "$run/cua.sock" \
    set_agent_cursor_enabled "{\"session\":\"$session\",\"enabled\":false}" > /dev/null
  exec "${sandbox_env[@]}" cua-driver call --socket "$run/cua.sock" "$tool" "$json"
}

stop() {
  for part in app cua registry sway; do stop_one "$part"; done
  rm -f "$home/display" "$home/bus"
  rm -rf "$run"
}

case ${1:-} in
  start | call | stop)
    verb=$1
    shift
    "$verb" "$@"
    ;;
  exec)
    shift
    exec_in "$@"
    ;;
  env) print_env ;;
  *) die "usage: cua-sandbox.sh start|exec|call|env|stop" ;;
esac
