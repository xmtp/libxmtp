# Unsupported-schema negative fixture

`unsupported-schema.db3` and its salt are a deliberately mutated copy of the
genuine `react-native-5.7.0.db3` fixture. They are not unmodified SDK output.
The database key is the same valid key in `expected-records.json`.

The only logical change is the insertion in `unsupported-schema.sql`: migration
version `999912312359590000`, with a fixed synthetic `run_on` value. The resulting
inventory has 63 rows. Its last version does not match the converter's accepted
63rd migration. Application history, consent, schema definitions, and all other
stored rows remain unchanged. This case tests schema rejection separately from
wrong-key rejection.

`unsupported-schema.json` records the mutation, database and salt hashes,
sidecar absence, and positive fixture hashes before and after. SQLCipher 4.17.0
opened only temporary copies. Full logical dumps matched after removing only
INSERT lines for `__diesel_schema_migrations`. Integrity checks passed.
The negative source has no WAL, SHM, or rollback journal after close.

The one-off host check called the staged Node SDK's public
`prepareMigrationArchive`, which executes the native Rust converter. It passed
`error instanceof XmtpError.MigrationUnsupportedSchema` and checked
`error.details.code === "MigrationUnsupportedSchema"`, storage category, and
`retryable === false`. No archive was published. Main database, salt, and sidecar
absence stayed unchanged for both positive and negative fixtures. The receipt
pins the actual native library hash. Installed RN execution on iOS and Android
belongs to the conformance journey.

## Reproduce the mutation

Run from the repository root through the current Nix shell. Use an absent output
directory. This opens only the new derivative through SQLCipher; the committed
positive source is copied as bytes.

```sh
dev/nix-shell --command python3 - <<'PY'
import json
from pathlib import Path
import shutil
import subprocess

fixture = Path.cwd() / 'sdks/react-native/test/conformance/legacy'
output = Path('/private/tmp/rn-legacy-unsupported-reproduce')
output.mkdir()
source = fixture / 'react-native-5.7.0.db3'
negative = output / 'unsupported-schema.db3'
for suffix in ['', '.sqlcipher_salt']:
    shutil.copyfile(Path(str(source) + suffix), Path(str(negative) + suffix))
key = json.loads((fixture / 'expected-records.json').read_text())['databaseKeyHex']
salt = Path(str(negative) + '.sqlcipher_salt').read_text().strip()
settings = (
    f'PRAGMA key="x\'{key}\'";\n'
    'PRAGMA cipher_plaintext_header_size=32;\n'
    f'PRAGMA cipher_salt="x\'{salt}\'";\n'
)
subprocess.run(
    ['sqlcipher', str(negative)],
    input=settings + (fixture / 'unsupported-schema.sql').read_text(),
    text=True,
    check=True,
)
PY
```

SQLCipher writes fresh page IVs, so a regenerated encrypted database can have a
new byte hash. Keep its database, salt, mutation, and recorded hashes together.
Never regenerate it with current SDK storage initialization.
