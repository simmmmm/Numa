#!/usr/bin/env bash
set -euo pipefail
MODEL="${1:-b2}"
RES="${2:-1024}"
WORK="${WORK:-$(mktemp -d)}"
cd "$WORK"

python3 -m venv venv
. venv/bin/activate
pip install -q --upgrade pip
pip install -q torch torchvision --index-url https://download.pytorch.org/whl/cpu
pip install -q onnx onnxsim onnxscript timm omegaconf einops opencv-python-headless pyyaml tqdm scipy huggingface_hub

[ -d repo ] || git clone -q --depth 1 https://github.com/mit-han-lab/efficientvit.git repo
cd repo

sed -i 's/^from \.sam import \*/# from .sam import *  # not needed for the seg export/' efficientvit/models/efficientvit/__init__.py
sed -i 's/^from efficientvit\.sam_model_zoo.*/# &/' assets/onnx_export.py
python - <<'PY'
p='efficientvit/models/nn/norm.py'; s=open(p).read()
s=s.replace('from efficientvit.models.nn.triton_rms_norm import TritonRMSNorm2dFunc','''try:
    from efficientvit.models.nn.triton_rms_norm import TritonRMSNorm2dFunc
except ModuleNotFoundError:
    TritonRMSNorm2dFunc = None''',1)
open(p,'w').write(s)
PY
sed -i 's/create_efficientvit_seg_model(name=args.model, pretrained=False)/create_efficientvit_seg_model(name=args.model, pretrained=True)/' assets/onnx_export.py
python - <<'PY'
p='efficientvit/apps/utils/export.py'; s=open(p).read()
if 'dynamo=' not in s:
    s=s.replace('torch.onnx.export(model, sample_inputs, buffer, opset_version=opset)','torch.onnx.export(model, sample_inputs, buffer, opset_version=opset, dynamo=False)',1)
open(p,'w').write(s)
PY

mkdir -p assets/checkpoints/efficientvit_seg
CKPT="assets/checkpoints/efficientvit_seg/efficientvit_seg_${MODEL}_ade20k.pt"
[ -f "$CKPT" ] || curl -sL -o "$CKPT" "https://huggingface.co/han-cai/efficientvit-seg/resolve/main/efficientvit_seg_${MODEL}_ade20k.pt"

OUT="$WORK/efficientvit_seg_${MODEL}_ade20k_${RES}.onnx"
python assets/onnx_export.py --export_path "$OUT" --task seg --model "efficientvit-seg-${MODEL}-ade20k" --resolution "$RES" "$RES" --bs 1 --op_set 17
echo "exported $OUT"
