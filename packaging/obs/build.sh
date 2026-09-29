#!/bin/sh
set -eu

case "$1" in
build)
    ORT_LIB_PATH="$PWD/onnxruntime/lib" ORT_PREFER_DYNAMIC_LINK=1 \
    RUSTFLAGS="-C link-arg=-Wl,-rpath,/usr/lib/numa" \
        cargo build --profile dist --offline --locked
    ;;
install)
    d="$2"
    install -Dm755 target/dist/numa "$d/usr/bin/numa"
    install -d "$d/usr/lib/numa"
    cp -P onnxruntime/lib/libonnxruntime.so.* "$d/usr/lib/numa/"
    install -Dm644 -t "$d/usr/share/numa/lensfun" data/lensfun/*
    install -Dm644 -t "$d/usr/share/numa/profiles/numa" data/private-profiles/*
    install -Dm644 data/com.tijmen.Numa.desktop "$d/usr/share/applications/com.tijmen.Numa.desktop"
    install -Dm644 data/com.tijmen.Numa.metainfo.xml "$d/usr/share/metainfo/com.tijmen.Numa.metainfo.xml"
    for size in 32 48 64 128 256; do
        for name in com.tijmen.Numa com.tijmen.Numa-dark; do
            install -Dm644 "data/icons/hicolor/${size}x${size}/apps/$name.png" \
                "$d/usr/share/icons/hicolor/${size}x${size}/apps/$name.png"
        done
    done
    install -Dm644 -t "$d/usr/share/icons/hicolor/scalable/actions" data/icons/hicolor/scalable/actions/*.svg
    install -Dm644 -t "$d/usr/share/licenses/numa" LICENSE data/THIRD_PARTY_LICENSES.txt data/MODELS-LICENSES.txt
    install -Dm644 -t "$d/usr/share/licenses/numa/onnxruntime" onnxruntime/LICENSE onnxruntime/ThirdPartyNotices.txt
    ;;
*)
    echo "usage: $0 build | install DESTDIR" >&2
    exit 2
    ;;
esac
