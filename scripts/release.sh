#!/bin/bash
# Release script for agent-core
# Automates cargo release + GitHub release with pre-flight checks

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Print functions
info() { echo -e "${BLUE}[INFO]${NC} $1"; }
success() { echo -e "${GREEN}[OK]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; exit 1; }

# Usage
usage() {
    cat << EOF
Usage: $0 <bump> [options]

Arguments:
    bump        Version bump type: major, minor, patch, or explicit version (e.g., 1.2.3)

Options:
    --dry-run   Run all checks but don't execute the release
    --help      Show this help message

Examples:
    $0 patch              # Release 0.2.0 -> 0.2.1
    $0 minor              # Release 0.2.0 -> 0.3.0
    $0 major              # Release 0.2.0 -> 1.0.0
    $0 1.0.0              # Release to specific version
    $0 patch --dry-run    # Check everything without releasing
EOF
    exit 0
}

# Parse arguments
BUMP=""
DRY_RUN=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --help|-h)
            usage
            ;;
        *)
            if [[ -z "$BUMP" ]]; then
                BUMP="$1"
            else
                error "Unknown argument: $1"
            fi
            shift
            ;;
    esac
done

if [[ -z "$BUMP" ]]; then
    error "Version bump type required. Run '$0 --help' for usage."
fi

# Validate bump type
if [[ ! "$BUMP" =~ ^(major|minor|patch|[0-9]+\.[0-9]+\.[0-9]+)$ ]]; then
    error "Invalid bump type: $BUMP. Must be major, minor, patch, or a semver (e.g., 1.2.3)"
fi

echo ""
echo "=========================================="
echo "  agent-core Release Script"
echo "=========================================="
echo ""

# Get current version
CURRENT_VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
info "Current version: $CURRENT_VERSION"
info "Bump type: $BUMP"
echo ""

# Calculate new version
if [[ "$BUMP" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    NEW_VERSION="$BUMP"
else
    IFS='.' read -r major minor patch <<< "$CURRENT_VERSION"
    case $BUMP in
        major) NEW_VERSION="$((major + 1)).0.0" ;;
        minor) NEW_VERSION="$major.$((minor + 1)).0" ;;
        patch) NEW_VERSION="$major.$minor.$((patch + 1))" ;;
    esac
fi
info "New version will be: $NEW_VERSION"
echo ""

# ==========================================
# Pre-flight checks
# ==========================================

echo "Running pre-flight checks..."
echo ""

# Check 1: gh CLI installed
info "Checking gh CLI..."
if ! command -v gh &> /dev/null; then
    error "gh CLI is not installed. Run: brew install gh"
fi
success "gh CLI installed"

# Check 2: gh authenticated
info "Checking GitHub authentication..."
if ! gh auth status &> /dev/null; then
    error "Not authenticated to GitHub. Run: gh auth login"
fi
success "GitHub authenticated"

# Check 3: cargo-release installed
info "Checking cargo-release..."
if ! cargo release --version &> /dev/null; then
    error "cargo-release not installed. Run: cargo install cargo-release"
fi
success "cargo-release installed"

# Check 4: Clean working directory (except CHANGELOG.md which may be staged)
info "Checking working directory..."
DIRTY_FILES=$(git status --porcelain | grep -v CHANGELOG.md | grep -v scripts/release.sh || true)
if [[ -n "$DIRTY_FILES" ]]; then
    warn "Uncommitted changes detected:"
    echo "$DIRTY_FILES"
    echo ""
    read -p "Continue anyway? (y/N) " -n 1 -r
    echo ""
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        error "Aborted. Commit or stash your changes first."
    fi
fi
success "Working directory OK"

# Check 5: On main branch (or confirm)
info "Checking branch..."
CURRENT_BRANCH=$(git branch --show-current)
if [[ "$CURRENT_BRANCH" != "main" && "$CURRENT_BRANCH" != "mainline" && "$CURRENT_BRANCH" != "master" ]]; then
    warn "Not on main branch (currently on: $CURRENT_BRANCH)"
    read -p "Continue anyway? (y/N) " -n 1 -r
    echo ""
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        error "Aborted. Switch to main branch first."
    fi
fi
success "Branch: $CURRENT_BRANCH"

# Check 6: Up to date with remote
info "Checking remote sync..."
git fetch origin &> /dev/null || warn "Could not fetch from origin"
LOCAL=$(git rev-parse HEAD)
REMOTE=$(git rev-parse origin/$CURRENT_BRANCH 2>/dev/null || echo "")
if [[ -n "$REMOTE" && "$LOCAL" != "$REMOTE" ]]; then
    warn "Local branch differs from remote"
    read -p "Continue anyway? (y/N) " -n 1 -r
    echo ""
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        error "Aborted. Pull or push changes first."
    fi
