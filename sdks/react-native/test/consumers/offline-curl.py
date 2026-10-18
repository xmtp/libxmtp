#!/usr/bin/env python3
"""Replay only SHA256-admitted native build downloads; deny undeclared URLs."""

import hashlib
import json
import os
from pathlib import Path
import shutil
import sys
from urllib.parse import urlparse


def main():
    arguments = sys.argv[1:]
    if arguments == ["--version"]:
        print("curl native immutable dependency replay")
        return
    urls = [value for value in arguments if value.startswith(("https://", "http://"))]
    if len(urls) != 1:
        raise ValueError("native dependency replay requires one declared URL")
    url = urls[0]
    root = Path(os.environ["XMTP_RN_IOS_DOWNLOADS"]).resolve(strict=True)
    manifest = json.loads(Path(os.environ["XMTP_RN_IOS_DOWNLOAD_MANIFEST"]).read_text())
    checksum = url.endswith(".sha1")
    base_url = url[:-5] if checksum else url
    matches = [row for row in manifest["artifacts"] if row["url"] == base_url]
    if len(matches) != 1:
        raise ValueError(f"undeclared native build download: {url}")
    row = matches[0]
    relative = Path(row["path"])
    if relative.is_absolute() or ".." in relative.parts:
        raise ValueError("native download path escaped its immutable input")
    source = (root / relative).resolve(strict=True)
    with source.open("rb") as stream:
        if hashlib.file_digest(stream, "sha256").hexdigest() != row["sha256"]:
            raise ValueError("native dependency input differs from its declared SHA256")
    head = any(
        value in ("--head", "-I")
        or (value.startswith("-") and not value.startswith("--") and "I" in value)
        for value in arguments
    )
    output = None
    write_out = None
    headers_output = None
    index = 0
    while index < len(arguments):
        value = arguments[index]
        if value in ("-o", "--output") or (
            value.startswith("-") and not value.startswith("--") and value.endswith("o")
        ):
            index += 1
            output = arguments[index]
        elif value.startswith("--output="):
            output = value.split("=", 1)[1]
        elif value in ("-w", "--write-out") or (
            value.startswith("-") and not value.startswith("--") and value.endswith("w")
        ):
            index += 1
            write_out = arguments[index]
        elif value in ("-D", "--dump-header"):
            index += 1
            headers_output = arguments[index]
        elif value in ("-O", "--remote-name"):
            output = Path(urlparse(url).path).name
        index += 1
    if checksum:
        with source.open("rb") as stream:
            body = hashlib.file_digest(stream, "sha1").hexdigest().encode()
        size = len(body)
    else:
        body = None
        size = source.stat().st_size
    header = f"HTTP/1.1 200 OK\r\nContent-Length: {size}\r\n\r\n".encode()
    if headers_output:
        Path(headers_output).write_bytes(header)
    if output:
        destination = Path(output)
        destination.parent.mkdir(parents=True, exist_ok=True)
        if head:
            destination.write_bytes(header)
        elif body is not None:
            destination.write_bytes(body)
        else:
            shutil.copyfile(source, destination)
    elif head:
        sys.stdout.buffer.write(header)
    elif body is not None:
        sys.stdout.buffer.write(body)
    else:
        with source.open("rb") as stream:
            shutil.copyfileobj(stream, sys.stdout.buffer)
    if write_out:
        sys.stdout.buffer.write(
            write_out.replace("%{http_code}", "200")
            .replace("%{size_download}", str(0 if head else size))
            .encode()
        )


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, OSError) as error:
        print(f"native dependency replay: {error}", file=sys.stderr)
        raise SystemExit(22)
