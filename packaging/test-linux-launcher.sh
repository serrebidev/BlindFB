#!/bin/sh
set -eu

source_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/blindfb-launcher-test.XXXXXX")
trap 'rm -rf -- "$work"' EXIT HUP INT TERM
mkdir -p "$work/app with spaces" "$work/tools"
cp "$source_dir/linux-launcher.sh" "$work/app with spaces/blindfb"
export BLINDFB_TEST_DIR="$work"
export PATH="$work/tools:$PATH"

cat > "$work/tools/dpkg-query" <<'EOF'
#!/bin/sh
if [ -f "$BLINDFB_TEST_DIR/installed" ]; then
    printf 'installed\n'
else
    printf 'not-installed\n'
    exit 1
fi
EOF
cat > "$work/tools/id" <<'EOF'
#!/bin/sh
printf '%s\n' "${BLINDFB_TEST_UID:-1000}"
EOF
cat > "$work/tools/pkexec" <<'EOF'
#!/bin/sh
printf 'authentication\n' >> "$BLINDFB_TEST_DIR/actions"
[ "${BLINDFB_TEST_AUTH_FAIL:-0}" = 0 ] || exit 126
exec "$@"
EOF
cat > "$work/tools/apt-get" <<'EOF'
#!/bin/sh
printf '%s\n' "$*" >> "$BLINDFB_TEST_DIR/actions"
touch "$BLINDFB_TEST_DIR/installed"
EOF
cat > "$work/app with spaces/blindfb-bin" <<'EOF'
#!/bin/sh
printf '%s\n' "$@" > "$BLINDFB_TEST_DIR/arguments"
EOF
chmod +x "$work"/tools/* "$work/app with spaces/blindfb-bin"

touch "$work/installed"
sh "$work/app with spaces/blindfb" 'argument with spaces' --flag
[ "$(cat "$work/arguments")" = "$(printf 'argument with spaces\n--flag')" ]
[ ! -f "$work/actions" ]

rm "$work/installed" "$work/arguments"
sh "$work/app with spaces/blindfb"
grep -q '^authentication$' "$work/actions"
grep -q 'gstreamer1.0-libav' "$work/actions"
[ -f "$work/arguments" ]

rm "$work/installed" "$work/arguments" "$work/actions"
if BLINDFB_TEST_AUTH_FAIL=1 sh "$work/app with spaces/blindfb" 2> "$work/error"; then
    echo 'Declined authentication unexpectedly launched BlindFB.' >&2
    exit 1
fi
[ ! -f "$work/arguments" ]
grep -q 'could not install' "$work/error"

rm "$work/actions"
BLINDFB_TEST_UID=0 sh "$work/app with spaces/blindfb"
! grep -q '^authentication$' "$work/actions"
[ -f "$work/arguments" ]
printf 'Linux launcher checks passed.\n'
