#!/usr/bin/env bash
set -euo pipefail

DEST="${XDG_DATA_HOME:-$HOME/.local/share}/numa/models"
mkdir -p "$DEST"

fetch() {
    local name="$1" url="$2"
    local target="$DEST/$name"

    if [ -s "$target" ]; then
        echo "already have $name"
        return 0
    fi

    echo "fetching $name"
    curl -fsSL -o "$target.part" "$url"

    if [ "$(head -c 8 "$target.part")" = "version " ]; then
        rm -f "$target.part"
        echo "got a Git LFS pointer instead of $name; the URL has moved" >&2
        return 1
    fi

    mv "$target.part" "$target"
}

fetch "face_detection_yunet_2023mar.onnx" \
    "https://github.com/simmmmm/Numa/releases/download/models/face_detection_yunet_2023mar.onnx"

fetch "efficientvit_seg_b2_ade20k_1024.onnx" \
    "https://github.com/simmmmm/Numa/releases/download/models/efficientvit_seg_b2_ade20k_1024.onnx"

fetch "sam_encoder.onnx" \
    "https://github.com/simmmmm/Numa/releases/download/models/sam_encoder.onnx"
fetch "sam_decoder.onnx" \
    "https://github.com/simmmmm/Numa/releases/download/models/sam_decoder.onnx"

fetch "isnet.onnx" \
    "https://github.com/simmmmm/Numa/releases/download/models/isnet.onnx"

fetch "face_recognition_sface_2021dec.onnx" \
    "https://github.com/simmmmm/Numa/releases/download/models/face_recognition_sface_2021dec.onnx"

fetch "image_classification_ppresnet50_2022jan.onnx" \
    "https://github.com/simmmmm/Numa/releases/download/models/image_classification_ppresnet50_2022jan.onnx"

echo "installed into $DEST"
