#!/usr/bin/env python3
"""Refuse changed Maven releases; only a completely absent version may publish."""

import argparse
import base64
import os
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path


class RegistryRedirects(urllib.request.HTTPRedirectHandler):
    """Keep registry credentials on the origin that received them."""

    def redirect_request(self, request, response, code, message, headers, url):
        redirected = super().redirect_request(
            request, response, code, message, headers, url
        )

        def origin(value):
            parsed = urllib.parse.urlsplit(value)
            return (
                parsed.scheme,
                parsed.hostname,
                parsed.port or {"https": 443, "http": 80}.get(parsed.scheme),
            )

        if redirected is not None and origin(request.full_url) != origin(url):
            redirected.remove_header("Authorization")
        return redirected


OPENER = urllib.request.build_opener(RegistryRedirects())


def check(directory, version, repository="central"):
    if repository == "central":
        base = "https://repo.maven.apache.org/maven2"
        headers = {}
    elif repository == "github":
        base = "https://maven.pkg.github.com/xmtp/libxmtp"
        actor, token = os.environ.get("GITHUB_ACTOR"), os.environ.get("GITHUB_TOKEN")
        if not actor or not token:
            raise ValueError("GitHub Packages needs GITHUB_ACTOR and GITHUB_TOKEN")
        credentials = base64.b64encode(f"{actor}:{token}".encode()).decode()
        headers = {"Authorization": f"Basic {credentials}"}
    else:
        raise ValueError("Unknown Maven repository")
    payloads = sorted(
        p
        for p in directory.iterdir()
        if p.suffix in (".aar", ".jar", ".pom", ".module")
    )
    if len(payloads) < 4:
        raise ValueError("Prepared Maven publication is incomplete")
    present = 0
    for path in payloads:
        url = f"{base}/org/xmtp/android/{version}/{path.name}"
        request = urllib.request.Request(url, headers=headers)
        try:
            with OPENER.open(request, timeout=60) as response:
                remote = response.read()
        except urllib.error.HTTPError as error:
            if error.code == 404:
                continue
            raise
        if remote != path.read_bytes():
            raise ValueError(f"Existing Maven version has different bytes: {path.name}")
        present += 1
    if present not in (0, len(payloads)):
        raise ValueError(
            "Maven release is incomplete; wait for visibility before retrying"
        )
    print("existing" if present else "absent")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("version")
    parser.add_argument(
        "--repository", choices=("central", "github"), default="central"
    )
    args = parser.parse_args()
    try:
        check(args.directory, args.version, args.repository)
    except ValueError as error:
        sys.exit(str(error))
