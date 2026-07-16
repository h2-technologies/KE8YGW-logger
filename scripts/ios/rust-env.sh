#!/usr/bin/env bash

PATH="${PATH:-}"

if [[ -n "${HOME:-}" && -f "$HOME/.cargo/env" ]]; then
  # shellcheck source=/dev/null
  . "$HOME/.cargo/env"
fi

prepend_path_if_dir() {
  local directory="$1"

  if [[ -d "$directory" ]]; then
    case ":$PATH:" in
      *":$directory:"*) ;;
      *) PATH="$directory:$PATH" ;;
    esac
  fi
}

if [[ -n "${HOME:-}" ]]; then
  prepend_path_if_dir "$HOME/.cargo/bin"
fi

prepend_path_if_dir "/opt/homebrew/bin"
prepend_path_if_dir "/usr/local/bin"

export PATH

require_tool() {
  local tool="$1"
  local hint="${2:-}"

  if ! command -v "$tool" >/dev/null 2>&1; then
    if [[ -n "$hint" ]]; then
      echo "error: required tool '$tool' was not found on PATH. $hint" >&2
    else
      echo "error: required tool '$tool' was not found on PATH." >&2
    fi
    exit 1
  fi
}
