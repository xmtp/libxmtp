_: {
  perSystem = { self', pkgs, lib, ... }:
    let
      src = ./..;
      xnetFileset = crate: lib.fileset.toSource {
        root = ./..;
        fileset = lib.fileset.unions [
          pkgs.xmtp.filesets.libraries
          (pkgs.xmtp.craneLib.fileset.commonCargoSources (src + /apps/xnet/lib))
          crate
        ];
      };
    in
    { };
}
