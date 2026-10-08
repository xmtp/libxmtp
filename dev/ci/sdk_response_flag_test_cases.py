"""Response-file cases that use the product transport fixture."""

import hashlib
import json


class ResponseFlagTests:
    def test_response_inputs_reject_generation_and_promotion_in_all_flag_routes(self):
        archive = self.stage("node")
        consumer = self.consumer()
        response = self.base / "compiler.args"
        response.write_text("-Cdebug-assertions=true\n")
        argument = "@" + str(response)
        routes = (
            "RUSTFLAGS",
            "CARGO_ENCODED_RUSTFLAGS",
            "CARGO_BUILD_RUSTFLAGS",
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS",
            "CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS",
            "CARGO_HOST_RUSTFLAGS",
        )
        for route in routes:
            with self.subTest(receipt=route):

                def update(folder, manifest):
                    path = folder / "generated/typescript-napi/sdk-contract.json"
                    record = json.loads(path.read_text())
                    record["artifact"]["compilerFlags"] = {route: [argument]}
                    path.write_text(json.dumps(record))
                    manifest["files"]["generated/typescript-napi/sdk-contract.json"] = (
                        hashlib.sha256(path.read_bytes()).hexdigest()
                    )

                changed = self.mutate(archive, update)
                self.assert_rejected(
                    consumer, changed, "node", "debug compiler semantics mismatch"
                )
        marker = self.base / "generation-called"
        launcher = self.repo / "dev/nix-shell"
        launcher.write_text(
            "#!/bin/sh\necho generation >> " + str(marker) + "\nexit 99\n"
        )
        launcher.chmod(0o755)
        for route in routes:
            with self.subTest(environment=route):
                result = self.command(
                    self.repo,
                    "build",
                    "--target",
                    "node",
                    "--output",
                    str(self.base / "response.tar"),
                    env={**self.env, route: argument},
                )
                self.assertIn("default debug compiler profile", result.stderr)
                self.assertFalse(marker.exists())
                self.assertFalse((self.base / "response.tar").exists())
        config = self.repo / ".cargo/config"
        config.parent.mkdir(exist_ok=True)
        for body in (
            "[build]\nrustflags=" + json.dumps([argument]),
            '[target."cfg(all())"]\nrustflags=' + json.dumps([argument]),
            "[target.x86_64-unknown-linux-gnu]\nrustflags=" + json.dumps([argument]),
            "[host]\nrustflags=" + json.dumps([argument]),
            "[env]\nRUSTFLAGS=" + json.dumps(argument),
            "[env]\nCARGO_ENCODED_RUSTFLAGS=" + json.dumps(argument),
            "[env]\nCARGO_BUILD_RUSTFLAGS=" + json.dumps(argument),
            "[env]\nCARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS="
            + json.dumps(argument),
        ):
            with self.subTest(config=body):
                config.write_text(body + "\n")
                result = self.command(
                    self.repo,
                    "build",
                    "--target",
                    "node",
                    "--output",
                    str(self.base / "response.tar"),
                )
                self.assertIn("default debug compiler profile", result.stderr)
                self.assertFalse(marker.exists())
        config.unlink()
        literal = '--cfg\x1fprobe="literal @text"'
        allowed = self.command(
            self.repo,
            "build",
            "--target",
            "node",
            "--output",
            str(self.base / "literal.tar"),
            env={**self.env, "CARGO_ENCODED_RUSTFLAGS": literal},
        )
        self.assertTrue(marker.is_file(), allowed.stderr)
