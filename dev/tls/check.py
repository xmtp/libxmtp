"""Check TLS ingress against the backend wire schema, with no Python packages."""

import base64
import http.client
import json
import os
from pathlib import Path
import re
import ssl
import struct
import subprocess
import sys
import time


HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
API = "xmtp.backend.v1."
SUBSCRIBE = "/" + API + "SubscriptionService/SubscribeStatic"
CONTEXT = ssl.create_default_context(cafile=str(HERE / ".generated/server.crt"))
CONTEXT.set_alpn_protocols(["http/1.1"])
HEADERS = {
    "content-type": "application/grpc-web+proto",
    "origin": "https://app.example",
    "authorization": "Bearer test",
    "x-app-version": "tls-check",
    "x-libxmtp-version": "tls-check",
    "traceparent": "00-0123456789abcdef0123456789abcdef-0123456789abcdef-01",
    "tracestate": "test=tls",
}


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def protobuf(kind, data, decode=False):
    """Use the repository schema for every request and response."""
    return subprocess.run(
        [
            "protoc",
            "-I",
            str(ROOT / "proto"),
            ("--decode=" if decode else "--encode=") + API + kind,
            "backend/v1/backend.proto",
        ],
        input=data,
        stdout=subprocess.PIPE,
        check=True,
    ).stdout


def literal(data):
    return '"' + "".join("\\%03o" % byte for byte in data) + '"'


def request(path, payload, method="POST", headers=None):
    connection = http.client.HTTPSConnection(
        "127.0.0.1", 18443, context=CONTEXT, timeout=5
    )
    body = (
        b"\0" + struct.pack("!I", len(payload)) + payload if method == "POST" else None
    )
    connection.request(method, path, body=body, headers=headers or HEADERS)
    response = connection.getresponse()
    require(response.version == 11, "client must use HTTP/1.1")
    require(response.status == 200, f"HTTP status {response.status}")
    return connection, response


def frame(response):
    header = response.read(5)
    require(len(header) == 5, "stream ended before a complete frame header")
    flag, size = struct.unpack("!BI", header)
    body = response.read(size)
    require(len(body) == size, "incomplete frame")
    return flag, body


def unary(service, kind, text, status=0):
    connection, response = request(
        "/" + API + service, protobuf(kind + "Request", text.encode())
    )
    try:
        message = None
        # Tonic can return an immediate error as a trailers-only response.
        # HAProxy represents those fields as HTTP/1.1 response headers.
        if response.getheader("grpc-status") is not None:
            require(
                response.getheader("grpc-status") == str(status),
                str(response.getheaders()),
            )
            require(response.read() == b"", "body after trailers-only error")
            return None, "\r\n".join(
                f"{name}: {value}" for name, value in response.getheaders()
            )
        while True:
            flag, body = frame(response)
            if flag == 128:
                trailers = body.decode().lower()
                require(
                    re.search(rf"grpc-status:\s*{status}\s*(?:\r?\n|$)", trailers),
                    trailers,
                )
                require(response.read() == b"", "bytes after trailers")
                return message, trailers
            require(flag == 0 and message is None, "unexpected unary frame")
            message = body
    finally:
        connection.close()


def grpcurl(data, method, *extra):
    return [
        "grpcurl",
        "-vv",
        "-cacert",
        str(HERE / ".generated/server.crt"),
        "-import-path",
        str(ROOT / "proto"),
        "-proto",
        "backend/v1/backend.proto",
        *extra,
        "-d",
        json.dumps(data),
        "127.0.0.1:18443",
        API + method,
    ]


def native(short=False):
    topic = base64.b64encode(b"\1" + os.urandom(32)).decode()
    data = {"topics": [{"topic": {"topic": topic}}]} if short else {}
    method = (
        "SubscriptionService/SubscribeStatic" if short else "QueryService/QueryNewest"
    )
    start = time.monotonic()
    result = subprocess.run(
        grpcurl(data, method, "-keepalive-time", "1", "-max-time", "25"),
        env={**os.environ, "GODEBUG": "http2debug=2"},
        capture_output=True,
        text=True,
        timeout=30,
    )
    elapsed = time.monotonic() - start
    log = result.stdout + result.stderr
    (HERE / ".generated" / ("short-timeout.log" if short else "native.log")).write_text(
        log
    )
    if short:
        require('"started"' in result.stdout, log)
        require(result.returncode != 0 and 10 <= elapsed < 20, log)
        require("DeadlineExceeded" not in log, "client deadline caused the drop")
        # grpc-go's idle keepalive uses eight zero bytes. Do not count its
        # earlier bandwidth-estimation PING as evidence of idle keepalives.
        require(
            re.search(r'read PING.*ACK.*ping="(?:\\x00){8}"', log),
            "no idle HTTP/2 PING acknowledgement before drop:\n" + log,
        )
        require(
            "RST_STREAM" in log or "EOF" in log, "no proxy stream termination:\n" + log
        )
        print(
            f"PASS: idle subscription dropped after {elapsed:.2f}s with 12s client/server timeouts"
        )
        for line in log.splitlines():
            if any(
                word in line for word in ("PING", "RST_STREAM", "Code:", "Message:")
            ):
                print(line)
    else:
        require(result.returncode == 0, log)
        require("Response trailers received:" in result.stdout, log)
        # grpc-go consumes grpc-status. Its HTTP/2 decoder log proves it was on wire.
        require(re.search(r'decoded hpack field.*grpc-status.*"0"', log), log)
        print("PASS 1: native gRPC trailers carry grpc-status=0 (grpcurl -vv)")
        for line in log.splitlines():
            if 'decoded hpack field header field "grpc-status"' in line:
                print(line)


