#!/bin/bash
set -euo pipefail

# ─────────────────────────────────────────────────────────────────────────────
# Rust Browser Engine — AppImage Builder
# ─────────────────────────────────────────────────────────────────────────────

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

info()  { echo -e "${GREEN}[✓]${NC} $*"; }
warn()  { echo -e "${YELLOW}[!]${NC} $*"; }
err()   { echo -e "${RED}[✗]${NC} $*"; }
step()  { echo -e "\n${BLUE}${BOLD}── $* ──${NC}"; }

APP_NAME="RustBrowserEngine"
BIN_NAME="rust_browser"
ARCH="$(uname -m)"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
APPDIR="${SCRIPT_DIR}/${APP_NAME}.AppDir"
APPIMAGETOOL="${SCRIPT_DIR}/appimagetool-${ARCH}.AppImage"

# Try to read version from Cargo.toml
VERSION="$(grep '^version' "${SCRIPT_DIR}/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/' 2>/dev/null || echo "0.1.0")"

echo -e "\n${BOLD}🌐 Rust Browser Engine — AppImage Builder${NC}"
echo -e "   Version: ${VERSION}  Arch: ${ARCH}\n"

# ─────────────────────────────────────────────────────────────────────────────
# 1. Check dependencies
# ─────────────────────────────────────────────────────────────────────────────
step "Checking dependencies"

CARGO_BIN=""
if command -v cargo &>/dev/null; then
    CARGO_BIN="cargo"
elif [ -x "$HOME/.cargo/bin/cargo" ]; then
    CARGO_BIN="$HOME/.cargo/bin/cargo"
    export PATH="$HOME/.cargo/bin:$PATH"
else
    err "cargo not found. Install Rust: https://rustup.rs"
    exit 1
fi
info "cargo found: $(${CARGO_BIN} --version)"

if ! command -v strip &>/dev/null; then
    warn "strip not found — binary will not be stripped (larger size)"
fi

DOWNLOAD_CMD=""
if command -v wget &>/dev/null; then
    DOWNLOAD_CMD="wget"
elif command -v curl &>/dev/null; then
    DOWNLOAD_CMD="curl"
else
    err "Neither wget nor curl found. Install one to download appimagetool."
    exit 1
fi
info "Download tool: ${DOWNLOAD_CMD}"

# ─────────────────────────────────────────────────────────────────────────────
# 2. Clean previous builds
# ─────────────────────────────────────────────────────────────────────────────
step "Cleaning previous AppImage artifacts"

if [ -d "${APPDIR}" ]; then
    rm -rf "${APPDIR}"
    info "Removed old ${APP_NAME}.AppDir"
fi

rm -f "${SCRIPT_DIR}/${APP_NAME}-"*.AppImage 2>/dev/null || true
info "Clean complete"

# ─────────────────────────────────────────────────────────────────────────────
# 3. Build release binary
# ─────────────────────────────────────────────────────────────────────────────
step "Building release binary"

cd "${SCRIPT_DIR}"
${CARGO_BIN} build --release 2>&1

RELEASE_BIN="${SCRIPT_DIR}/target/release/${BIN_NAME}"
if [ ! -f "${RELEASE_BIN}" ]; then
    err "Release binary not found at ${RELEASE_BIN}"
    exit 1
fi

ORIG_SIZE=$(du -h "${RELEASE_BIN}" | cut -f1)
info "Binary built: ${RELEASE_BIN} (${ORIG_SIZE})"

# ─────────────────────────────────────────────────────────────────────────────
# 4. Download appimagetool (if not present)
# ─────────────────────────────────────────────────────────────────────────────
step "Preparing appimagetool"

APPIMAGETOOL_URL="https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-${ARCH}.AppImage"

if [ ! -f "${APPIMAGETOOL}" ]; then
    info "Downloading appimagetool for ${ARCH}..."
    if [ "${DOWNLOAD_CMD}" = "wget" ]; then
        wget -q --show-progress -O "${APPIMAGETOOL}" "${APPIMAGETOOL_URL}"
    else
        curl -L --progress-bar -o "${APPIMAGETOOL}" "${APPIMAGETOOL_URL}"
    fi
    chmod +x "${APPIMAGETOOL}"
    info "Downloaded appimagetool"
else
    info "appimagetool already present"
fi

# ─────────────────────────────────────────────────────────────────────────────
# 5. Create AppDir structure
# ─────────────────────────────────────────────────────────────────────────────
step "Creating AppDir structure"

mkdir -p "${APPDIR}/usr/bin"
mkdir -p "${APPDIR}/usr/share/applications"
mkdir -p "${APPDIR}/usr/share/icons/hicolor/scalable/apps"

