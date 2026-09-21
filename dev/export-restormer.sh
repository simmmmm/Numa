#!/usr/bin/env bash
set -euo pipefail
OUT="${1:-${XDG_DATA_HOME:-$HOME/.local/share}/numa/models}"
WORK="${WORK:-$(mktemp -d)}"
cd "$WORK"
python3 -m venv venv
PIP_NO_CACHE_DIR=1 venv/bin/pip install -q torch --index-url https://download.pytorch.org/whl/cpu
PIP_NO_CACHE_DIR=1 venv/bin/pip install -q onnx einops
curl -fsSL -o restormer_arch.py https://raw.githubusercontent.com/swz30/Restormer/main/basicsr/models/archs/restormer_arch.py
curl -fsSL -o motion_deblurring.pth https://github.com/swz30/Restormer/releases/download/v1.0/motion_deblurring.pth
venv/bin/python - "$OUT" <<'PY'
import sys, torch
sys.path.insert(0, '.')
from restormer_arch import Restormer
model = Restormer(inp_channels=3, out_channels=3, dim=48, num_blocks=[4, 6, 6, 8],
                  num_refinement_blocks=4, heads=[1, 2, 4, 8], ffn_expansion_factor=2.66,
                  bias=False, LayerNorm_type='WithBias', dual_pixel_task=False)
model.load_state_dict(torch.load('motion_deblurring.pth', map_location='cpu')['params'])
model.eval()
torch.onnx.export(model, torch.rand(1, 3, 256, 256), f"{sys.argv[1]}/restormer_motion_deblurring.onnx",
                  input_names=['image'], output_names=['sharp'],
                  dynamic_axes={'image': {2: 'height', 3: 'width'}, 'sharp': {2: 'height', 3: 'width'}},
                  opset_version=17, dynamo=False)
PY
sha256sum "$OUT/restormer_motion_deblurring.onnx"
