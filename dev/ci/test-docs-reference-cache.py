#!/usr/bin/env python3
"""Check exact source and output contracts for generated reference reuse."""

import copy
import importlib.util
import json
import os
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
        environment = patch.dict(
            os.environ,
            {
                "CARGO_HOME": str(self.root / "cargo-home"),
                "GRADLE_USER_HOME": str(self.root / "gradle-home"),
            },
        )
        environment.start()
        self.addCleanup(environment.stop)
        self.write(cache.POLICY_PATH, Path(cache.__file__).read_text())
        self.write(cache.CONFIG_READER, "# Audited configuration reader\n")
        self.write(
            "crates/xmtp_sdk/dev/record-generated.py", "# Audited provenance reader\n"
        )
        self.write(
            "crates/xmtp_sdk/dev/sdk-artifacts.py", "# Audited artifact reader\n"
        )
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
            self.assertTrue(self.identity(kind)["source"]["cacheEligible"])
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

    def test_policy_normalization_preserves_all_code_except_literal_pins(self):
        source = (self.root / cache.POLICY_PATH).read_bytes()
        normalized = cache.normalized_policy(source)
        changed_pins = source
        assignment = next(
            node
            for node in cache.ast.parse(source.decode()).body
            if isinstance(node, cache.ast.Assign)
            and isinstance(node.targets[0], cache.ast.Name)
            and node.targets[0].id == "REVIEWED_READERS"
        )
        for value in cache.ast.literal_eval(assignment.value).values():
            changed_pins = changed_pins.replace(value.encode(), b"new-reviewed-pin")
        self.assertNotEqual(source, changed_pins)
        self.assertEqual(normalized, cache.normalized_policy(changed_pins))
        lines = source.splitlines(keepends=True)
        lines[assignment.end_lineno - 1] = (
            lines[assignment.end_lineno - 1].rstrip(b"\n")
            + b"; external = Path('/tmp/unlisted').read_text()\n"
        )
        self.assertNotEqual(normalized, cache.normalized_policy(b"".join(lines)))
        self.assertNotEqual(
            normalized,
            cache.normalized_policy(
                source
                + b"\ndef read_external():\n    return Path('/tmp/unlisted').read_text()\n"
            ),
        )
        with self.assertRaises(ValueError):
            cache.normalized_policy(b"REVIEWED_READERS = dict(rust='dynamic')\n")

    def test_changed_policy_and_native_provenance_block_exact_new_receipts(self):
        self.write("output/index.html", "<html>Native reference</html>")
        self.write(
            "output/documentation/xmtpsdk/index.html", "<html>Swift reference</html>"
        )
        for name in (
            cache.POLICY_PATH,
            cache.CONFIG_READER,
            "crates/xmtp_sdk/dev/record-generated.py",
            "crates/xmtp_sdk/dev/sdk-artifacts.py",
        ):
            path = self.root / name
            original = path.read_text()
            kinds = (
                ("rust", "kotlin", "swift")
                if name in (cache.POLICY_PATH, cache.CONFIG_READER)
                else ("kotlin", "swift")
            )
            with self.subTest(reader=name):
                path.write_text(
                    original
                    + "\ndef read_external():\n    return Path('/tmp/unlisted').read_text()\n"
                )
                for kind in kinds:
                    current = self.identity(kind)
                    self.assertFalse(current["source"]["cacheEligible"])
                    cache.stamp(self.output, current, current)
                    with self.assertRaisesRegex(ValueError, "not eligible"):
                        cache.verify(self.output, current)
                path.write_text(original)

    def test_clean_supported_cargo_inputs_permit_rust_reuse(self):
        self.write(
            ".cargo/config.toml",
            '[target."cfg(all())"]\nrustflags=["--cfg", "tracing_unstable"]\n',
        )
        cache.REVIEWED_READERS["rust"] = cache.reader_contract(
            self.root, cache.source_names(self.root), "rust"
        )
        current = self.stamped()
        self.assertTrue(current["cacheEligible"])
        cache.verify(self.output, current)

    def test_prior_schema_receipt_cannot_be_promoted(self):
        current = self.stamped()
        path = self.output / cache.RECEIPT
        receipt = json.loads(path.read_text())
        receipt["identity"]["schema"] = 2
        path.write_text(json.dumps(receipt))
        with self.assertRaisesRegex(ValueError, "current inputs"):
            cache.verify(self.output, current)

    def test_external_cargo_config_and_header_cannot_reuse(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            header = home / "header.html"
            header.write_text("Alpha")
            config = home / "config.toml"
            config.write_text(
                f'[build]\nrustdocflags=["--html-in-header", "{header}"]\n'
            )
            with patch.dict(os.environ, {"CARGO_HOME": str(home)}):
                alpha = self.identity()
                self.assertFalse(alpha["cacheEligible"])
                cache.stamp(self.output, alpha, alpha)
                header.write_text("Beta")
                with self.assertRaisesRegex(ValueError, "not eligible"):
                    cache.verify(self.output, self.identity())
                config.write_text(
                    '[build]\nrustdocflags=["--document-private-items"]\n'
                )
                self.assertNotEqual(alpha["key"], self.identity()["key"])

    def test_workspace_cargo_file_flags_and_external_includes_are_ineligible(self):
        for text in (
            '[build]\nrustdocflags=["--html-in-header", "/tmp/header.html"]\n',
            'include=["../../outside.toml"]\n',
            '[env]\nCI="true"\n',
        ):
            with self.subTest(config=text):
                self.write(".cargo/config.toml", text)
                cache.REVIEWED_READERS["rust"] = cache.reader_contract(
                    self.root, cache.source_names(self.root), "rust"
                )
                current = self.identity()
                self.assertFalse(current["cacheEligible"])
                cache.stamp(self.output, current, current)
                with self.assertRaisesRegex(ValueError, "not eligible"):
                    cache.verify(self.output, current)

    def test_ignored_glossary_source_disables_reuse_and_binds_its_bytes(self):
        before = self.stamped()
        self.write("crates/example/.gitignore", "ignored.rs\n")
        self.write("crates/example/ignored.rs", "pub enum IgnoredError { Alpha }\n")
        current = self.identity()
        self.assertFalse(current["cacheEligible"])
        self.assertNotEqual(before["key"], current["key"])
        cache.stamp(self.output, current, current)
        with self.assertRaisesRegex(ValueError, "not eligible"):
            cache.verify(self.output, current)
        self.write("crates/example/ignored.rs", "pub enum IgnoredError { Beta }\n")
        self.assertNotEqual(current["key"], self.identity()["key"])

    def test_global_gradle_inputs_and_unqualified_native_tools_cannot_reuse(self):
        self.write("output/index.html", "<html>Kotlin reference</html>")
        for name in (
            "gradle.properties",
            "init.gradle",
            "init.gradle.kts",
            "init.d/custom.gradle",
        ):
            with self.subTest(input=name):
                self.write("gradle-home/" + name, "external Gradle reader\n")
                current = self.identity("kotlin")
                self.assertFalse(current["cacheEligible"])
                self.assertIn(
                    "globalGradleConfig", current["ambientContract"]["unreviewed"]
                )
                cache.stamp(self.output, current, current)
                self.write("gradle-home/" + name, "changed external Gradle reader\n")
                changed = self.identity("kotlin")
                self.assertNotEqual(current["key"], changed["key"])
                with self.assertRaisesRegex(ValueError, "current inputs"):
                    cache.verify(self.output, changed)
                cache.stamp(self.output, changed, changed)
                with self.assertRaisesRegex(ValueError, "not eligible"):
                    cache.verify(self.output, changed)
        self.assertFalse(self.identity("swift")["cacheEligible"])

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

    def test_all_cargo_rustdoc_environment_routes_reject_external_files(self):
        for name in (
            "RUSTDOCFLAGS",
            "CARGO_ENCODED_RUSTDOCFLAGS",
            "CARGO_BUILD_RUSTDOCFLAGS",
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTDOCFLAGS",
        ):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as directory:
                header = Path(directory) / "header.html"
                header.write_text("Alpha")
                separator = "\x1f" if name == "CARGO_ENCODED_RUSTDOCFLAGS" else " "
                flag = separator.join(("--html-in-header", str(header)))
                with (
                    patch.dict(os.environ, {name: flag}, clear=True),
                    patch.object(cache, "probe", return_value={"compiler": "pinned"}),
                ):
                    environment = cache.tool_identity("rust", self.root)
                self.assertIn(name, environment["flags"])
                self.assertEqual(environment["flags"][name], flag)
                with patch.object(cache, "tool_identity", return_value=environment):
                    alpha = cache.identity("rust", self.root)
                    self.assertFalse(alpha["cacheEligible"])
                    cache.stamp(self.output, alpha, alpha)
                    header.write_text("Beta")
                    with self.assertRaisesRegex(ValueError, "not eligible"):
                        cache.verify(self.output, cache.identity("rust", self.root))

    def test_unknown_cargo_build_environment_routes_are_not_silent(self):
        with (
            patch.dict(
                os.environ, {"CARGO_BUILD_FUTURE_FILE_OPTION": "/tmp/input"}, clear=True
            ),
            patch.object(cache, "probe", return_value={"compiler": "pinned"}),
        ):
            environment = cache.tool_identity("rust", self.root)
        self.assertIn("CARGO_BUILD_FUTURE_FILE_OPTION", environment["flags"])
        self.assertEqual(
            environment["flags"]["CARGO_BUILD_FUTURE_FILE_OPTION"], "/tmp/input"
        )
        self.assertFalse(cache.flag_contract("rust", environment)["cacheEligible"])

    def test_unknown_wrappers_stay_ineligible(self):
        for wrapper in (
            "/tmp/wrapper",
            "/nix/store/" + "a" * 32 + "-unreviewed/bin/wrapper",
        ):
            with self.subTest(wrapper=wrapper):
                self.environment["flags"] = {"RUSTC_WRAPPER": wrapper}
                self.assertFalse(self.identity()["cacheEligible"])

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

    def linux_link_environment(self):
        store = "/nix/store/" + "a" * 32 + "-compiler"
        linker = "/nix/store/" + "b" * 32 + "-mold/bin/mold"
        self.environment["flags"] = {
            "CC": store + "/bin/clang",
            "NIX_BINTOOLS": store + "/bintools",
            "NIX_CFLAGS_LINK": " -fuse-ld=mold",
            "NIX_LDFLAGS": f"-rpath {store}/lib -L{store}/lib",
        }
        self.environment["toolInputs"] = {
            "CC": {"path": store + "/bin/clang", "sha256": "compiler bytes"},
            "NIX_BINTOOLS": {"path": linker, "sha256": "linker bytes"},
        }

    def test_linux_mold_and_store_rpath_allow_exact_reference_reuse(self):
        self.linux_link_environment()
        alpha = self.identity()
        self.assertTrue(alpha["cacheEligible"])
        cache.stamp(self.output, alpha, alpha)
        cache.verify(self.output, alpha)
        self.environment["toolInputs"]["NIX_BINTOOLS"]["sha256"] = "changed linker"
        beta = self.identity()
        self.assertNotEqual(alpha["key"], beta["key"])
        with self.assertRaises(ValueError):
            cache.verify(self.output, beta)

    def test_linux_link_flags_reject_external_and_unreviewed_forms(self):
        for name, value in (
            ("NIX_LDFLAGS", "-rpath /tmp/library/lib"),
            ("NIX_LDFLAGS", "-rpath relative/lib"),
            ("NIX_LDFLAGS", "-rpath"),
            ("NIX_LDFLAGS", '-rpath "'),
            ("NIX_LDFLAGS", "@/tmp/link-response"),
            ("NIX_LDFLAGS", "-T /tmp/link-script"),
            ("NIX_LDFLAGS", "-rpath /nix/store/" + "a" * 32 + "-library/../../tmp/lib"),
            ("NIX_LDFLAGS", "-rpath /nix/store/" + "a" * 32 + "-library/share"),
            ("NIX_CFLAGS_LINK", "-fuse-ld=lld"),
            ("NIX_CFLAGS_LINK", '-fuse-ld=mold "'),
            ("NIX_CFLAGS_LINK", "-fuse-ld=mold -T /tmp/link-script"),
        ):
            with self.subTest(name=name, value=value):
                self.linux_link_environment()
                self.environment["flags"][name] = value
                self.assertFalse(self.identity()["cacheEligible"])

    def test_mold_requires_immutable_compiler_and_linker_records(self):
        for name, field, value in (
            ("CC", "path", "/tmp/compiler"),
            ("NIX_BINTOOLS", "path", "/tmp/mold"),
            ("NIX_BINTOOLS", "path", "/nix/store/" + "b" * 32 + "-tool/bin/ld"),
            ("CC", "sha256", None),
            ("NIX_BINTOOLS", "sha256", None),
        ):
            with self.subTest(name=name, field=field):
                self.linux_link_environment()
                self.environment["toolInputs"][name][field] = value
                self.assertFalse(self.identity()["cacheEligible"])

    def test_actual_selected_linker_bytes_change_reference_identity(self):
        tools = self.root / "mutable-bintools/bin"
        tools.mkdir(parents=True)
        linker = tools / "ld.mold"
        linker.write_text("Alpha linker")
        self.environment["flags"] = {
            "NIX_BINTOOLS": str(tools.parent),
            "NIX_CFLAGS_LINK": " -fuse-ld=mold",
        }
        self.environment["toolInputs"] = cache.compiler_tool_inputs(
            self.environment["flags"]
        )
        alpha = copy.deepcopy(self.identity())
        self.assertFalse(alpha["cacheEligible"])
        linker.write_text("Beta linker")
        self.environment["toolInputs"] = cache.compiler_tool_inputs(
            self.environment["flags"]
        )
        beta = self.identity()
        self.assertNotEqual(alpha["key"], beta["key"])
        self.assertNotEqual(
            alpha["environment"]["toolInputs"]["NIX_BINTOOLS"]["sha256"],
            beta["environment"]["toolInputs"]["NIX_BINTOOLS"]["sha256"],
        )

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
