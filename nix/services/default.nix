# The test services as one process-compose definition (stack.nix).
#
#   nix run .#services                        # the stack in the foreground
#   dev/services up | env | ps | logs | down  # this worktree's stack, in the background
#   XMTP_SERVICES_TEST_COMMAND=CMD nix run .#services-test
#                                             # start, run CMD, exit with its status, stop
#   nix build .#checks.<system>.services      # the stack inside a Nix build
#
# `legacyPackages.mkServices backend` gives the same outputs for another
# backend binary, so a test derivation keys on the binary that it starts.
# `mkServices null` is the storage subset: PostgreSQL, the replica, and S3.
{ inputs, ... }:
{
  perSystem =
    {
      pkgs,
      lib,
      self',
      ...
    }:
    let
      repo = lib.fileset.toSource {
        root = ../..;
        fileset = lib.fileset.unions [
          ../../dev/backend/local-s3.toml
          ../../dev/docker/s3/public-read.json
          ../../dev/docker/s3/cors.json
          ../../dev/docker/toxiproxy/config.json
        ];
      };
      mkServices =
        backend:
        let
          # Not the process-compose-flake flake module: its generated check has
          # no __darwinAllowLocalNetworking, and its API server listens on TCP
          # port 8080, so parallel builds on one loopback would collide.
          outputs =
            (lib.evalModules {
              specialArgs = {
                inherit pkgs;
                name = "services";
              };
              modules = [
                "${inputs.process-compose-flake}/nix/process-compose"
                (import ./stack.nix { inherit repo backend; })
              ];
            }).config.outputs;
          # Start, wait, run XMTP_SERVICES_TEST_COMMAND, exit with its status,
          # and stop every service. It removes the state directory that it
          # creates. When the command never started and XMTP_SERVICES_LOGS is
          # set, it prints the end of each service log.
          test = pkgs.writeShellApplication {
            name = "services-test";
            runtimeInputs = [ pkgs.coreutils ];
            text = ''
              if [ -z "''${XMTP_SERVICES_STATE:-}" ]; then
                parent="''${TMPDIR:-/tmp}"
                if [ "$(uname -s)" = Linux ] && [ -d /dev/shm ] && [ -w /dev/shm ]; then
                  parent=/dev/shm
                fi
                XMTP_SERVICES_STATE="$(mktemp -d "''${parent%/}/xmtp-services.XXXXXX")"
                trap 'rm -rf "$XMTP_SERVICES_STATE"' EXIT
              fi
              export XMTP_SERVICES_STATE
              status=0
              ${lib.getExe outputs.testPackage} up -t=false --no-server "$@" || status=$?
              if [ "$status" -ne 0 ] && [ ! -e "$XMTP_SERVICES_STATE/test-started" ] &&
                [ -n "''${XMTP_SERVICES_LOGS:-}" ]; then
                for log in "$XMTP_SERVICES_LOGS"/*.log; do
                  [ -f "$log" ] || continue
                  echo "--- last lines of $log" >&2
                  tail -n 40 "$log" >&2
                done
              fi
              exit "$status"
            '';
          };
        in
        {
          inherit (outputs) package;
          inherit test;
          # A check-phase command that runs `command` (a store script) with the
          # services. The derivation must set __darwinAllowLocalNetworking
          # (or __noChroot): a Darwin sandbox denies loopback networking.
          checkCommand = command: ''
            HOME="$TMPDIR" XMTP_SERVICES_LOGS="$TMPDIR/services-logs" \
              XMTP_SERVICES_TEST_COMMAND=${command} ${lib.getExe test}
          '';
        };
      services = mkServices self'.packages.xmtp-backend;
      healthCheck = pkgs.writeShellApplication {
        name = "services-health-check";
        runtimeInputs = with pkgs; [
          postgresql_18
          awscli2
          curl
          jq
          grpc-health-probe
          coreutils
          gnugrep
        ];
        text = builtins.readFile ./health-check.sh;
      };
    in
    {
      legacyPackages = {
        inherit mkServices;
        storageServices = mkServices null;
      };
      packages = {
        services = services.package;
        services-test = services.test;
      };
      # Each service with its own client, inside a Nix build.
      checks.services =
        pkgs.runCommand "services-check"
          {
            __darwinAllowLocalNetworking = true;
          }
          ''
            ${services.checkCommand (lib.getExe healthCheck)}
            touch "$out"
          '';
    };
}
