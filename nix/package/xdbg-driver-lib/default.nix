# Shared Python helpers for cross-version + cross-talk drivers.
# Library only — entry points live in the test-runner packages.
{ python3Packages }:
python3Packages.buildPythonPackage {
  pname = "xdbg-driver-lib";
  version = "0.1.0";
  pyproject = true;
  src = ../../../dev/drivers/xdbg_driver_lib;
  build-system = [ python3Packages.setuptools ];
  dependencies = with python3Packages; [
    gitpython
    packaging
    rich
  ];
}
