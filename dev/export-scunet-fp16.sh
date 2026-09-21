#!/usr/bin/env bash
set -euo pipefail
MODELS="${1:-${XDG_DATA_HOME:-$HOME/.local/share}/numa/models}"
WORK="${WORK:-$(mktemp -d)}"
python3 -m venv "$WORK/venv"
"$WORK/venv/bin/pip" install -q onnx onnxconverter-common
"$WORK/venv/bin/python" - "$MODELS" <<'PY' 2>/dev/null
import sys, onnx
from onnxconverter_common import float16
models = sys.argv[1]
model = onnx.load(f"{models}/scunet_color_real_psnr.onnx")
half = float16.convert_float_to_float16(model, keep_io_types=True)
onnx.save_model(half, f"{models}/scunet_color_real_psnr_fp16.onnx", save_as_external_data=False)
PY
sha256sum "$MODELS/scunet_color_real_psnr_fp16.onnx"
