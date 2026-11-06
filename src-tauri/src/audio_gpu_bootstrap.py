"""Read-only compatibility/bootstrap for the pinned upstream Smule CLI, not an inference worker."""
import json
import os
from pathlib import Path
import runpy
import sys
import sysconfig

extras, abi, source, operation, *arguments = sys.argv[1:]
if sysconfig.get_config_var("SOABI") != abi:
    raise RuntimeError("AUDIO_GPU_INCOMPATIBLE: managed and extras CPython ABI differ")
# Import GPU Torch before exposing any CPU environment packages.
import torch

torch_file = Path(torch.__file__).resolve()
if not torch_file.is_relative_to(Path(sys.prefix).resolve()):
    raise RuntimeError("AUDIO_GPU_INCOMPATIBLE: Torch is outside the selected environment")
if not torch.version.cuda or not torch.cuda.is_available():
    raise RuntimeError("AUDIO_GPU_UNAVAILABLE: selected environment has no available CUDA Torch")
sys.path.append(str(Path(source).parent / "gpu-extras"))
sys.path.append(extras)
sys.path.insert(0, source)
import triton
import torchaudio
import librosa
import rotary_embedding_torch
from torch.nn.attention.flex_attention import flex_attention

if not Path(torchaudio.__file__).resolve().is_relative_to(Path(sys.prefix).resolve()):
    raise RuntimeError("AUDIO_GPU_INCOMPATIBLE: selected environment must supply torchaudio")
# Real Triton support is mandatory on Windows too, not a package-name assertion.
from torch.utils._triton import has_triton
if not has_triton():
    raise RuntimeError("AUDIO_GPU_INCOMPATIBLE: compatible CUDA Triton compiler is required")
if operation == "probe":
    free, total = torch.cuda.mem_get_info(0)
    print(json.dumps({"torch": str(torch_file), "version": torch.__version__,
                      "device": "cuda", "free": free, "total": total}))
elif operation == "run":
    # The pinned model selects max-autotune, which enables CUDA graphs. FlexAttention
    # in newer Torch rejects graph capture; the supported compiler setting disables it.
    import torch._inductor.config
    torch._inductor.config.triton.cudagraphs = False
    sys.argv = [os.path.join(source, "main.py"), *arguments]
    runpy.run_path(sys.argv[0], run_name="__main__")
    torch.cuda.synchronize()
    print(json.dumps({"device": "cuda", "peakAllocated": torch.cuda.max_memory_allocated(),
                      "peakReserved": torch.cuda.max_memory_reserved()}))
else:
    raise RuntimeError("AUDIO_GPU_INCOMPATIBLE: invalid bootstrap operation")
