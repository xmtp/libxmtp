"""Android dependency-input cases for the normal packaging suite."""

from unittest.mock import Mock


class AndroidDependencyInputs:
    def seed_android_dependency_inputs(self):
        project = self.sdk_root
        for name in (
            "library/gradle.lockfile",
            "buildscript-gradle.lockfile",
            "gradle/verification-metadata.xml",
        ):
            file = project / name
            file.parent.mkdir(parents=True, exist_ok=True)
            file.write_text("fixture dependency input")


    def test_android_owned_project_override_keeps_common_native_receipts(self):
        self.prepare_mobile_stage("android")
        self.sdk_root = self.root / "owned-worktree/sdks/android"
        self.seed_android_dependency_inputs()
        self.assemble_mobile("android", sdk_root=self.sdk_root)
        self.assertTrue((self.root / "products/android/xmtp-sdk.aar").exists())
        self.assertFalse((self.root / "sdks/android/library/build").exists())


    def test_android_dependency_inputs_are_required_before_tool_use(self):
        for name in (
            "library/gradle.lockfile",
            "buildscript-gradle.lockfile",
            "gradle/verification-metadata.xml",
        ):
            with self.subTest(name=name):
                output = self.prepare_mobile_stage("android")
                previous = self.product_files(output)
                project = self.sdk_root
                (project / name).unlink()
                tool = Mock(side_effect=self.mobile_tool)
                with self.assertRaisesRegex(
                    ValueError, "Android dependency input missing"
                ):
                    self.assemble_mobile("android", tool)
                tool.assert_not_called()
                self.assertEqual(self.product_files(output), previous)
                self.assertEqual(list(output.parent.glob(".sdk-mobile-stage-*")), [])


    def test_android_fixture_inputs_do_not_cover_selected_sdk_root(self):
        output = self.prepare_mobile_stage("android")
        previous = self.product_files(output)
        fixture = self.root / "crates/xmtp_sdk/packaging/android"
        for name in ("gradle.lockfile", "buildscript-gradle.lockfile",
                     "gradle/verification-metadata.xml"):
            path = fixture / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("fixture dependency input")
        self.sdk_root = self.root / "owned-worktree/sdks/android"
        tool = Mock(side_effect=self.mobile_tool)
        with self.assertRaisesRegex(ValueError, "Android dependency input missing"):
            self.assemble_mobile("android", tool, sdk_root=self.sdk_root)
        tool.assert_not_called()
        self.assertEqual(self.product_files(output), previous)
        self.seed_android_dependency_inputs()
        self.assemble_mobile("android", sdk_root=self.sdk_root)
        self.assertTrue((output / "xmtp-sdk.aar").is_file())


    def test_android_stage_uses_strict_read_only_dependency_inputs(self):
        self.prepare_mobile_stage("android")

        def tool(command, **kwargs):
            self.assertEqual(command[0], str(self.sdk_root / "gradlew"))
            self.assertEqual(command[2], str(self.sdk_root))
            self.assertIn(":library:assembleRelease", command)
            self.assertIn("--dependency-verification=strict", command)
            self.assertFalse(any(arg.startswith("--max-workers") for arg in command))
            self.assertIn("-Pkotlin.compiler.execution.strategy=in-process", command)
            self.assertFalse(
                any(
                    arg.startswith(
                        (
                            "--write-locks",
                            "--update-locks",
                            "--write-verification-metadata",
                        )
                    )
                    for arg in command
                )
            )
            self.mobile_tool(command, **kwargs)

        self.assemble_mobile("android", tool)


