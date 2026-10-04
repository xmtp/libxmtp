# iOS validation

Use the current run's `reports/ios.md` and `proofs/ios.md` for proof status.
A source build does not prove an installed device or simulator package. The
release gate needs the generated `XmtpSdk` package, both example app builds,
callback proof, and the target performance run from the tested commit.

CI runs the test, example, and simulator recipes through `just backend ci`.
Each job starts disposable native PostgreSQL, VersityGW, and backend services.
The wrapper stops its services on success, failure, or cancellation. CI retains
service logs for 7 days. The job can be rerun on its own.
