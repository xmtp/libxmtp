#!/usr/bin/env python3
"""Check exact source and output contracts for generated reference reuse."""

import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "reference_cache", Path(__file__).with_name("docs-reference-cache.py")
)
cache = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cache)


class ReferenceCacheTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)
        self.write(".gitignore", "output/\n")
        self.write("Cargo.lock", "locked dependency\n")
        self.write("crates/example/src/lib.rs", "pub struct Example;\n")
        self.write("docs/guide.md", "Standalone guide\n")
        self.write("docs/error_glossary.md", "Current glossary\n")
        subprocess.run(["git", "add", "."], cwd=self.root, check=True)
        self.output = self.root / "output"
        self.output.mkdir()
        self.write("output/xmtp_mls/index.html", "<html>Rust reference</html>")
        self.environment = {
            "tools": [{"compiler": "rustc pinned"}],
            "flags": {},
            "runner": {"system": "Linux", "arch": "x86_64"},
        }
        audited = {
            kind: cache.reader_contract(self.root, cache.source_names(self.root), kind)
            for kind in ("rust", "kotlin", "swift")
        }
        self.audit_patch = patch.object(cache, "REVIEWED_READERS", audited)
        self.audit_patch.start()
        self.addCleanup(self.audit_patch.stop)

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def identity(self, kind="rust"):
        with patch.object(cache, "tool_identity", return_value=self.environment):
            return cache.identity(kind, self.root)

    def stamped(self):
        identity = self.identity()
        cache.stamp(self.output, identity, identity)
        return identity

    def test_prose_changes_keep_reference_key(self):
        before = self.identity()
        self.write("docs/guide.md", "Changed prose\n")
        self.write("apps/docs/src/content/docs/new-guide.mdx", "New prose\n")
        self.assertEqual(before, self.identity())

    def test_source_lock_glossary_and_generator_changes_invalidate(self):
        before = self.identity()
        for name in (
            "Cargo.lock",
            "crates/example/src/lib.rs",
            "docs/error_glossary.md",
            "apps/xmtp_sdk_bindgen/templates/example.kt",
            "nix/docs.nix",
            "dev/nix-shell",
        ):
            with self.subTest(name=name):
                path = self.root / name
                previous = path.read_text() if path.exists() else None
                self.write(name, "Changed build input\n")
                self.assertNotEqual(before, self.identity())
                if previous is None:
                    path.unlink()
                else:
                    path.write_text(previous)

    def test_embedded_markdown_changes_invalidate(self):
        for code in (
            'include_str!("../../../docs/guide.md")',
            'include_str ! ( "../../../docs/guide.md" )',
        ):
            with self.subTest(code=code):
                self.write(
                    "crates/example/src/lib.rs", f"const GUIDE: &str = {code};\n"
                )
                before = self.identity()
                self.write("docs/guide.md", code + " Changed embedded input\n")
                self.assertNotEqual(before, self.identity())

    def test_dynamic_include_uses_full_tree(self):
        self.write(
            "crates/example/src/lib.rs",
            'include_str!(concat!("../../../docs/", "guide.md"));',
        )
        before = self.identity()
        self.assertTrue(before["source"]["completeTree"])
        self.write("docs/guide.md", "Changed prose")
        self.assertNotEqual(before, self.identity())

    def test_raw_commented_and_generated_include_syntax_use_full_tree(self):
        for code in (
            'include_str!(r#"../../../docs/guide.md"#);',
            'include_str /* macro comment */ !("../../../docs/guide.md");',
            'let generated = "include_str!(\\"../../../docs/guide.md\\")";',
        ):
            with self.subTest(code=code):
                self.write("crates/example/src/lib.rs", code)
                before = self.identity()
                self.assertTrue(before["source"]["completeTree"])
                self.write("docs/guide.md", code + " Changed prose")
                self.assertNotEqual(before, self.identity())

    def test_unknown_build_script_uses_full_tree(self):
        self.write("crates/example/build.rs", "fn main() { /* unknown input reads */ }")
        before = self.identity()
        self.assertTrue(before["source"]["completeTree"])
        self.write("docs/guide.md", "Changed prose")
        self.assertNotEqual(before, self.identity())

    def test_source_delete_and_link_target_changes_invalidate(self):
        before = self.identity()
        (self.root / "crates/example/src/lib.rs").unlink()
        self.assertNotEqual(before, self.identity())
        self.write("crates/example/src/lib.rs", "Original source\n")
        (self.root / "linked-source").symlink_to("crates/example/src/lib.rs")
        before = self.identity()
        self.write("crates/example/src/lib.rs", "Changed linked source\n")
        self.assertNotEqual(before, self.identity())

    def test_source_links_cannot_escape(self):
        (self.root / "external-source").symlink_to("/etc/hosts")
        with self.assertRaisesRegex(ValueError, "escapes"):
            self.identity()

    def test_verified_output_matches_current_source(self):
        identity = self.stamped()
        cache.verify(self.output, identity)
        record = json.loads((self.output / cache.RECEIPT).read_text())
        self.assertEqual(
            record["files"]["xmtp_mls/index.html"],
            cache.digest(b"<html>Rust reference</html>"),
        )

    def test_changed_missing_or_extra_output_fails(self):
        identity = self.stamped()
        index = self.output / "xmtp_mls/index.html"
        index.write_text("tampered output")
        with self.assertRaisesRegex(ValueError, "bytes changed"):
            cache.verify(self.output, identity)
        index.unlink()
        with self.assertRaisesRegex(ValueError, "entrypoint"):
            cache.verify(self.output, identity)
        index.write_text("<html>Rust reference</html>")
        self.write("output/extra.js", "unexpected file")
        with self.assertRaisesRegex(ValueError, "bytes changed"):
            cache.verify(self.output, identity)

    def test_tool_runner_or_current_source_mismatch_fails(self):
        identity = self.stamped()
        for area in ("tools", "runner"):
            with self.subTest(area=area):
                altered = copy.deepcopy(identity)
                altered["environment"][area] = "different"
                with self.assertRaisesRegex(ValueError, "current inputs"):
                    cache.verify(self.output, altered)
        self.write("Cargo.lock", "New dependency\n")
        with self.assertRaisesRegex(ValueError, "current inputs"):
            cache.verify(self.output, self.identity())

    def test_output_symlinks_and_missing_receipt_fail(self):
        identity = self.stamped()
        (self.output / "asset.js").symlink_to("xmtp_mls/index.html")
        with self.assertRaisesRegex(ValueError, "symbolic link"):
            cache.verify(self.output, identity)
        (self.output / cache.RECEIPT).unlink()
        with self.assertRaisesRegex(ValueError, "receipt"):
            cache.verify(self.output, identity)

    def test_changed_inputs_during_generation_fail(self):
        before = self.identity()
        self.write("Cargo.lock", "Changed while generating\n")
        with self.assertRaisesRegex(ValueError, "changed during"):
            cache.stamp(self.output, before, self.identity())

    def test_native_entrypoints_are_required(self):
        for kind in ("swift", "kotlin"):
            with self.subTest(kind=kind):
                identity = self.identity(kind)
                with self.assertRaisesRegex(ValueError, "entrypoint"):
                    cache.stamp(self.output, identity, identity)

    def test_sdk_source_families_change_only_their_reference(self):
        self.write("sdks/agent/src/index.ts", "export class Agent {}\n")
        self.write("sdks/ios/Sources/Client.swift", "public struct Client {}\n")
        self.write("sdks/android/library/src/main/Client.kt", "class Client {}\n")
        before = {kind: self.identity(kind) for kind in ("rust", "kotlin", "swift")}
        self.write("sdks/agent/src/index.ts", "export class ChangedAgent {}\n")
        self.write("sdks/agent/src/new.ts", "export const newSource = true;\n")
        for kind in before:
            self.assertEqual(before[kind], self.identity(kind))
        self.write("sdks/ios/Sources/Client.swift", "public struct ChangedClient {}\n")
        self.assertEqual(before["rust"], self.identity("rust"))
        self.assertEqual(before["kotlin"], self.identity("kotlin"))
        self.assertNotEqual(before["swift"], self.identity("swift"))
        self.write(
            "sdks/android/library/src/main/Client.kt", "class ChangedClient {}\n"
        )
        self.assertEqual(before["rust"], self.identity("rust"))
        self.assertNotEqual(before["kotlin"], self.identity("kotlin"))

    def test_literal_embedded_excluded_and_ignored_files_are_inputs(self):
        self.write(".gitignore", "output/\nignored/\n")
        for name in ("sdks/agent/src/index.ts", "ignored/embedded.ts"):
            with self.subTest(name=name):
                self.write(name, "First embedded source\n")
                self.write(
                    "crates/example/src/lib.rs",
                    f'const SOURCE: &str = include_str!("../../../{name}");\n',
                )
                before = self.identity()
                self.write(name, "Changed embedded source\n")
                self.assertNotEqual(before, self.identity())

    def test_changed_macro_reader_cannot_reuse_prose_cache(self):
        self.write(
            "crates/reader/Cargo.toml",
            '[package]\nname="reader"\nversion="0.1.0"\n[lib]\nproc-macro=true\n',
        )
        self.write("crates/reader/src/lib.rs", "fn no_file_reads() {}\n")
        cache.REVIEWED_READERS["rust"] = cache.reader_contract(
            self.root, cache.source_names(self.root), "rust"
        )
        self.assertTrue(self.identity()["cacheEligible"])
        self.write(
            "crates/reader/src/lib.rs",
            'fn read_doc() { let _ = std::fs::read_to_string("docs/guide.md"); }\n',
        )
        alpha = self.identity()
        self.assertTrue(alpha["source"]["completeTree"])
        self.assertFalse(alpha["cacheEligible"])
        cache.stamp(self.output, alpha, alpha)
        self.write("docs/guide.md", "Beta compiler-read document\n")
        beta = self.identity()
        self.assertNotEqual(alpha["key"], beta["key"])
        with self.assertRaisesRegex(ValueError, "current inputs"):
            cache.verify(self.output, beta)

    def test_new_reader_and_job_commands_disable_cache_reuse(self):
        self.write("dev/kache-env", "export RUSTC_WRAPPER=kache\n")
        self.write("dev/kache-darwin-wrapper", 'exec kache "$@"\n')
        for kind in ("rust", "kotlin", "swift"):
            cache.REVIEWED_READERS[kind] = cache.reader_contract(
                self.root, cache.source_names(self.root), kind
            )
            self.assertTrue(self.identity(kind)["cacheEligible"])
        for name, text in (
            ("dev/kache-env", 'export RUSTFLAGS="$(cat /tmp/compiler-flags)"\n'),
            ("dev/kache-darwin-wrapper", 'exec "$(cat /tmp/compiler-command)" "$@"\n'),
            (
                "crates/reader/Cargo.toml",
                '[package]\nname="reader"\nversion="0.1.0"\n[lib]\nproc-macro=true\n',
            ),
            (".github/workflows/deploy-docs.yml", "new reference reader command\n"),
            ("nix/source-reader.nix", "new compiler input selector\n"),
        ):
            with self.subTest(name=name):
                path = self.root / name
                previous = path.read_text() if path.exists() else None
                self.write(name, text)
                for kind in ("rust", "kotlin", "swift"):
                    self.assertFalse(self.identity(kind)["cacheEligible"])
                current = self.identity()
                cache.stamp(self.output, current, current)
                with self.assertRaisesRegex(ValueError, "not eligible"):
                    cache.verify(self.output, current)
                if previous is None:
                    path.unlink()
                else:
                    path.write_text(previous)

    def test_unchanged_file_flag_string_cannot_reuse_changed_header(self):
        with tempfile.TemporaryDirectory() as directory:
            header = Path(directory) / "header.html"
            header.write_text("Alpha header")
            self.environment["flags"]["RUSTDOCFLAGS"] = f"--html-in-header {header}"
            alpha = self.identity()
            self.assertFalse(alpha["cacheEligible"])
            cache.stamp(self.output, alpha, alpha)
            header.write_text("Beta header")
            beta = self.identity()
            self.assertFalse(beta["cacheEligible"])
            with self.assertRaisesRegex(ValueError, "not eligible"):
                cache.verify(self.output, beta)

    def test_custom_sysroot_and_java_agent_flags_are_not_cacheable(self):
        for name, value, kind in (
            ("SDKROOT", "/tmp/mutable-sdk", "rust"),
            ("CFLAGS", "-include /tmp/mutable.h", "rust"),
            ("RUSTFLAGS", "@/tmp/compiler-response", "rust"),
            ("JAVA_TOOL_OPTIONS", "-javaagent:/tmp/mutable.jar", "kotlin"),
            ("GRADLE_OPTS", "-javaagent:/tmp/mutable.jar", "kotlin"),
            ("NIX_CFLAGS_COMPILE", "-include /tmp/mutable.h", "rust"),
        ):
            with self.subTest(name=name):
                self.environment["flags"] = {name: value}
                self.assertFalse(self.identity(kind)["cacheEligible"])

    def test_mutable_compiler_override_bytes_change_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            compiler = Path(directory) / "compiler-override"
            compiler.write_text("Alpha compiler")
            self.environment["flags"] = {"CC": str(compiler)}
            self.environment["toolInputs"] = cache.compiler_tool_inputs(
                self.environment["flags"]
            )
            alpha = self.identity()
            self.assertFalse(alpha["cacheEligible"])
            compiler.write_text("Beta compiler")
            self.environment["toolInputs"] = cache.compiler_tool_inputs(
                self.environment["flags"]
            )
            beta = self.identity()
            self.assertNotEqual(alpha["key"], beta["key"])

    def test_pinned_nix_stock_flags_allow_cache_reuse(self):
        store = "/nix/store/" + "a" * 32 + "-compiler"
        self.environment["flags"] = {
            "CC": store + "/bin/cc",
            "SDKROOT": store + "/sdk",
            "OPENSSL_DIR": store + "/openssl",
            "OPENSSL_NO_VENDOR": "1",
            "CFLAGS_wasm32_unknown_unknown": f"-I {store}/lib/clang/21/include",
            "MACOSX_DEPLOYMENT_TARGET": "14.0",
            "CI": "true",
            "NIX_CFLAGS_COMPILE": f"-frandom-seed=abc123 -isystem {store}/include -fmacro-prefix-map={store}={store}/canonical -O2",
        }
        self.environment["toolInputs"] = {
            "CC": {"path": store + "/bin/cc", "sha256": "compiler bytes"}
        }
        self.assertTrue(self.identity()["cacheEligible"])

    def test_mismatch_diagnostics_report_paths_and_redact_flags(self):
        state = self.root / "output/state.json"
        secret = "DIAGNOSTIC_PRIVATE_SENTINEL"
        self.environment["flags"] = {"RUSTDOCFLAGS": secret}
        self.environment["tools"][0]["stderr"] = secret
        before = self.identity("swift")
        with patch.object(cache, "ROOT", self.root):
            cache.write_baseline(state, before)
            self.write(
                "sdks/ios/Sources/XmtpSdk/xmtp_sdk.swift", "changed generated input"
            )
            after = self.identity("swift")
            cache.write_mismatch(state, before, after)
        report = json.loads(Path(str(state) + ".failure-safe.json").read_text())
        self.assertIn("source", report["components"])
        self.assertIn(
            "sdks/ios/Sources/XmtpSdk/xmtp_sdk.swift", report["changedSourcePaths"]
        )
        for path in self.root.glob("output/state.json.*.json"):
            self.assertNotIn(secret, path.read_text())


if __name__ == "__main__":
    unittest.main()
