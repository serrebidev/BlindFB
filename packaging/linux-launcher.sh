#!/bin/sh
set -eu

app_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
packages='libgtk-3-0 gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-libav gstreamer1.0-x'

installed() {
    dpkg-query -W -f='${db:Status-Status}\n' "$1" 2>/dev/null | grep -qx installed
}

dependencies_installed() {
    for package in $packages; do
        if ! installed "$package"; then
            # Ubuntu 24.04 renamed GTK; apt still accepts its older package name.
            if [ "$package" = libgtk-3-0 ] && installed libgtk-3-0t64; then
                continue
            fi
            return 1
        fi
    done
}

fail() {
    printf 'BlindFB: %s\n' "$1" >&2
    exit 1
}

command -v dpkg-query >/dev/null 2>&1 && command -v apt-get >/dev/null 2>&1 ||
    fail 'This Linux download requires Ubuntu or Debian.'

if ! dependencies_installed; then
    printf 'BlindFB needs to install its media libraries. Your system may ask for your password.\n' >&2
    apt=$(command -v apt-get)
    install_dependencies() {
        if [ "$(id -u)" = 0 ]; then
            "$apt" install -y --no-install-recommends $packages
        elif command -v pkexec >/dev/null 2>&1; then
            pkexec "$apt" install -y --no-install-recommends $packages
        elif [ -t 0 ] && command -v sudo >/dev/null 2>&1; then
            sudo "$apt" install -y --no-install-recommends $packages
        else
            return 1
        fi
    }
    install_dependencies || fail 'could not install the media libraries. Launch BlindFB from a terminal to try again.'
    dependencies_installed || fail 'the media libraries are still unavailable. Launch BlindFB from a terminal to try again.'
fi

exec "$app_dir/blindfb-bin" "$@"
