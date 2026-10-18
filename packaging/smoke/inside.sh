#!/usr/bin/env bash
set -euo pipefail

how=$1
heading=$2
repo=$(cd "$(dirname "$0")/../.." && pwd)
raws=$(cut -f1 "$repo/packaging/smoke/raws.tsv")

trap 'chown -R "$(stat -c %u:%g /out)" /out' EXIT

stage() {
    echo "$1" > /out/stage
    echo "== $1"
}

stage "desktop baseline"
. /etc/os-release
case "$ID" in
fedora)
    dnf -y -q install sudo wget curl gnupg2 dnf5-plugins shadow-utils xorg-x11-server-Xvfb xauth dbus-daemon dejavu-sans-fonts glibc-langpack-en expect
    ;;
opensuse-tumbleweed)
    zypper -n -q install sudo wget curl gpg2 shadow xvfb-run dbus-1-daemon dejavu-fonts expect
    ;;
debian | ubuntu)
    export DEBIAN_FRONTEND=noninteractive
    apt-get -qq update
    apt-get -qq install -y sudo wget curl gpg ca-certificates xvfb xauth dbus dbus-x11 fonts-dejavu-core locales expect >/dev/null
    case "$how" in
    appimage) apt-get -qq install -y libfontconfig1 libfreetype6 libx11-6 libx11-xcb1 libxcb1 libwayland-client0 libwayland-egl1 libgl1 libegl1 libgles2 libxkbcommon0 >/dev/null ;;
    flatpak) apt-get -qq install -y flatpak >/dev/null ;;
    esac
    ;;
arch)
    pacman-key --init >/dev/null
    pacman -Syu --noconfirm --needed -q sudo wget curl gnupg xorg-server-xvfb xorg-xauth dbus ttf-dejavu expect >/dev/null
    ;;
*)
    echo "no baseline for $ID" >&2
    exit 1
    ;;
esac
mkdir -p /run/dbus
dbus-daemon --system --fork
if [[ -n "${LOCALE:-}" ]]; then
    sed -i "s/^# *\($LOCALE\)/\1/" /etc/locale.gen
    locale-gen >/dev/null
    locale -a | grep -qi "^${LOCALE%.UTF-8}.utf"
fi

stage "new user"
useradd -m photographer
echo 'photographer ALL=(ALL) NOPASSWD: ALL' > /etc/sudoers.d/photographer
as() {
    su - photographer -c "cd ~ && $1"
}
if [[ "$how" == flatpak ]]; then
    as "flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo"
fi
if [[ "$how" != readme ]]; then
    cp /bundle/* /home/photographer/
    chown photographer /home/photographer/Numa-*
fi

stage "README lines under $heading"
cp /out/readme.sh /tmp/readme.sh
start=numa
if [[ "$how" != readme ]]; then
    start=$(tail -n 1 /tmp/readme.sh)
    sed -i '$d' /tmp/readme.sh
fi
[[ "$how" == appimage ]] && start="$start --appimage-extract-and-run"
chmod 644 /tmp/readme.sh
cat /tmp/readme.sh
expect "$repo/packaging/smoke/answers.exp"

if [[ "$how" == readme ]]; then
    stage "installed package"
    if command -v pacman >/dev/null; then
        pkg=$(pacman -Qqo /usr/bin/numa)
        echo "$(pacman -Q "$pkg" | cut -d' ' -f2) $(pacman -Qi "$pkg" | sed -n 's/^Description *: //p')"
    elif command -v dpkg-query >/dev/null; then
        dpkg-query -W -f '${Version} ${binary:Summary}\n' numa
    else
        rpm -q --qf '%{VERSION} %{SUMMARY}\n' numa
    fi | tee /out/package
fi

stage "start and develop"
as "mkdir -p Pictures/smoke smoke-out && cp /raws/* Pictures/smoke/"
files=$(for raw in $raws; do printf ' ~/Pictures/smoke/%s' "$raw"; done)
set +e
as "LANG=${LOCALE:-C.UTF-8} timeout 1500 dbus-run-session -- xvfb-run -a -s '-screen 0 1920x1080x24' $start --self-test ~/smoke-out$files" 2>&1 | tee /out/self-test.log
status=${PIPESTATUS[0]}
set -e
cp /home/photographer/smoke-out/*.jpg /out/ 2>/dev/null || true
exit "$status"
