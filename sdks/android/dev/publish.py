#!/usr/bin/env python3
"""Resume Android publication using an immutable bundle and a saved deployment ID."""

import argparse
import base64
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
import xml.etree.ElementTree as ET
import zipfile
import io


class RegistryRedirectHandler(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, fp, code, message, headers, url):
        before = urllib.parse.urlsplit(request.full_url)
        after = urllib.parse.urlsplit(url)
        if before.scheme == "https" and after.scheme != "https":
            raise urllib.error.URLError("Refusing registry HTTPS downgrade")
        redirected = super().redirect_request(request, fp, code, message, headers, url)
        if redirected is not None and (before.scheme, before.netloc) != (
            after.scheme,
            after.netloc,
        ):
            # Maven downloads redirect to signed blob URLs. Registry credentials
            # must never follow that redirect to a different origin.
            redirected.remove_header("Authorization")
        return redirected


class Publisher:
    def __init__(
        self, repository, state_dir, version, source, github_auth, central_auth
    ):
        self.version = version
        self.source = source
        self.directory = Path(repository) / "org/xmtp/android" / version
        self.files = sorted(path for path in self.directory.iterdir() if path.is_file())
        for suffix in (".aar", ".pom", "-sources.jar", "-javadoc.jar"):
            artifact = self.directory / f"android-{version}{suffix}"
            if (
                not artifact.is_file()
                or not artifact.with_name(artifact.name + ".asc").is_file()
            ):
                raise RuntimeError(f"Missing signed release artifact: {artifact.name}")
        manifest = {
            p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in self.files
        }
        self.identity = dict(version=version, source=source, files=manifest)
        self.state_dir = Path(state_dir)
        self.state = self.identity.copy()
        for path in self.state_dir.rglob("state.json"):
            saved = json.loads(path.read_text())
            if any(saved.get(key) != value for key, value in self.identity.items()):
                raise RuntimeError(
                    "Saved deployment belongs to different release bytes or source"
                )
            if self.state.get("deployment_id") not in (None, saved["deployment_id"]):
                raise RuntimeError("Multiple Central deployment IDs for this release")
            self.state = saved
        self.github_url = "https://maven.pkg.github.com/xmtp/libxmtp"
        self.central_url = "https://central.sonatype.com/api/v1/publisher"
        self.github_auth = github_auth
        self.central_auth = central_auth

    def request(self, method, url, auth, data=None, content_type=None, missing=False):
        headers = {"Authorization": auth}
        if content_type:
            headers["Content-Type"] = content_type
        request = urllib.request.Request(url, data=data, headers=headers, method=method)
        try:
            opener = urllib.request.build_opener(RegistryRedirectHandler())
            with opener.open(request, timeout=120) as response:
                return response.read()
        except urllib.error.HTTPError as error:
            code = error.code
            error.close()
            if missing and code == 404:
                return None
            # Response bodies and exception strings may contain credentials.
            raise RuntimeError(
                f"{method} {urllib.parse.urlsplit(url).hostname}: HTTP {code}"
            ) from None
        except (OSError, urllib.error.URLError):
            raise RuntimeError(
                f"{method} {urllib.parse.urlsplit(url).hostname}: connection failed"
            ) from None

    def stage(self):
        # GitHub may have accepted some files before a failed request. Resume
        # only matching bytes, and never overwrite an existing different file.
        for path in self.files:
            relative = f"org/xmtp/android/{self.version}/{path.name}"
            url = self.github_url + "/" + urllib.parse.quote(relative)
            data = path.read_bytes()
            existing = self.request("GET", url, self.github_auth, missing=True)
            if existing is not None:
                if existing != data:
                    raise RuntimeError(
                        f"GitHub Packages already contains different bytes: {path.name}"
                    )
                continue
            self.request("PUT", url, self.github_auth, data, "application/octet-stream")

        # Maven's version index is mutable. Merge it rather than replacing older
        # releases with the local repository's one-version index. The workflow
        # serializes Android publication across versions while updating it.
        metadata_url = self.github_url + "/org/xmtp/android/maven-metadata.xml"
        existing = self.request("GET", metadata_url, self.github_auth, missing=True)
        metadata = ET.fromstring(existing) if existing else ET.Element("metadata")
        for name, value in (("groupId", "org.xmtp"), ("artifactId", "android")):
            field = metadata.find(name)
            if field is None:
                ET.SubElement(metadata, name).text = value
            elif field.text != value:
                raise RuntimeError(
                    "GitHub Packages metadata belongs to another artifact"
                )
        versioning = metadata.find("versioning")
        if versioning is None:
            versioning = ET.SubElement(metadata, "versioning")
        versions = versioning.find("versions")
        if versions is None:
            versions = ET.SubElement(versioning, "versions")
        if self.version not in [item.text for item in versions.findall("version")]:
            ET.SubElement(versions, "version").text = self.version
            for name, value in (
                ("latest", self.version),
                ("release", self.version),
                ("lastUpdated", datetime.now(timezone.utc).strftime("%Y%m%d%H%M%S")),
            ):
                field = versioning.find(name)
                if field is None:
                    field = ET.SubElement(versioning, name)
                field.text = value
            existing = ET.tostring(metadata, encoding="utf-8", xml_declaration=True)
            self.request(
                "PUT", metadata_url, self.github_auth, existing, "application/xml"
            )
        # Complete these even when a prior attempt saved the index but lost its
        # response before uploading the checksums.
        for algorithm in ("sha1", "md5"):
            checksum = hashlib.new(algorithm, existing).hexdigest().encode()
            self.request(
                "PUT",
                metadata_url + "." + algorithm,
                self.github_auth,
                checksum,
                "text/plain",
            )

        if not self.state.get("deployment_id"):
            bundle = io.BytesIO()
            with zipfile.ZipFile(bundle, "w", zipfile.ZIP_DEFLATED) as archive:
                for path in self.files:
                    archive.write(path, f"org/xmtp/android/{self.version}/{path.name}")
            boundary = uuid.uuid4().hex
            body = (
                (
                    f'--{boundary}\r\nContent-Disposition: form-data; name="bundle"; filename="android.zip"\r\n'
                    "Content-Type: application/octet-stream\r\n\r\n"
                ).encode()
                + bundle.getvalue()
                + f"\r\n--{boundary}--\r\n".encode()
            )
            query = urllib.parse.urlencode(
                {"name": f"android-{self.version}", "publishingType": "USER_MANAGED"}
            )
            deployment = (
                self.request(
                    "POST",
                    self.central_url + "/upload?" + query,
                    self.central_auth,
                    body,
                    f"multipart/form-data; boundary={boundary}",
                )
                .decode()
                .strip()
            )
            self.state["deployment_id"] = str(uuid.UUID(deployment))
        # The workflow must retain this file successfully before calling release.
        # A lost upload response can leave a draft, but cannot publish it.
        self.state_dir.mkdir(parents=True, exist_ok=True)
        (self.state_dir / "state.json").write_text(
            json.dumps(self.state, indent=2) + "\n"
        )
        print(
            "GitHub Packages complete; Central deployment staged: "
            + self.state["deployment_id"]
        )

    def release(self, timeout=900, interval=10):
        deployment = self.state.get("deployment_id")
        if not deployment:
            raise RuntimeError("Central deployment ID must be saved before release")
        deadline = time.monotonic() + timeout
        while True:
            status = json.loads(
                self.request(
                    "POST",
                    self.central_url
                    + "/status?"
                    + urllib.parse.urlencode({"id": deployment}),
                    self.central_auth,
                )
            )
            state = status["deploymentState"]
            if state == "PUBLISHED":
                print("Maven Central publication complete: " + deployment)
                return
            if state == "VALIDATED":
                self.request(
                    "POST",
                    self.central_url + "/deployment/" + deployment,
                    self.central_auth,
                )
            elif state not in ("PENDING", "VALIDATING", "PUBLISHING"):
                raise RuntimeError(
                    f"Central deployment {deployment}: {state}; inspect the Central Portal"
                )
            if time.monotonic() >= deadline:
                raise RuntimeError(
                    f"Central deployment {deployment} is still {state}; retry failed jobs"
                )
            time.sleep(interval)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=("stage", "release"))
    parser.add_argument("--repository", default="android-maven-bundle")
    parser.add_argument("--state-dir", default="android-central-state")
    args = parser.parse_args()
    required = (
        "VERSION",
        "SOURCE_SHA",
        "GITHUB_ACTOR",
        "GITHUB_TOKEN",
        "MAVEN_USERNAME",
        "MAVEN_PASSWORD",
    )
    for name in required:
        if not os.environ.get(name):
            parser.error(f"Missing Android publishing credential or input: {name}")

    def auth(scheme, user, password):
        return scheme + " " + base64.b64encode(f"{user}:{password}".encode()).decode()

    publisher = Publisher(
        args.repository,
        args.state_dir,
        os.environ["VERSION"],
        os.environ["SOURCE_SHA"],
        auth("Basic", os.environ["GITHUB_ACTOR"], os.environ["GITHUB_TOKEN"]),
        auth("Bearer", os.environ["MAVEN_USERNAME"], os.environ["MAVEN_PASSWORD"]),
    )
    getattr(publisher, args.operation)()


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, ValueError, KeyError, OSError) as error:
        raise SystemExit(str(error)) from None
