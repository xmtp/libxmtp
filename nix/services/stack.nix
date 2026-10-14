# The libxmtp test services as one process-compose-flake module: a PostgreSQL
# primary and a streaming replica, VersityGW with the attachments bucket, and,
# when `backend` is set, anvil, the backend, and toxiproxy in front of it.
#
# Ports are run-time inputs. A port variable that the caller sets wins; every
# other port is a free loopback port that the kernel picks. So one definition
# serves this worktree's stack (dev/services, slot ports from
# dev/worktree-env), `services-test`, and a Nix build (free ports). The port
# and URL names are the names in dev/docker/.env.
#
# Run-time settings:
#   XMTP_SERVICES_STATE  state directory: data, configs, the env file
#                        (default: a new temporary directory, on /dev/shm
#                        where Linux has one, so the replica replays fast)
#   XMTP_SERVICES_LOGS   when set, each service writes its output to
#                        `<name>.log` there instead of to process-compose
#   XMTP_BACKEND_LISTEN_HOST  the backend listen address (default 127.0.0.1)
#
# `$XMTP_SERVICES_STATE/env` holds `export` lines for the ports and URLs.
{
  backend,
  repo,
}:
{ pkgs, lib, ... }:
let
  inherit (pkgs)
    postgresql_18
    versitygw
    awscli2
    curl
    jq
    toxiproxy
    grpc-health-probe
    foundry-bin
    python3
    coreutils
    cacert
    ;
  withBackend = backend != null;
  bin = pkg: name: "${pkg}/bin/${name}";
  pg = name: bin postgresql_18 name;
  # Every command and probe is a store script. process-compose substitutes
  # `$NAME` in the whole config file before it applies disable_env_expansion
  # (src/loader/loader.go:152-191 at v1.120.0), so a `$NAME` in the config
  # breaks the parse when its value holds a quote. A store path has no `$`.
  script =
    name: text:
    "${pkgs.writeShellScript "xmtp-services-${name}" ''
      set -euo pipefail
      ${text}
    ''}";
  # A service process. With XMTP_SERVICES_LOGS set, its output goes to a file,
  # so a test command's output stays readable.
  service =
    name: text:
    script name ''
      if [ -n "''${XMTP_SERVICES_LOGS:-}" ]; then
        exec >>"$XMTP_SERVICES_LOGS/${name}.log" 2>&1
      fi
      ${text}
    '';
  # Every port this stack listens on.
  ports = [
    "XMTP_BACKEND_DB_PORT"
    "XMTP_BACKEND_REPLICA_PORT"
    "XMTP_S3_PORT"
  ]
  ++ lib.optionals withBackend [
    "XMTP_BACKEND_PORT"
    "XMTP_BACKEND_METRICS_PORT"
    "XMTP_ANVIL_PORT"
    "XMTP_TOXIPROXY_API_PORT"
    "XMTP_TOXIPROXY_PORT"
  ];
  # A port that the caller sets must be free: fail at once if something
  # accepts a connection on it. Bind every other port at once, so two picks
  # in one run cannot be equal, then release them for the services.
  freePorts = pkgs.writeText "free-ports.py" ''
    import os
    import socket
    import sys

    sockets = []
    for name in sys.argv[1:]:
        value = os.environ.get(name)
        if value:
            try:
                socket.create_connection(("127.0.0.1", int(value)), 0.5).close()
            except OSError:
                continue
            sys.exit(f"services: {name}={value} is in use")
        s = socket.socket()
        s.bind(("127.0.0.1", 0))
        sockets.append((name, s))
    for name, s in sockets:
        print(f"export {name}={s.getsockname()[1]}")
  '';
  # The URLs always follow the ports.
  urls = {
    DATABASE_URL = "postgres://xmtp:xmtp@127.0.0.1:$XMTP_BACKEND_DB_PORT/xmtp_backend";
    XMTP_REPLICA_URL = "postgres://xmtp:xmtp@127.0.0.1:$XMTP_BACKEND_REPLICA_PORT/xmtp_backend";
    XMTP_S3_URL = "http://127.0.0.1:$XMTP_S3_PORT";
    XMTP_S3_BASE_URL = "http://127.0.0.1:$XMTP_S3_PORT/attachments";
  }
  // lib.optionalAttrs withBackend {
    XMTP_BACKEND_URL = "http://127.0.0.1:$XMTP_BACKEND_PORT";
    XMTP_BACKEND_TOXIC_URL = "http://127.0.0.1:$XMTP_TOXIPROXY_PORT";
    XMTP_TOXIPROXY_API = "http://127.0.0.1:$XMTP_TOXIPROXY_API_PORT";
    ANVIL_URL = "http://127.0.0.1:$XMTP_ANVIL_PORT";
    XMTP_CHAIN_31337_URL = "http://127.0.0.1:$XMTP_ANVIL_PORT";
  };
  envNames = ports ++ builtins.attrNames urls;

  postgresConfig = port: ''
    {
      echo "listen_addresses = '127.0.0.1'"
      echo "port = ${port}"
      echo "unix_socket_directories = '''"
      echo "fsync = off"
    }'';
  probe = name: text: {
    exec.command = script "${name}-ready" text;
    period_seconds = 1;
    timeout_seconds = 3;
    failure_threshold = 120;
  };
  pgReady =
    name: port: probe name ''${pg "pg_isready"} --host=127.0.0.1 --port="${port}" --username=xmtp'';
  # Isolate the AWS CLI from caller profiles, credentials, proxies and endpoints.
  aws = ''
    ${bin coreutils "env"} -i HOME="$XMTP_SERVICES_STATE" \
      AWS_CONFIG_FILE="$XMTP_SERVICES_STATE/aws-config" \
      AWS_SHARED_CREDENTIALS_FILE="$XMTP_SERVICES_STATE/aws-credentials" \
      AWS_ACCESS_KEY_ID=xmtps3 AWS_SECRET_ACCESS_KEY=xmtps3secret \
      AWS_EC2_METADATA_DISABLED=true AWS_PAGER= AWS_MAX_ATTEMPTS=1 \
      ${bin awscli2 "aws"} --endpoint-url "$XMTP_S3_URL" --region us-east-1 \
      --cli-connect-timeout 2 --cli-read-timeout 2'';
  data = "$XMTP_SERVICES_STATE/data";

  storage = {
    db-init.command = service "db-init" ''
      ${pg "postgres"} --version
      ${pg "initdb"} --pgdata="${data}/primary" --username=xmtp --encoding=UTF8 \
        --locale=C --auth=trust
      {
        ${postgresConfig "$XMTP_BACKEND_DB_PORT"}
        # The replica reads retained WAL after replay pauses in replica tests.
        echo "wal_keep_size = '256MB'"
      } >>"${data}/primary/postgresql.conf"
    '';
    db = {
      command = service "db" ''exec ${pg "postgres"} -D "${data}/primary"'';
      depends_on.db-init.condition = "process_completed_successfully";
      readiness_probe = pgReady "db" "$XMTP_BACKEND_DB_PORT";
      # SIGINT is a fast shutdown.
      shutdown.signal = 2;
    };
    db-setup = {
      command = service "db-setup" ''
        ${pg "createdb"} --host=127.0.0.1 --port="$XMTP_BACKEND_DB_PORT" --username=xmtp xmtp_backend
      '';
      depends_on.db.condition = "process_healthy";
    };
    replica-init = {
      command = service "replica-init" ''
        ${pg "pg_basebackup"} --host=127.0.0.1 --port="$XMTP_BACKEND_DB_PORT" --username=xmtp \
          --pgdata="${data}/replica" --wal-method=stream --write-recovery-conf --checkpoint=fast
        {
          ${postgresConfig "$XMTP_BACKEND_REPLICA_PORT"}
          echo 'hot_standby = on'
        } >>"${data}/replica/postgresql.conf"
      '';
      depends_on.db-setup.condition = "process_completed_successfully";
    };
    replica = {
      command = service "replica" ''exec ${pg "postgres"} -D "${data}/replica"'';
      depends_on.replica-init.condition = "process_completed_successfully";
      readiness_probe = pgReady "replica" "$XMTP_BACKEND_REPLICA_PORT";
      shutdown.signal = 2;
    };
    s3 = {
      # A Linux build directory has no extended attributes, so VersityGW
      # keeps bucket and object metadata in sidecar files.
      command = service "s3" ''
        ${bin versitygw "versitygw"} --version
        ${bin coreutils "mkdir"} -p "$XMTP_SERVICES_STATE/s3/objects" "$XMTP_SERVICES_STATE/s3/metadata"
        exec ${bin coreutils "env"} -i HOME="$XMTP_SERVICES_STATE" \
          ROOT_ACCESS_KEY=xmtps3 ROOT_SECRET_KEY=xmtps3secret \
          ${bin versitygw "versitygw"} --port "127.0.0.1:$XMTP_S3_PORT" --cors-allow-origin '*' \
          --health /health posix --sidecar "$XMTP_SERVICES_STATE/s3/metadata" \
          "$XMTP_SERVICES_STATE/s3/objects"
      '';
      readiness_probe = probe "s3" ''${bin curl "curl"} -sf "$XMTP_S3_URL/health"'';
    };
    s3-init = {
      # The Docker stack's bucket settings.
      command = service "s3-init" ''
        ${aws} s3api create-bucket --bucket attachments
        ${aws} s3api put-bucket-policy --bucket attachments \
          --policy file://${repo + "/dev/docker/s3/public-read.json"}
        ${aws} s3api put-bucket-cors --bucket attachments \
          --cors-configuration file://${repo + "/dev/docker/s3/cors.json"}
      '';
      depends_on.s3.condition = "process_healthy";
    };
  };

  client = {
    anvil = {
      command = service "anvil" ''
        exec ${bin foundry-bin "anvil"} --host 127.0.0.1 --port "$XMTP_ANVIL_PORT" --base-fee 100
      '';
      readiness_probe = probe "anvil" ''
        ${bin curl "curl"} -sf -H 'content-type: application/json' \
          -d '{"jsonrpc":"2.0","id":1,"method":"eth_chainId","params":[]}' "$ANVIL_URL"
      '';
    };
    backend = {
      # The shared local config, with this run's listen and metrics addresses.
      command = service "backend" ''
        config="$XMTP_SERVICES_STATE/backend.toml"
        ${bin python3 "python3"} - ${repo + "/dev/backend/local-s3.toml"} "$config" <<'PY'
        import os, re, sys
        text = open(sys.argv[1]).read()
        listen = f'{os.environ["XMTP_BACKEND_LISTEN_HOST"]}:{os.environ["XMTP_BACKEND_PORT"]}'
        text, count = re.subn(r'(?m)^listen = .*$', f'listen = "{listen}"', text)
        assert count == 1, "expected one [server] listen line"
        assert "[telemetry]" not in text, "expected no [telemetry] table"
        metrics = f'127.0.0.1:{os.environ["XMTP_BACKEND_METRICS_PORT"]}'
        text += f'\n[telemetry]\nmetrics_listen = "{metrics}"\n'
        open(sys.argv[2], "w").write(text)
        PY
        # A Linux build has no system trust store; the Docker image ships cacert.
        export SSL_CERT_FILE=${cacert}/etc/ssl/certs/ca-bundle.crt
        XMTP_DATABASE_URL="$DATABASE_URL" exec ${backend}/bin/xmtp-backend --config-file "$config"
      '';
      depends_on = {
        db.condition = "process_healthy";
        replica.condition = "process_healthy";
        s3-init.condition = "process_completed_successfully";
        anvil.condition = "process_healthy";
      };
      readiness_probe = probe "backend" ''
        ${bin grpc-health-probe "grpc-health-probe"} -addr="127.0.0.1:$XMTP_BACKEND_PORT" \
          -connect-timeout=1s -rpc-timeout=1s
      '';
    };
    toxiproxy = {
      # The Docker stack's proxy file, with this run's addresses.
      command = service "toxiproxy" ''
        ${bin jq "jq"} --arg listen "127.0.0.1:$XMTP_TOXIPROXY_PORT" \
          --arg upstream "127.0.0.1:$XMTP_BACKEND_PORT" \
          'map(select(.name == "backend") | .listen = $listen | .upstream = $upstream)' \
          ${repo + "/dev/docker/toxiproxy/config.json"} >"$XMTP_SERVICES_STATE/toxiproxy.json"
        exec ${bin toxiproxy "toxiproxy-server"} -host 127.0.0.1 -port "$XMTP_TOXIPROXY_API_PORT" \
          -config "$XMTP_SERVICES_STATE/toxiproxy.json"
      '';
      depends_on.backend.condition = "process_healthy";
      readiness_probe = probe "toxiproxy" ''${bin curl "curl"} -sf "$XMTP_TOXIPROXY_API/version"'';
    };
  };