# Copy and strip binary
cp "${RELEASE_BIN}" "${APPDIR}/usr/bin/${BIN_NAME}"
if command -v strip &>/dev/null; then
    strip "${APPDIR}/usr/bin/${BIN_NAME}"
    STRIPPED_SIZE=$(du -h "${APPDIR}/usr/bin/${BIN_NAME}" | cut -f1)
    info "Binary copied and stripped: ${ORIG_SIZE} → ${STRIPPED_SIZE}"
else
    info "Binary copied (not stripped): ${ORIG_SIZE}"
fi

# Copy desktop file
cp "${SCRIPT_DIR}/assets/rust-browser.desktop" "${APPDIR}/usr/share/applications/rust-browser.desktop"
info "Desktop file installed"

# Bundle a system font so the browser works on any system
mkdir -p "${APPDIR}/usr/share/fonts"
FONT_SRC=""
for f in /usr/share/fonts/liberation-sans-fonts/LiberationSans-Regular.ttf \
         /usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf \
         /usr/share/fonts/truetype/dejavu/DejaVuSans.ttf \
         /usr/share/fonts/google-droid-sans-fonts/DroidSans.ttf \
         /usr/share/fonts/TTF/DejaVuSans.ttf; do
    if [ -f "$f" ]; then
        FONT_SRC="$f"
        break
    fi
done
if [ -n "${FONT_SRC}" ]; then
    cp "${FONT_SRC}" "${APPDIR}/usr/share/fonts/LiberationSans-Regular.ttf"
    info "Font bundled: $(basename "${FONT_SRC}")"
else
    warn "No system font found to bundle — text rendering may not work"
fi

# Copy icon
cp "${SCRIPT_DIR}/assets/rust-browser.svg" "${APPDIR}/usr/share/icons/hicolor/scalable/apps/rust-browser.svg"
info "Icon installed"

# Create root-level symlinks (required by AppImage spec)
ln -sf usr/share/applications/rust-browser.desktop "${APPDIR}/rust-browser.desktop"
ln -sf usr/share/icons/hicolor/scalable/apps/rust-browser.svg "${APPDIR}/rust-browser.svg"

# Create AppRun script
cat > "${APPDIR}/AppRun" << 'APPRUN_EOF'
#!/bin/bash
SELF_DIR="$(dirname "$(readlink -f "$0")")"
exec "${SELF_DIR}/usr/bin/rust_browser" "$@"
APPRUN_EOF
chmod +x "${APPDIR}/AppRun"
info "AppRun script created"

# Show AppDir structure
echo ""
info "AppDir structure:"
find "${APPDIR}" -type f -o -type l | sort | while read -r f; do
    echo "     ${f#${APPDIR}/}"
done

# ─────────────────────────────────────────────────────────────────────────────
# 6. Package AppImage
# ─────────────────────────────────────────────────────────────────────────────
step "Packaging AppImage"

OUTPUT_FILE="${SCRIPT_DIR}/${APP_NAME}-${VERSION}-${ARCH}.AppImage"

# Try running appimagetool directly; fall back to --appimage-extract-and-run
# for environments without FUSE (containers, CI, etc.)
export ARCH="${ARCH}"
if "${APPIMAGETOOL}" --help &>/dev/null 2>&1; then
    "${APPIMAGETOOL}" "${APPDIR}" "${OUTPUT_FILE}"
else
    warn "FUSE not available — using --appimage-extract-and-run"
    APPIMAGE_EXTRACT_AND_RUN=1 "${APPIMAGETOOL}" "${APPDIR}" "${OUTPUT_FILE}"
fi

chmod +x "${OUTPUT_FILE}"

# ─────────────────────────────────────────────────────────────────────────────
# 7. Summary
# ─────────────────────────────────────────────────────────────────────────────
echo ""
echo -e "${GREEN}${BOLD}════════════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}${BOLD}  ✅ AppImage built successfully!${NC}"
echo -e "${GREEN}${BOLD}════════════════════════════════════════════════════════════════${NC}"
echo ""

FILE_SIZE=$(du -h "${OUTPUT_FILE}" | cut -f1)
SHA256=$(sha256sum "${OUTPUT_FILE}" | cut -d' ' -f1)

echo -e "  ${BOLD}File:${NC}    $(basename "${OUTPUT_FILE}")"
echo -e "  ${BOLD}Size:${NC}    ${FILE_SIZE}"
echo -e "  ${BOLD}SHA-256:${NC} ${SHA256}"
echo -e "  ${BOLD}Path:${NC}    ${OUTPUT_FILE}"
echo ""
echo -e "  Run it with:"
echo -e "    ${BOLD}chmod +x $(basename "${OUTPUT_FILE}")${NC}"
echo -e "    ${BOLD}./$(basename "${OUTPUT_FILE}")${NC}"
echo ""
