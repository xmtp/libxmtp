set working-directory := "../../../.."
set shell := ["bash", "-eu", "-c"]

# Check the host result gate without starting a device or a backend.
check:
    python3 -m unittest discover -s sdks/android/example/performance -p 'test_*.py' -v

# Run only inside the owned, fixed emulator scope, after app bindings are staged.
device output backend:
    python3 sdks/android/example/performance/run.py --output '{{output}}' --backend '{{backend}}' --red-control
