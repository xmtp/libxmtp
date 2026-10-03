#!/usr/bin/env python3
"""Check iOS target OpenSSL selection and production build forwarding."""

import importlib.util
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "mobile", Path(__file__).with_name("mobile-package.py")
)
mobile = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mobile)


class IosOpenSslTests(unittest.TestCase):
    def test_defaults_vendor_for_both_targets_and_preserve_host_inputs(self):
        for triple in mobile.IOS:
            for policy in (None, "0", "1"):
                inputs = {
                    "OPENSSL_DIR": "/host/root",
                    "OPENSSL_LIB_DIR": "/host/lib",
                    "OPENSSL_INCLUDE_DIR": "/host/include",
                    "CC_" + triple.replace("-", "_"): "/xcode/clang",
                }
                if policy is not None:
                    inputs["OPENSSL_NO_VENDOR"] = policy
                with self.subTest(triple=triple, policy=policy):
                    with patch.dict(os.environ, inputs, clear=True):
                        child = mobile.ios_environment(triple)
                        prefix = triple.upper().replace("-", "_") + "_OPENSSL_"
                        self.assertEqual(child.get(prefix + "NO_VENDOR"), "0")
                        self.assertEqual(dict(os.environ), inputs)
                        for key, value in inputs.items():
                            self.assertEqual(child[key], value)

    def test_external_paths_and_explicit_policy(self):
        for triple in mobile.IOS:
            prefix = triple.upper().replace("-", "_") + "_OPENSSL_"
            for component in (None, "DIR", "LIB_DIR", "INCLUDE_DIR"):
                for policy in (None, "0", "1"):
                    inputs = {"OPENSSL_NO_VENDOR": "1"}
                    if component:
                        inputs[prefix + component] = "/target/" + component
                    if policy is not None:
                        inputs[prefix + "NO_VENDOR"] = policy
                    with self.subTest(triple=triple, path=component, policy=policy):
                        with patch.dict(os.environ, inputs, clear=True):
                            child = mobile.ios_environment(triple)
                            self.assertEqual(
                                child.get(prefix + "NO_VENDOR"),
                                policy
                                if policy is not None
                                else "1"
                                if component
                                else "0",
                            )
                            for key, value in inputs.items():
                                self.assertEqual(child[key], value)
                            self.assertEqual(dict(os.environ), inputs)

    def test_target_root_keeps_host_paths_and_overrides_separate(self):
        host_prefix = "AARCH64_APPLE_DARWIN_OPENSSL_"
        for triple in mobile.IOS:
            prefix = triple.upper().replace("-", "_") + "_OPENSSL_"
            for policy in (None, "0", "1"):
                for components in (
                    (),
                    ("LIB_DIR",),
                    ("INCLUDE_DIR",),
                    ("LIB_DIR", "INCLUDE_DIR"),
                ):
                    for qualified in (False, True):
                        inputs = {
                            prefix + "DIR": "/target/root",
                            "OPENSSL_LIB_DIR": "/host/lib",
                            "OPENSSL_INCLUDE_DIR": "/host/include",
                        }
                        inputs.update(
                            {prefix + key: "/target/" + key for key in components}
                        )
                        if policy is not None:
                            inputs[prefix + "NO_VENDOR"] = policy
                        if qualified:
                            inputs.update(
                                {
                                    host_prefix + key: "/qualified/" + key
                                    for key in ("LIB_DIR", "INCLUDE_DIR")
                                }
                            )
                        with self.subTest(
                            triple=triple,
                            policy=policy,
                            components=components,
                            qualified=qualified,
                        ):
                            with (
                                patch.dict(os.environ, inputs, clear=True),
                                patch.object(
                                    mobile.artifacts,
                                    "compiler_host",
                                    return_value="aarch64-apple-darwin",
                                ),
                            ):
                                child = mobile.ios_environment(triple)
                                self.assertEqual(dict(os.environ), inputs)
                                for key in ("LIB_DIR", "INCLUDE_DIR"):
                                    self.assertEqual(
                                        child.get(prefix + key),
                                        inputs.get(prefix + key),
                                    )
                                    if policy == "0":
                                        self.assertEqual(
                                            child["OPENSSL_" + key],
                                            inputs["OPENSSL_" + key],
                                        )
                                    else:
                                        self.assertFalse(
                                            "OPENSSL_" + key in child,
                                            "Host path shadows target root",
                                        )
                                        if host_prefix != prefix:
                                            self.assertEqual(
                                                child[host_prefix + key],
                                                inputs.get(
                                                    host_prefix + key,
                                                    inputs["OPENSSL_" + key],
                                                ),
                                            )

    def test_equal_host_target_cannot_restore_generic_root_shadow(self):
        for triple in mobile.IOS:
            prefix = triple.upper().replace("-", "_") + "_OPENSSL_"
            inputs = {prefix + "DIR": "/target/root", "OPENSSL_LIB_DIR": "/host/lib"}
            with self.subTest(triple=triple):
                with (
                    patch.dict(os.environ, inputs, clear=True),
                    patch.object(
                        mobile.artifacts, "compiler_host", return_value=triple
                    ),
                ):
                    child = mobile.ios_environment(triple)
                    self.assertFalse(
                        prefix + "LIB_DIR" in child,
                        "Host compensation shadows target root",
                    )
                    self.assertFalse(
                        "OPENSSL_LIB_DIR" in child, "Generic path shadows target root"
                    )
                    self.assertEqual(dict(os.environ), inputs)

    def test_production_build_forwards_each_selected_target_environment(self):
        calls = []
        inputs = {"OPENSSL_NO_VENDOR": "1", "OPENSSL_LIB_DIR": "/host/lib"}
        with (
            tempfile.TemporaryDirectory() as output,
            patch.dict(os.environ, inputs, clear=True),
            patch.object(
                mobile.sys,
                "argv",
                ["mobile-package.py", "build", "ios", "--artifacts", output],
            ),
            patch.object(
                mobile,
                "run",
                side_effect=lambda command, **kwargs: calls.append((command, kwargs)),
            ),
        ):
            mobile.main()
            self.assertEqual(dict(os.environ), inputs)
            self.assertEqual(len(calls), len(mobile.IOS))
            for (command, kwargs), triple in zip(calls, mobile.IOS):
                self.assertEqual(command[command.index("--rust-target") + 1], triple)
                self.assertEqual(command[-1], str(Path(output) / triple))
                self.assertEqual(
                    kwargs["env"].get(
                        triple.upper().replace("-", "_") + "_OPENSSL_NO_VENDOR"
                    ),
                    "0",
                )
                self.assertEqual(kwargs["env"]["OPENSSL_LIB_DIR"], "/host/lib")
                self.assertIn("--skip-bindgen", command)

    def test_effective_target_and_host_selectors_change_cache_identity(self):
        names = [
            "OPENSSL_" + key for key in ("DIR", "LIB_DIR", "INCLUDE_DIR", "NO_VENDOR")
        ]
        for triple in (*mobile.IOS, "aarch64-apple-darwin"):
            names.extend(
                triple.upper().replace("-", "_") + "_OPENSSL_" + key
                for key in ("DIR", "LIB_DIR", "INCLUDE_DIR", "NO_VENDOR")
            )
        with (
            patch.dict(os.environ, {}, clear=True),
            patch.object(
                mobile.artifacts.subprocess,
                "check_output",
                return_value=b"compiler fixture",
            ),
            patch.object(mobile.artifacts.shutil, "which", return_value=None),
        ):
            baseline = mobile.artifacts.build_context()
            for name in names:
                with self.subTest(name=name):
                    os.environ[name] = "caller-value"
                    self.assertNotEqual(mobile.artifacts.build_context(), baseline)
                    del os.environ[name]
                    self.assertEqual(mobile.artifacts.build_context(), baseline)


if __name__ == "__main__":
    unittest.main()
