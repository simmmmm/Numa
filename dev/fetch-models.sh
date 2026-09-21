#!/usr/bin/env bash
set -euo pipefail

DEST="${XDG_DATA_HOME:-$HOME/.local/share}/numa/models"
mkdir -p "$DEST"

fetch() {
    local name="$1" url="$2" sha256="$3"
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

    if ! echo "$sha256  $target.part" | sha256sum --check --status; then
        rm -f "$target.part"
        echo "$name does not match its SHA-256; not installed" >&2
        return 1
    fi

    mv "$target.part" "$target"
}

fetch "face_detection_yunet_2023mar.onnx" \
    "https://github.com/simmmmm/Numa/releases/download/models/face_detection_yunet_2023mar.onnx" \
    8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4

fetch "efficientvit_seg_b2_ade20k_1024.onnx" \
    "https://github.com/simmmmm/Numa/releases/download/models/efficientvit_seg_b2_ade20k_1024.onnx" \
    39f11050777fe5562292ca2bfbac344515da28128d4330c6d8c514ca5d5e6efa

fetch "sam_encoder.onnx" \
    "https://github.com/simmmmm/Numa/releases/download/models/sam_encoder.onnx" \
    9f8433273a6750b587779baa0cf5508111001bf7e7acfcf585d370139fd366d0
fetch "sam_decoder.onnx" \
    "https://github.com/simmmmm/Numa/releases/download/models/sam_decoder.onnx" \
    f4514391764fbd56e08e119060d874ecd7d52994bfb1968af159e12d4943b5bb

fetch "isnet.onnx" \
    "https://github.com/simmmmm/Numa/releases/download/models/isnet.onnx" \
    60920e99c45464f2ba57bee2ad08c919a52bbf852739e96947fbb4358c0d964a

fetch "face_recognition_sface_2021dec.onnx" \
    "https://github.com/simmmmm/Numa/releases/download/models/face_recognition_sface_2021dec.onnx" \
    0ba9fbfa01b5270c96627c4ef784da859931e02f04419c829e83484087c34e79

fetch "image_classification_ppresnet50_2022jan.onnx" \
    "https://github.com/simmmmm/Numa/releases/download/models/image_classification_ppresnet50_2022jan.onnx" \
    ad5486b0de6c2171ea4d28c734c2fb7c5f64fcdbd97180a0ef515cf4b766a405

fetch "vitmatte_small.onnx" \
    "https://huggingface.co/Xenova/vitmatte-small-composition-1k/resolve/main/onnx/model.onnx" \
    bf28d2e0be2c073286e88d60ad649d7123da2749a2d99133fd1098d5887e0225

fetch "scunet_color_real_psnr.onnx" \
    "https://huggingface.co/Heliosoph/scunet-onnx/resolve/main/scunet_color_real_psnr.onnx" \
    231be201ab413dbc999d7951caa9844846b93a12a40a41e037d6b5888ed4e88c
fetch "scunet_color_real_psnr.onnx.data" \
    "https://huggingface.co/Heliosoph/scunet-onnx/resolve/main/scunet_color_real_psnr.onnx.data" \
    98825ea1210b641c71e5f052f582c70c49fd44b35387ebe2c034268c17df3feb

echo "installed into $DEST"
