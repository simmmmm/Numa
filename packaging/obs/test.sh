#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
IN="$ROOT/target/obs"
[[ -f "$IN/numa.spec" ]] || { echo "Run packaging/obs/sources.sh first." >&2; exit 1; }

declare -A IMAGE=(
    [fedora]=fedora:latest
    [tumbleweed]=opensuse/tumbleweed
    [debian]=debian:testing
    [ubuntu]=ubuntu:26.04
    [arch]=archlinux:latest
)
RPM_BUILD='rpmbuild --define "_sourcedir /in" --define "_rpmdir /tmp/rpms" -bb /in/numa.spec &&
    cp /tmp/rpms/*/*.rpm /out/ && rpm -i /out/*.rpm'
DEB_DEPS='apt-get update && apt-get -y install --no-install-recommends obs-build dpkg-dev xz-utils \
    libvulkan1 hicolor-icon-theme && mkdir -p /d/debian && cp /in/debian.control /d/debian/control &&
    apt-get -y build-dep /d'
DEB_BUILD='mkdir /t && PATH=/usr/lib/obs-build:$PATH debtransform /in /in/numa.dsc /t && cd /t &&
    dpkg-source -x numa_*.dsc src && cd src && dpkg-buildpackage -b -uc -us &&
    cp ../*.deb /out/ && dpkg -i /out/*.deb'
declare -A DEPS=(
    [fedora]='dnf -y install rpm-build dnf-plugins-core vulkan-loader && dnf -y builddep /in/numa.spec'
    [tumbleweed]='zypper -n in rpm-build rpmlint libvulkan1 && zypper -n in $(rpmspec -q --buildrequires /in/numa.spec | sed "s/ .*//")'
    [debian]="$DEB_DEPS"
    [ubuntu]="$DEB_DEPS"
    [arch]='pacman -Syu --noconfirm --needed base-devel rust openssl gtk4 libadwaita vulkan-icd-loader hicolor-icon-theme'
)
declare -A BUILD=(
    [fedora]="$RPM_BUILD"
    [tumbleweed]="$RPM_BUILD && rpmlint /out/*.rpm"
    [debian]="$DEB_BUILD"
    [ubuntu]="$DEB_BUILD"
    [arch]='useradd -m b && cp /in/numa-*.tar.xz /in/PKGBUILD /home/b/ && chown -R b /home/b &&
        su b -c "cd ~ && makepkg --nodeps" && cp /home/b/*.pkg.tar.zst /out/ && pacman -U --noconfirm /out/*.pkg.tar.zst'
)

for distro in "${@:-fedora tumbleweed debian ubuntu arch}"; do
  for d in $distro; do
    out="$ROOT/target/obs-test/$d"
    rm -rf "$out" && mkdir -p "$out"
    echo "== $d"
    docker rm -f "numa-obs-$d" >/dev/null 2>&1 || true
    if ! docker run --name "numa-obs-$d" -v "$IN:/in:ro" "${IMAGE[$d]}" sh -c "${DEPS[$d]}" > "$out/deps.log" 2>&1; then
        echo "   dependencies FAILED, see $out/deps.log"; tail -5 "$out/deps.log"; continue
    fi
    docker commit "numa-obs-$d" "numa-obs-$d" >/dev/null && docker rm "numa-obs-$d" >/dev/null
    if docker run --rm --network none --memory 6g --memory-swap 6g -e CARGO_BUILD_JOBS=4 \
        -v "$IN:/in:ro" -v "$out:/out" "numa-obs-$d" \
        sh -c "${BUILD[$d]} && ! ldd /usr/bin/numa | grep 'not found' && ldd /usr/bin/numa | grep onnxruntime" \
        > "$out/build.log" 2>&1; then
        echo "   ok: $(cd "$out" && ls *.rpm *.deb *.pkg.tar.zst 2>/dev/null | tr '\n' ' ')"
    else
        echo "   FAILED, see $out/build.log"; tail -5 "$out/build.log"
    fi
  done
done
