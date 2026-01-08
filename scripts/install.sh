#!/usr/bin/env bash
set -euo pipefail

# nudge installer script
#
# Usage:
#   curl -sSfL https://raw.githubusercontent.com/attunehq/nudge/main/scripts/install.sh | bash
#   curl -sSfL https://raw.githubusercontent.com/attunehq/nudge/main/scripts/install.sh | bash -s -- -b /usr/local/bin
#   curl -sSfL https://raw.githubusercontent.com/attunehq/nudge/main/scripts/install.sh | bash -s -- -v 0.1.0
#
# Options:
#   -v, --version    Specify a version (default: latest)
#   -b, --bin-dir    Specify the installation directory (default: $HOME/.local/bin)
#   -t, --tmp-dir    Specify the temporary directory (default: system temp directory)
#   -h, --help       Show help message

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

# GitHub repository configuration
REPO="attunehq/nudge"
GITHUB_API="https://api.github.com/repos/${REPO}/releases"
GITHUB_DOWNLOAD="https://github.com/${REPO}/releases/download"

# Fail with an error message
fail() {
  echo -e "${RED}Error: $1${NC}" >&2
  exit 1
}

# Print an informational message
info() {
  echo -e "${GREEN}$1${NC}" >&2
}

# Print a warning message
warn() {
  echo -e "${YELLOW}Warning: $1${NC}" >&2
}

# Check for required commands
check_requirements() {
  local missing=()

  # Check for curl
  if ! command -v curl > /dev/null; then
    missing+=("curl")
  fi

  # Check for tar
  if ! command -v tar > /dev/null; then
    missing+=("tar")
  fi

  # Check for checksum utility
  if ! command -v sha256sum > /dev/null && ! command -v shasum > /dev/null; then
    missing+=("sha256sum or shasum")
  fi

  if [[ ${#missing[@]} -gt 0 ]]; then
    fail "Missing required commands: ${missing[*]}

Please install the missing commands and try again:

  Debian/Ubuntu:  apt-get update && apt-get install -y curl tar coreutils
  Alpine:         apk add --no-cache curl tar coreutils
  RHEL/CentOS:    yum install -y curl tar coreutils
  macOS:          (should be pre-installed)"
  fi
}

# Detect the operating system and architecture
detect_platform() {
  local kernel
  local machine
  local os
  local arch

  kernel=$(uname -s)
  machine=$(uname -m)

  case "$kernel" in
    Linux)
      os="unknown-linux"
      ;;
    Darwin)
      os="apple-darwin"
      ;;
    MINGW* | MSYS* | CYGWIN*)
      fail "Windows is not supported by this installer."
      ;;
    *)
      fail "Unsupported operating system: $kernel"
      ;;
  esac

  case "$machine" in
    x86_64 | amd64)
      arch="x86_64"
      ;;
    arm64 | aarch64)
      arch="aarch64"
      ;;
    *)
      fail "Unsupported architecture: $machine"
      ;;
  esac

  # Check for musl instead of glibc on Linux
  if [[ "$os" == "unknown-linux" ]]; then
    if [[ -e /etc/alpine-release ]] || ldd /bin/sh 2>/dev/null | grep -q musl; then
      os="$os-musl"
    else
      os="$os-gnu"
    fi
  fi

  echo "${arch}-${os}"
}

# Parse command line arguments
parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      -v|--version)
        VERSION="$2"
        shift 2
        ;;
      -b|--bin-dir)
        BIN_DIR="$2"
        shift 2
        ;;
      -t|--tmp-dir)
        TMP_DIR="$2"
        shift 2
        ;;
      -h|--help)
        echo "nudge installer"
        echo
        echo "Usage: curl -sSfL https://raw.githubusercontent.com/attunehq/nudge/main/scripts/install.sh | bash [args]"
        echo
        echo "Options:"
        echo "  -v, --version    Specify a version (default: latest)"
        echo "  -b, --bin-dir    Specify the installation directory (default: \$HOME/.local/bin)"
        echo "  -t, --tmp-dir    Specify the temporary directory (default: system temp directory)"
        echo "  -h, --help       Show this help message"
        echo
        echo "Examples:"
        echo "  # Install latest version"
        echo "  curl -sSfL https://raw.githubusercontent.com/attunehq/nudge/main/scripts/install.sh | bash"
        echo
        echo "  # Install specific version"
        echo "  curl -sSfL https://raw.githubusercontent.com/attunehq/nudge/main/scripts/install.sh | bash -s -- -v 0.1.0"
        echo
        echo "  # Install to custom directory"
        echo "  curl -sSfL https://raw.githubusercontent.com/attunehq/nudge/main/scripts/install.sh | bash -s -- -b /usr/local/bin"
        exit 0
        ;;
      *)
        fail "Unknown option: $1"
        ;;
    esac
  done
}

