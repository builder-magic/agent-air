#!/bin/bash
# Extract release notes for a specific version from CHANGELOG.md
# Usage: ./scripts/extract-release-notes.sh 0.2.0

set -e

VERSION="${1:-}"

if [ -z "$VERSION" ]; then
    echo "Usage: $0 <version>"
    echo "Example: $0 0.2.0"
    exit 1
fi

# Extract the section for this version (from ## [version] to next ## [)
sed -n "/^## \\[$VERSION\\]/,/^## \\[/p" CHANGELOG.md | sed '$ d'
