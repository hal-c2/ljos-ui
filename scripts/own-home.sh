# Sourced, not run: own_home DIR gives the rest of the script, and what it starts, an empty
# home under DIR. What an app saves (Settings, LocalStorage) then stays out of the user's
# directories, where it would outlive the run and change what the next run or picture shows.
# HOME moves with the XDG directories, so a variable left relative or empty falls back inside
# DIR too, and Ljós is told the real home (LJOS_GUARD_HOME), which it then refuses to write
# in. The user's fonts and font configuration come along, so text looks as it does outside.
# Tools that find their own files through HOME (cargo, rustup) keep them.
own_home() {
  local dir="$1"
  local user_config="${XDG_CONFIG_HOME:-$HOME/.config}" user_data="${XDG_DATA_HOME:-$HOME/.local/share}"
  [[ $user_config == /* ]] || user_config="$HOME/.config"
  [[ $user_data == /* ]] || user_data="$HOME/.local/share"
  rm -rf "$dir"
  mkdir -p "$dir/home" "$dir/config" "$dir/data" "$dir/state" "$dir/cache"
  if [[ -d $user_config/fontconfig ]]; then ln -s "$user_config/fontconfig" "$dir/config/fontconfig"; fi
  if [[ -d $user_data/fonts ]]; then ln -s "$user_data/fonts" "$dir/data/fonts"; fi
  export LJOS_GUARD_HOME="${LJOS_GUARD_HOME:-$HOME}"
  export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
  export HOME="$dir/home" XDG_CONFIG_HOME="$dir/config" XDG_DATA_HOME="$dir/data" \
    XDG_STATE_HOME="$dir/state" XDG_CACHE_HOME="$dir/cache"
}
