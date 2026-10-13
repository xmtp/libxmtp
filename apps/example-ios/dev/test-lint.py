#!/usr/bin/env python3
"""Prove the architecture checks reject forbidden imports and settings."""
from pathlib import Path
import subprocess
import sys
import tempfile

root = Path(__file__).resolve().parents[1]
lint = [sys.executable, str(root / "dev/lint.py")]
subprocess.run(lint, check=True)
app = root / "ExampleApp"
app.mkdir(exist_ok=True)
with tempfile.NamedTemporaryFile(mode="w", suffix=".swift", dir=app) as source:
    source.write("import XmtpSdk\n")
    source.flush()
    result = subprocess.run(lint, capture_output=True, text=True)
    assert result.returncode != 0 and "only ExampleSDK" in result.stderr, result
config = root / "Config/Base.xcconfig"
original = config.read_text()
try:
    config.write_text(original.replace("SWIFT_STRICT_CONCURRENCY = complete", "SWIFT_STRICT_CONCURRENCY = minimal"))
    result = subprocess.run(lint, capture_output=True, text=True)
    assert result.returncode != 0 and "SWIFT_STRICT_CONCURRENCY" in result.stderr, result
finally:
    config.write_text(original)
subprocess.run(lint, check=True)
print("Architecture mutation checks passed.")