# Get the latest version number from GitHub Releases API
get_latest_version() {
  local latest_url="${GITHUB_API}/latest"
  local version
  local response

  # Try to fetch latest release info from GitHub API
  if ! response=$(curl -sSfL "$latest_url" 2>&1); then
    fail "Failed to fetch latest release from $latest_url. Error: $response"
  fi

  version=$(echo "$response" | grep -o '"tag_name": *"[^"]*"' | cut -d'"' -f4 | sed 's/^v//')

  if [[ -z "$version" ]]; then
    fail "Could not parse version from GitHub API response. Response: $response"
  fi

  echo "$version"
}

# Download a file
download() {
  local url="$1"
  local dest="$2"

  info "Downloading from $url"

  if ! curl -sSfL "$url" -o "$dest"; then
    fail "Failed to download from $url"
  fi
}

# Install the binary
install_binary() {
  local platform="$1"
  local version="$2"
  local bin_dir="$3"
  local tmp_dir="$4"
  local archive_name="nudge-${platform}.tar.gz"
  local binary_name="nudge"

  version="${version#v}"
  local tag="v${version}"
  local download_url="${GITHUB_DOWNLOAD}/${tag}/${archive_name}"
  local checksums_url="${GITHUB_DOWNLOAD}/${tag}/checksums.txt"

  # Create temporary directory
  local workdir="$tmp_dir/nudge-install-$$"
  mkdir -p "$workdir"
  cd "$workdir"

  # Download archive and checksums
  download "$download_url" "$archive_name"
  download "$checksums_url" "checksums.txt"

  # Verify checksum
  info "Verifying checksum"
  local expected_checksum
  expected_checksum=$(grep "$archive_name" checksums.txt | awk '{print $1}')
  if [[ -z "$expected_checksum" ]]; then
    fail "Couldn't find checksum for $archive_name"
  fi

  local actual_checksum
  if command -v sha256sum > /dev/null; then
    actual_checksum=$(sha256sum "$archive_name" | awk '{print $1}')
  elif command -v shasum > /dev/null; then
    actual_checksum=$(shasum -a 256 "$archive_name" | awk '{print $1}')
  else
    fail "Neither sha256sum nor shasum found, cannot verify download"
  fi

  if [[ "$expected_checksum" != "$actual_checksum" ]]; then
    fail "Checksum verification failed! Expected: $expected_checksum, got: $actual_checksum"
  fi

  info "Checksum verified"

  # Extract archive and binary
  info "Extracting archive"
  tar -xzf "$archive_name"
  mkdir -p "$bin_dir"

  local extracted_binary
  extracted_binary=$(find . -name "$binary_name" -type f | head -n 1)
  if [[ -z "$extracted_binary" ]]; then
    fail "Could not find $binary_name in the extracted archive"
  fi
  cp "$extracted_binary" "$bin_dir/nudge"
  chmod +x "$bin_dir/nudge"

  # Clean up
  cd - > /dev/null
  rm -rf "$workdir"

  local installed_version
  installed_version=$("$bin_dir/nudge" --version 2>/dev/null || echo "nudge")
  info "Installed '$installed_version' to '$bin_dir/nudge'"

  # Check if bin_dir is in PATH
  if [[ ":$PATH:" != *":$bin_dir:"* ]]; then
    warn "'$bin_dir' is not in your PATH. You may need to add it to your shell's configuration."
    echo "" >&2
    echo "Add the following to your shell configuration file:" >&2
    echo "  export PATH=\"$bin_dir:\$PATH\"" >&2
  fi
}

# Main function
main() {
  # Set defaults
  local VERSION=""
  local BIN_DIR="$HOME/.local/bin"
  local TMP_DIR="${TMPDIR:-/tmp}"

  # Parse command line arguments
  parse_args "$@"

  # Check for required commands
  check_requirements

  # Detect platform
  local PLATFORM
  PLATFORM=$(detect_platform)
  info "Detected platform: $PLATFORM"

  # If version not specified, get latest
  if [[ -z "$VERSION" ]]; then
    VERSION=$(get_latest_version)
    info "Installing latest version: $VERSION"
  else
    info "Installing version: $VERSION"
  fi

  # Install binary
  install_binary "$PLATFORM" "$VERSION" "$BIN_DIR" "$TMP_DIR"

  echo "" >&2
  info "Installation complete! Run 'nudge --help' to get started."
}

# Run main function
main "$@"