in
{
  # process-compose reads `.env` from its working directory and prefers it to
  # the environment (src/loader/loader.go:153-164).
  cli.environment.PC_DISABLE_DOTENV = true;
  cli.preHook = ''
    # No user settings, themes, or shortcuts: the same stack everywhere. This
    # also stops a debug line about a missing config directory on each call.
    export PROC_COMP_CONFIG="''${PROC_COMP_CONFIG:-${pkgs.emptyDirectory}}"
    # Client subcommands (down, process, project, attach) reach a running
    # stack through PC_SOCKET_PATH and need no setup.
    case "''${1:-up}" in
      up | -*)
        if [ -z "''${XMTP_SERVICES_STATE:-}" ]; then
          parent="''${TMPDIR:-/tmp}"
          if [ "$(uname -s)" = Linux ] && [ -d /dev/shm ] && [ -w /dev/shm ]; then
            parent=/dev/shm
          fi
          XMTP_SERVICES_STATE="$(${bin coreutils "mktemp"} -d "''${parent%/}/xmtp-services.XXXXXX")"
        fi
        export XMTP_SERVICES_STATE
        ${bin coreutils "mkdir"} -p "$XMTP_SERVICES_STATE/data"
        if [ -n "''${XMTP_SERVICES_LOGS:-}" ]; then
          ${bin coreutils "mkdir"} -p "$XMTP_SERVICES_LOGS"
        fi
        assigned="$(${bin python3 "python3"} ${freePorts} ${lib.concatStringsSep " " ports})"
        eval "$assigned"
        ${lib.concatStrings (lib.mapAttrsToList (name: value: "export ${name}=\"${value}\"\n") urls)}
        export XMTP_BACKEND_LISTEN_HOST="''${XMTP_BACKEND_LISTEN_HOST:-127.0.0.1}"
        export PC_LOG_FILE="''${PC_LOG_FILE:-$XMTP_SERVICES_STATE/process-compose.log}"
        # Consumers source this file; stdout stays free for process-compose.
        {
          ${lib.concatMapStrings (name: ''
            printf "export %s='%s'\n" ${name} "''$${name}"
          '') envNames}
        } >"$XMTP_SERVICES_STATE/env"
        unset OTEL_EXPORTER_OTLP_ENDPOINT
        ;;
    esac
  '';

  settings = {
    # Stop the clients of a service before the service.
    ordered_shutdown = true;
    processes =
      storage
      // lib.optionalAttrs withBackend client
      // {
        # Run XMTP_SERVICES_TEST_COMMAND once every service is ready. Only
        # `services-test` enables this process; it then exits with the
        # command's status and stops every service.
        test = {
          command = script "test" ''
            [ -n "''${XMTP_SERVICES_TEST_COMMAND:-}" ] || {
              echo 'services-test: set XMTP_SERVICES_TEST_COMMAND' >&2
              exit 2
            }
            # process-compose runs a process whose dependency was skipped
            # before the process started waiting (src/app/project_runner.go:
            # 287-322 at v1.120.0: a skipped process is neither running nor
            # done), so check that every service listens.
            for port in ${lib.concatMapStringsSep " " (name: ''"''$${name}"'') ports}; do
              if ! (exec 3<>"/dev/tcp/127.0.0.1/$port") 2>/dev/null; then
                echo "services-test: nothing listens on port $port; a service failed or was skipped" >&2
                exit 1
              fi
            done
            ${bin coreutils "touch"} "$XMTP_SERVICES_STATE/test-started"
            exec ${pkgs.bash}/bin/bash -c "$XMTP_SERVICES_TEST_COMMAND"
          '';
          disabled = true;
          depends_on =
            if withBackend then
              { toxiproxy.condition = "process_healthy"; }
            else
              {
                replica.condition = "process_healthy";
                s3-init.condition = "process_completed_successfully";
              };
        };
      };
  };
}
