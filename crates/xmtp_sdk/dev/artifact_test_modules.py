"""Load the production SDK tools once for artifact test fixtures."""

import importlib.util
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "artifacts", Path(__file__).with_name("sdk-artifacts.py")
)
artifacts = importlib.util.module_from_spec(spec)
spec.loader.exec_module(artifacts)

mobile_spec = importlib.util.spec_from_file_location(
    "mobile", Path(__file__).with_name("mobile-package.py")
)
mobile = importlib.util.module_from_spec(mobile_spec)
mobile_spec.loader.exec_module(mobile)
