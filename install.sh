#!/usr/bin/env sh
# liteconfig installer — downloads a prebuilt binary, verifies the checksum,
# and drops it in a user-writable directory on PATH.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/liteconfig/liteconfig/main/install.sh | sh
#
# Environment overrides:
#   LITECONFIG_VERSION  pin a specific version (default: latest release)
#   LITECONFIG_BIN_DIR  install location (default: $HOME/.local/bin)
#   LITECONFIG_REPO     override GitHub repo (default: liteconfig/liteconfig)
#
# Idempotent: re-running the same command upgrades in place.

set -eu

REPO="${LITECONFIG_REPO:-liteconfig/liteconfig}"
VERSION="${LITECONFIG_VERSION:-latest}"
BIN_DIR="${LITECONFIG_BIN_DIR:-$HOME/.local/bin}"
BIN_NAME="liteconfig"

# ---------- pretty output ----------

if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
    BOLD="$(printf '\033[1m')"; DIM="$(printf '\033[2m')"
    GREEN="$(printf '\033[32m')"; RED="$(printf '\033[31m')"
    YELLOW="$(printf '\033[33m')"; RESET="$(printf '\033[0m')"
else
    BOLD=""; DIM=""; GREEN=""; RED=""; YELLOW=""; RESET=""
fi

info()  { printf '%s•%s %s\n' "$DIM" "$RESET" "$*"; }
ok()    { printf '%s✓%s %s\n' "$GREEN" "$RESET" "$*"; }
warn()  { printf '%s!%s %s\n' "$YELLOW" "$RESET" "$*" >&2; }
fail()  { printf '%s✗%s %s\n' "$RED" "$RESET" "$*" >&2; exit 1; }

# ---------- platform detection ----------

uname_s="$(uname -s 2>/dev/null || echo unknown)"
uname_m="$(uname -m 2>/dev/null || echo unknown)"

case "$uname_s" in
    Darwin) os="apple-darwin" ;;
    Linux)
        if ldd --version 2>&1 | grep -qi musl; then
            fail "Prebuilt musl Linux binaries are not published yet - use \`cargo install liteconfig-tui\`."
        else
            os="unknown-linux-gnu"
        fi
        ;;
    MINGW*|MSYS*|CYGWIN*)
        fail "Windows is not supported by this script — use cargo install or scoop."
        ;;
    *) fail "Unsupported OS: $uname_s (try \`cargo install liteconfig-tui\`)" ;;
esac

case "$uname_m" in
    x86_64|amd64) arch="x86_64" ;;
    arm64|aarch64) arch="aarch64" ;;
    *) fail "Unsupported arch: $uname_m (try \`cargo install liteconfig-tui\`)" ;;
esac

target="${arch}-${os}"

# ---------- dependencies ----------

need() { command -v "$1" >/dev/null 2>&1 || fail "Required tool missing: $1"; }
need curl
need tar
if command -v shasum >/dev/null 2>&1; then sha_cmd="shasum -a 256"
elif command -v sha256sum >/dev/null 2>&1; then sha_cmd="sha256sum"
else fail "Need shasum or sha256sum to verify the download"
fi

# ---------- resolve version ----------

api_url="https://api.github.com/repos/${REPO}/releases"
if [ "$VERSION" = "latest" ]; then
    info "Looking up latest release from ${REPO}"
    VERSION="$(curl -fsSL "${api_url}/latest" \
        | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' \
        | head -n1)"
    [ -n "$VERSION" ] || fail "Couldn't determine latest version — is ${REPO} a public repo with releases?"
fi
ok "Installing ${BOLD}liteconfig ${VERSION}${RESET} for ${target}"

# ---------- download ----------

asset="liteconfig-${VERSION}-${target}.tar.gz"
base="https://github.com/${REPO}/releases/download/${VERSION}"
tmp="$(mktemp -d 2>/dev/null || mktemp -d -t liteconfig)"
trap 'rm -rf "$tmp"' EXIT INT TERM

info "Downloading $asset"
curl -fsSL --retry 3 --retry-delay 2 -o "${tmp}/${asset}" "${base}/${asset}" \
    || fail "Download failed from ${base}/${asset}"

info "Downloading checksums"
curl -fsSL --retry 3 --retry-delay 2 -o "${tmp}/SHA256SUMS" "${base}/SHA256SUMS" \
    || warn "Could not fetch SHA256SUMS — skipping verification (not recommended)"

if [ -f "${tmp}/SHA256SUMS" ]; then
    expected="$(grep " ${asset}$" "${tmp}/SHA256SUMS" | awk '{print $1}')"
    [ -n "$expected" ] || fail "No checksum for ${asset} in SHA256SUMS"
    actual="$(cd "$tmp" && $sha_cmd "$asset" | awk '{print $1}')"
    [ "$expected" = "$actual" ] || fail "Checksum mismatch! expected=$expected actual=$actual"
    ok "Checksum verified"
fi

# ---------- extract & install ----------

info "Extracting"
tar -xzf "${tmp}/${asset}" -C "$tmp"

src_bin="${tmp}/${BIN_NAME}"
[ -f "$src_bin" ] || src_bin="$(find "$tmp" -type f -name "$BIN_NAME" -perm -u+x 2>/dev/null | head -n1)"
[ -n "$src_bin" ] && [ -f "$src_bin" ] || fail "Binary ${BIN_NAME} not found inside archive"

mkdir -p "$BIN_DIR" || fail "Cannot create $BIN_DIR"
install -m 0755 "$src_bin" "${BIN_DIR}/${BIN_NAME}" 2>/dev/null \
    || cp "$src_bin" "${BIN_DIR}/${BIN_NAME}" && chmod 0755 "${BIN_DIR}/${BIN_NAME}"

ok "Installed to ${BOLD}${BIN_DIR}/${BIN_NAME}${RESET}"

# ---------- PATH hint ----------

case ":$PATH:" in
    *":${BIN_DIR}:"*) ;;
    *)
        warn "${BIN_DIR} is not on your PATH."
        printf '  Add this to your shell config (%s/.zshrc, %s/.bashrc, etc.):\n' "$HOME" "$HOME"
        printf '    %sexport PATH="%s:$PATH"%s\n' "$BOLD" "$BIN_DIR" "$RESET"
        ;;
esac

printf '\n%sNext:%s  run %sliteconfig%s to launch the TUI.\n' "$BOLD" "$RESET" "$BOLD" "$RESET"
