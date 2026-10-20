# Disposable backend admission

`target.py` admits only the active disposable backend lease that
`fixtures/metadata_backend.py` creates. The integration recipe runs it before
Gradle starts, so the group settings and metadata tests reach only the owned
loopback catalogue backend. It rejects a missing lease, a lease file that is
not a private file of this user, and a lease whose backend process has exited.

`test_target.py` checks these rules with a stub process. `just test dev` runs
it; it starts no backend or device:

```bash
dev/nix-shell 'python3.11 -B -m unittest discover -s apps/example-android/performance -p "test_*.py"'
```