def main():
    # Compose can report a running proxy before its listener is ready.
    for attempt in range(50):
        try:
            connection = http.client.HTTPSConnection(
                "127.0.0.1", 18443, context=CONTEXT, timeout=1
            )
            connection.connect()
            connection.close()
            break
        except OSError:
            if attempt == 49:
                raise
            time.sleep(0.1)
    if sys.argv[1:] == ["--short-timeout"]:
        native(short=True)
        return
    native()
    newest, _ = unary("QueryService/QueryNewest", "QueryNewest", "")
    require(newest == b"", "expected an empty QueryNewestResponse")
    _, trailers = unary("PublishService/Publish", "Publish", "envelopes {}", status=3)
    require("grpc-status-details-bin:" in trailers, "missing structured error details")
    print(
        "PASS 2: HTTP/1.1 gRPC-Web succeeds through the single proto h2 backend; error details survive"
    )

    required = set(HEADERS) - {"origin"}
    connection, response = request(
        SUBSCRIBE,
        b"",
        "OPTIONS",
        {
            "origin": HEADERS["origin"],
            "access-control-request-method": "POST",
            "access-control-request-headers": ",".join(sorted(required)),
        },
    )
    allowed = {
        value.strip().lower()
        for value in response.getheader("access-control-allow-headers", "").split(",")
    }
    require(required <= allowed, f"missing allowed headers: {required - allowed}")
    response.read()
    connection.close()

    key = os.urandom(32)
    topic = b"\1" + key
    payload = protobuf(
        "SubscribeStaticRequest",
        f"topics {{ topic {{ topic: {literal(topic)} }} }}".encode(),
    )
    connection, response = request(SUBSCRIBE, payload)
    try:
        exposed = {
            value.strip().lower()
            for value in response.getheader("access-control-expose-headers", "").split(
                ","
            )
        }
        require(
            {"grpc-status", "grpc-message", "grpc-status-details-bin", "x-request-id"}
            <= exposed,
            str(exposed),
        )
        require(
            response.getheader("access-control-allow-origin") == "*",
            "missing allow-origin",
        )
        require(response.getheader("x-request-id"), "missing request ID")
        print(
            "PASS 3: all six CORS request headers and all four exposed headers survive"
        )
        flag, body = frame(response)
        started = protobuf("SubscribeStaticResponse", body, decode=True)
        require(flag == 0 and started.startswith(b"started {"), repr(started))
        expected_target = protobuf(
            "SubscribeStaticResponse",
            f"started {{ targets {{ topic {{ topic: {literal(topic)} }} }} }}".encode(),
        )
        expected_text = protobuf(
            "SubscribeStaticResponse", expected_target, decode=True
        )
        require(expected_text.split(b"  targets", 1)[1] in started, repr(started))
        print("  Started arrived before publish (one empty target)")
        time.sleep(1)
        # Same fixture bytes as inline_welcome_envelope in https_ingress.rs.
        envelope_text = f'welcome_message {{ v1 {{ installation_key: {literal(key)} data: "\\020\\021" hpke_public_key: "\\022\\023" wrapper_algorithm: 1 welcome_metadata: "\\024\\025" }} }}'
        envelope = protobuf("ClientEnvelope", envelope_text.encode())
        published, _ = unary(
            "PublishService/Publish", "Publish", "envelopes {" + envelope_text + "}"
        )
        published_text = protobuf("PublishResponse", published, decode=True).decode()
        # Re-encode the returned metadata and the exact fixture as the expected frame.
        meta = published_text.removeprefix("envelope_metas {\n").removesuffix("}\n")
        expected = protobuf(
            "SubscribeStaticResponse",
            (
                "messages { envelopes { meta {"
                + meta
                + "} envelope {"
                + envelope_text
                + "} } }"
            ).encode(),
        )
        deadline = time.monotonic() + 5
        while True:
            require(time.monotonic() < deadline, "message did not arrive within 5s")
            flag, body = frame(response)
            require(flag == 0, "stream ended before message")
            if body == expected:
                break
            decoded = protobuf("SubscribeStaticResponse", body, decode=True)
            require(decoded.startswith(b"keepalive {"), repr(decoded))
        require(envelope in body, "published envelope changed")
        require(not response.isclosed(), "response closed after message")
        print(
            "PASS 4: later message and exact metadata arrived on the SAME open response; no buffering"
        )
    finally:
        connection.close()


if __name__ == "__main__":
    main()