fi
success "Remote sync OK"

# Check 7: CHANGELOG.md has entry for new version
info "Checking CHANGELOG.md..."
if ! grep -q "## \[$NEW_VERSION\]" CHANGELOG.md; then
    if grep -q "## \[Unreleased\]" CHANGELOG.md; then
        warn "CHANGELOG.md has [Unreleased] section but no [$NEW_VERSION] section"
        echo ""
        echo "You need to rename [Unreleased] to [$NEW_VERSION] in CHANGELOG.md"
        echo ""
        read -p "Open CHANGELOG.md in editor? (Y/n) " -n 1 -r
        echo ""
        if [[ ! $REPLY =~ ^[Nn]$ ]]; then
            ${EDITOR:-vim} CHANGELOG.md
            # Re-check after edit
            if ! grep -q "## \[$NEW_VERSION\]" CHANGELOG.md; then
                error "CHANGELOG.md still missing [$NEW_VERSION] section"
            fi
        else
            error "CHANGELOG.md must have a [$NEW_VERSION] section before release"
        fi
    else
        error "CHANGELOG.md missing both [Unreleased] and [$NEW_VERSION] sections"
    fi
fi
success "CHANGELOG.md has [$NEW_VERSION] section"

# Check 8: Release notes extraction works
info "Testing release notes extraction..."
RELEASE_NOTES=$(./scripts/extract-release-notes.sh "$NEW_VERSION" 2>/dev/null || echo "")
if [[ -z "$RELEASE_NOTES" ]]; then
    error "Could not extract release notes for $NEW_VERSION from CHANGELOG.md"
fi
NOTES_LINES=$(echo "$RELEASE_NOTES" | wc -l | tr -d ' ')
if [[ "$NOTES_LINES" -lt 3 ]]; then
    warn "Release notes seem short ($NOTES_LINES lines). Preview:"
    echo "$RELEASE_NOTES"
    echo ""
    read -p "Continue with these release notes? (y/N) " -n 1 -r
    echo ""
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        error "Aborted. Update CHANGELOG.md with more detailed release notes."
    fi
fi
success "Release notes OK ($NOTES_LINES lines)"

# Check 9: cargo release dry run
info "Running cargo release dry run..."
echo ""
if ! cargo release "$BUMP" 2>&1; then
    error "cargo release dry run failed"
fi
echo ""
success "cargo release dry run passed"

echo ""
echo "=========================================="
echo "  All pre-flight checks passed!"
echo "=========================================="
echo ""

# Show release notes preview
echo "Release notes for v$NEW_VERSION:"
echo "---"
echo "$RELEASE_NOTES"
echo "---"
echo ""

if [[ "$DRY_RUN" == true ]]; then
    info "Dry run complete. No changes made."
    exit 0
fi

# Confirm release
echo -e "${YELLOW}Ready to release v$NEW_VERSION${NC}"
echo ""
echo "This will:"
echo "  1. Bump version in Cargo.toml"
echo "  2. Commit and create git tag v$NEW_VERSION"
echo "  3. Push to GitHub"
echo "  4. Publish to crates.io"
echo "  5. Create GitHub release with notes"
echo ""
read -p "Proceed with release? (y/N) " -n 1 -r
echo ""
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    info "Aborted."
    exit 0
fi

# ==========================================
# Execute release
# ==========================================

echo ""
info "Executing cargo release..."
if ! cargo release "$BUMP" --execute --no-confirm; then
    error "cargo release failed"
fi

success "cargo release completed"

# Check if GitHub release was created by the hook
info "Verifying GitHub release..."
sleep 2  # Give GitHub a moment

if gh release view "v$NEW_VERSION" &> /dev/null; then
    success "GitHub release v$NEW_VERSION created"
else
    warn "GitHub release not found - creating manually..."

    if echo "$RELEASE_NOTES" | gh release create "v$NEW_VERSION" \
        --title "v$NEW_VERSION" \
        --notes-file -; then
        success "GitHub release v$NEW_VERSION created manually"
    else
        error "Failed to create GitHub release. Create manually with:"
        echo "  ./scripts/extract-release-notes.sh $NEW_VERSION | gh release create v$NEW_VERSION --title 'v$NEW_VERSION' --notes-file -"
    fi
fi

echo ""
echo "=========================================="
echo -e "  ${GREEN}Release v$NEW_VERSION complete!${NC}"
echo "=========================================="
echo ""
echo "Links:"
echo "  GitHub: https://github.com/deepmesa/agent-core/releases/tag/v$NEW_VERSION"
echo "  Crates: https://crates.io/crates/agent-core/$NEW_VERSION"
echo ""
