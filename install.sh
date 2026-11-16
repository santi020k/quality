#!/bin/sh
set -eu

repository=${1:-}
version=${2:-latest}

usage() {
    echo "Usage: install.sh OWNER/REPOSITORY [VERSION]" >&2
    echo "Example: install.sh your-org/quality v2.0.0" >&2
    exit 2
}

[ -n "$repository" ] || usage
owner=${repository%%/*}
name=${repository#*/}
[ -n "$owner" ] && [ -n "$name" ] && [ "$name" = "${name#*/}" ] || usage
case "$repository" in
    *[!A-Za-z0-9._/-]*) usage ;;
esac
case "$version" in
    latest|v[0-9]*) ;;
    *) echo "Version must be 'latest' or a tag beginning with v followed by a number." >&2; exit 2 ;;
esac

system=$(uname -s)
machine=$(uname -m)
case "$system:$machine" in
    Darwin:arm64|Darwin:aarch64) target=aarch64-apple-darwin ;;
    Darwin:x86_64|Darwin:amd64) target=x86_64-apple-darwin ;;
    Linux:x86_64|Linux:amd64) target=x86_64-unknown-linux-gnu ;;
    Linux:aarch64|Linux:arm64) target=aarch64-unknown-linux-gnu ;;
    *)
        echo "No prebuilt quality binary is available for $system/$machine." >&2
        echo "Install with Cargo or download a release archive manually." >&2
        exit 1
        ;;
esac

asset="quality-$target.tar.gz"
if [ "$version" = latest ]; then
    release_path="releases/latest/download"
else
    release_path="releases/download/$version"
fi
base_url=${QUALITY_RELEASE_BASE_URL:-https://github.com}
download_url="$base_url/$repository/$release_path"
temporary_directory=$(mktemp -d "${TMPDIR:-/tmp}/quality-install.XXXXXX")
trap 'rm -rf "$temporary_directory"' EXIT HUP INT TERM

echo "Downloading $asset..."
curl --fail --location --silent --show-error \
    "$download_url/$asset" --output "$temporary_directory/$asset"
curl --fail --location --silent --show-error \
    "$download_url/$asset.sha256" --output "$temporary_directory/$asset.sha256"

# Use only the entry that names this archive. A checksum tool's --check mode
# verifies whatever files the download lists, even /dev/null, and some
# implementations succeed without verifying any file.
entries=0
expected_digest=
carriage_return=$(printf '\r')
while IFS= read -r line || [ -n "$line" ]; do
    line=${line%"$carriage_return"}
    digest=${line%%[!0123456789abcdefABCDEF]*}
    case ${line#"$digest"} in
        " $asset" | "  $asset" | " *$asset")
            entries=$((entries + 1))
            expected_digest=$digest
            ;;
    esac
done < "$temporary_directory/$asset.sha256"
if [ "$entries" -ne 1 ] || [ "${#expected_digest}" -ne 64 ]; then
    echo "The checksum file must contain exactly one SHA-256 entry for $asset." >&2
    exit 1
fi
expected_digest=$(printf '%s\n' "$expected_digest" | tr ABCDEF abcdef)

if command -v sha256sum >/dev/null 2>&1; then
    actual_digest=$(sha256sum < "$temporary_directory/$asset")
elif command -v shasum >/dev/null 2>&1; then
    actual_digest=$(shasum -a 256 < "$temporary_directory/$asset")
else
    echo "A SHA-256 tool (sha256sum or shasum) is required." >&2
    exit 1
fi
actual_digest=${actual_digest%%[!0123456789abcdef]*}
if [ "$actual_digest" != "$expected_digest" ]; then
    echo "Checksum mismatch for $asset: expected $expected_digest, got $actual_digest." >&2
    exit 1
fi
echo "Verified the SHA-256 checksum of $asset."

tar -xzf "$temporary_directory/$asset" -C "$temporary_directory"
[ -f "$temporary_directory/quality" ] || {
    echo "The release archive does not contain the quality binary." >&2
    exit 1
}

install_directory=${QUALITY_INSTALL_DIR:-${HOME}/.local/bin}
mkdir -p "$install_directory"
install -m 0755 "$temporary_directory/quality" "$install_directory/quality"

echo "Installed quality to $install_directory/quality"
case ":${PATH}:" in
    *":$install_directory:"*) ;;
    *) echo "Add $install_directory to PATH to run quality from any directory." ;;
esac
