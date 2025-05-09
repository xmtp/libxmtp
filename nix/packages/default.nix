{ inputs, self, ... }: {
  perSystem = { self', pkgs, inputs', ... }:
    let
      craneLib = inputs.crane.mkLib pkgs;
      muslToolchain = craneLib.overrideToolchain (pkgs.xmtp.mkToolchain [ "x86_64-unknown-linux-musl" ] [ ]);
      wasmToolchain = craneLib.overrideToolchain (pkgs.xmtp.mkToolchain [ "wasm32-unknown-unknown" ] [ ]);
      nativeToolchain = craneLib.overrideToolchain (pkgs.xmtp.mkToolchain [ ] [ ]);
      nativeArtifacts = pkgs.callPackage pkgs.xmtp.mkWorkspace { craneLib = nativeToolchain; };
      muslArtifacts = pkgs.pkgsCross.musl64.callPackage pkgs.xmtp.mkWorkspace { craneLib = muslToolchain; rustTarget = "x86_64-unknown-linux-musl"; };
      validationServiceDocker = inputs'.nix2container.packages.nix2container.buildImage {
        name = "ghcr.io/xmtp/mls-validation-service"; # override ghcr images
        tag = "main";
        # ugly workaround https://github.com/nlewo/nix2container/issues/89
        created = "${builtins.substring 0 4 self.lastModifiedDate}-${builtins.substring 4 2 self.lastModifiedDate}-${builtins.substring 6 2 self.lastModifiedDate}T${builtins.substring 8 2 self.lastModifiedDate}:${builtins.substring 10 2 self.lastModifiedDate}:${builtins.substring 12 2 self.lastModifiedDate}Z";
        config = {
          Env = [
            "ANVIL_URL=http://localhost:8545"
          ];
          entrypoint = [ "${self'.packages.validationServiceMusl}/bin/mls-validation-service" ];
        };
      };
      xdbgDocker = inputs'.nix2container.packages.nix2container.buildImage {
        name = "xmtp/xdbg";
        tag = "main";
        created = "${builtins.substring 0 4 self.lastModifiedDate}-${builtins.substring 4 2 self.lastModifiedDate}-${builtins.substring 6 2 self.lastModifiedDate}T${builtins.substring 8 2 self.lastModifiedDate}:${builtins.substring 10 2 self.lastModifiedDate}:${builtins.substring 12 2 self.lastModifiedDate}Z";
        config.entrypoint = [ "${self'.packages.xdbgMusl}/bin/xdbg" ];
      };
      # Build a crate with "name" and at path "workspacePath" from the root of the workspace
      buildCrate = { crate, path ? crate, artifacts, extraArgs ? "" }:
        let
          workspaceFilesets = pkgs.xmtp.filesets {
            inherit (pkgs) lib; inherit (artifacts) craneLib;
          };
          src = workspaceFilesets.filesetForCrate ./. + "/../../${path}";
        in
        artifacts.craneLib.buildPackage ({
          inherit src;
          inherit (artifacts) cargoArtifacts;
          inherit (craneLib.crateNameFromCargoToml { cargoToml = ./. + "/../../${path}/Cargo.toml"; }) pname version;
          cargoExtraArgs = "-p ${crate}" + " " + extraArgs;
          doCheck = false;
        } // artifacts.commonArgs);
    in
    {
      packages = {
        xdbg = buildCrate { crate = "xdbg"; path = "xmtp_debug"; artifacts = nativeArtifacts; };
        xdbgMusl = buildCrate { crate = "xdbg"; path = "xmtp_debug"; artifacts = muslArtifacts; };
        validationService = buildCrate { crate = "mls_validation_service"; artifacts = nativeArtifacts; extraArgs = "--features test-utils"; };
        bindingsWasm = (pkgs.callPackage ./bindings_wasm.nix { toolchain = wasmToolchain; }).bin;
        validationServiceMusl = buildCrate { crate = "mls_validation_service"; artifacts = muslArtifacts; extraArgs = "--features test-utils"; };
        inherit validationServiceDocker xdbgDocker;
      };
    };
}
