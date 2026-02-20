#!/usr/bin/env bash
set -euo pipefail

# emulaunch installation script
# Detects OS and architecture, downloads appropriate binary, and installs

REPO="ArnabXD/emulaunch"
BINARY_NAME="emulaunch"
VERSION="${1:-latest}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

info() {
  echo -e "${GREEN}INFO:${NC} $1"
}

warn() {
  echo -e "${YELLOW}WARN:${NC} $1"
}

error() {
  echo -e "${RED}ERROR:${NC} $1" >&2
}

# Detect OS
detect_os() {
  case "$(uname -s)" in
    Linux*)  echo "linux";;
    Darwin*) echo "macos";;
    MINGW*|MSYS*|CYGWIN*) echo "windows";;
    *)
      error "Unsupported OS: $(uname -s)"
      exit 1
      ;;
  esac
}

# Detect architecture
detect_arch() {
  case "$(uname -m)" in
    x86_64|amd64) echo "x86_64";;
    aarch64|arm64) echo "aarch64";;
    *)
      error "Unsupported architecture: $(uname -m)"
      exit 1
      ;;
  esac
}

# Get download URL for current platform
get_download_url() {
  local os=$1
  local arch=$2

  case "$os-$arch" in
    linux-x86_64)
      echo "${BINARY_NAME}-x86_64-unknown-linux-gnu.tar.gz"
      ;;
    linux-aarch64)
      echo "${BINARY_NAME}-aarch64-unknown-linux-gnu.tar.gz"
      ;;
    macos-x86_64)
      echo "${BINARY_NAME}-x86_64-apple-darwin.tar.gz"
      ;;
    macos-aarch64)
      echo "${BINARY_NAME}-aarch64-apple-darwin.tar.gz"
      ;;
    windows-x86_64)
      echo "${BINARY_NAME}-x86_64-pc-windows-msvc.zip"
      ;;
    *)
      error "Unsupported platform: $os-$arch"
      exit 1
      ;;
  esac
}

# Download the binary
download_binary() {
  local url=$1
  local dest=$2

  info "Downloading $BINARY_NAME from $url..."

  if command -v curl &> /dev/null; then
    curl -fsSL "$url" -o "$dest"
  elif command -v wget &> /dev/null; then
    wget -q "$url" -O "$dest"
  else
    error "Neither curl nor wget is installed"
    exit 1
  fi
}

# Extract the archive
extract_archive() {
  local archive=$1
  local dest=$2

  case "$archive" in
    *.tar.gz)
      tar -xzf "$archive" -C "$dest"
      ;;
    *.zip)
      unzip -q "$archive" -d "$dest"
      ;;
  esac
}

# Get version string
get_version() {
  if [ "$VERSION" = "latest" ]; then
    curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/'
  else
    echo "$VERSION"
  fi
}

# Main installation
main() {
  local os
  local arch
  local filename
  local version
  local download_url
  local temp_dir
  local install_dir
  local binary_path

  os=$(detect_os)
  arch=$(detect_arch)
  filename=$(get_download_url "$os" "$arch")
  version=$(get_version)

  info "Installing $BINARY_NAME $version for $os-$arch"

  if [ "$os" = "windows" ]; then
    download_url="https://github.com/$REPO/releases/download/$version/$filename"
  else
    download_url="https://github.com/$REPO/releases/download/$version/$filename"
  fi

  # Create temporary directory
  temp_dir=$(mktemp -d)
  trap "rm -rf '$temp_dir'" EXIT

  # Download binary
  local archive_path="$temp_dir/$filename"
  download_binary "$download_url" "$archive_path"

  # Extract archive
  extract_archive "$archive_path" "$temp_dir"

  # Find install directory
  if [ -n "${INSTALL_DIR:-}" ]; then
    install_dir="$INSTALL_DIR"
  elif [ "$os" = "windows" ]; then
    install_dir="$LOCALAPPDATA\\emulaunch"
  else
    # Prefer ~/.local/bin if it exists or user has write access
    if [ -w "$HOME/.local/bin" ] || mkdir -p "$HOME/.local/bin" 2>/dev/null; then
      install_dir="$HOME/.local/bin"
    elif [ -w "/usr/local/bin" ]; then
      install_dir="/usr/local/bin"
    else
      install_dir="$HOME/.local/bin"
      mkdir -p "$install_dir" 2>/dev/null || {
        warn "Could not create $install_dir, trying with sudo"
        install_dir="/usr/local/bin"
      }
    fi
  fi

  # Create install directory if needed
  if [ ! -d "$install_dir" ]; then
    if [ -w "$(dirname "$install_dir")" ]; then
      mkdir -p "$install_dir"
    else
      sudo mkdir -p "$install_dir"
    fi
  fi

  # Copy binary to install directory
  local temp_binary="$temp_dir/$BINARY_NAME"
  if [ ! -f "$temp_binary" ]; then
    # cargo-dist might put binary in a subdirectory
    temp_binary=$(find "$temp_dir" -name "$BINARY_NAME" -type f | head -1)
  fi

  if [ -z "$temp_binary" ] || [ ! -f "$temp_binary" ]; then
    error "Could not find $BINARY_NAME in downloaded archive"
    exit 1
  fi

  if [ -w "$install_dir" ]; then
    cp "$temp_binary" "$install_dir/$BINARY_NAME"
    chmod +x "$install_dir/$BINARY_NAME"
  else
    sudo cp "$temp_binary" "$install_dir/$BINARY_NAME"
    sudo chmod +x "$install_dir/$BINARY_NAME"
  fi

  info "Installed $BINARY_NAME to $install_dir/$BINARY_NAME"

  # Check if install directory is in PATH
  if [ "$os" != "windows" ]; then
    case ":$PATH:" in
      *":$install_dir:"*)
        info "$install_dir is already in PATH"
        ;;
      *)
        warn "$install_dir is not in PATH"
        info "Add it to your shell configuration:"
        echo ""
        echo "  For bash:  echo 'export PATH=\"\$PATH:$install_dir\"' >> ~/.bashrc"
        echo "  For zsh:   echo 'export PATH=\"\$PATH:$install_dir\"' >> ~/.zshrc"
        echo "  For fish:  fish_add_path $install_dir"
        ;;
    esac
  fi

  info "Installation complete! Run '$BINARY_NAME' to get started."
}

main "$@"
