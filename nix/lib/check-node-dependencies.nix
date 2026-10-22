{
  lib,
  runCommand,
  node,
  dependencies,
  unpatchedDependencies,
}:
let
  usesDependency =
    package: dependency:
    let
      selected = builtins.filter (input: input.pname or null == dependency.pname) package.buildInputs;
    in
    builtins.length selected == 1 && (builtins.head selected).drvPath == dependency.drvPath;
  sharedLibraries = {
    inherit (dependencies)
      brotli
      nghttp2
      ngtcp2
      openssl
      ;
    cares = dependencies.c-ares;
  };
  directChecks = lib.mapAttrsToList (
    name: dependency:
    lib.assertMsg (
      dependency.drvPath != unpatchedDependencies.${dependency.pname}.drvPath
    ) "Node's scoped ${dependency.pname} must not resolve to the unpatched package"
    && lib.assertMsg (usesDependency node dependency) "Node ${node.version} must use scoped ${dependency.pname} in buildInputs"
    && lib.assertMsg (
      builtins.elem "--shared-${name}" node.configureFlags
      && builtins.elem "--shared-${name}-libpath=${lib.getLib dependency}/lib" node.configureFlags
    ) "Node ${node.version} must link scoped ${dependency.pname}"
  ) sharedLibraries;
  transitiveChecks =
    map
      (
        edge:
        lib.assertMsg (usesDependency dependencies.${edge.package}
          dependencies.${edge.dependency}
        ) "Node's ${edge.package} must use scoped ${edge.dependency}"
      )
      [
        {
          package = "nghttp2";
          dependency = "c-ares";
        }
        {
          package = "nghttp2";
          dependency = "openssl";
        }
        {
          package = "ngtcp2";
          dependency = "brotli";
        }
        {
          package = "ngtcp2";
          dependency = "openssl";
        }
      ];
  simdutf = builtins.filter (input: input.pname or null == "simdutf") node.buildInputs;
  simdutfCheck = lib.assertMsg (
    builtins.length simdutf == 1
    && builtins.elem "-DSIMDUTF_CXX_STANDARD=20" (builtins.head simdutf).cmakeFlags
  ) "Node ${node.version} must build simdutf with C++20";
in
assert builtins.all (check: check) (directChecks ++ transitiveChecks ++ [ simdutfCheck ]);
runCommand "nodejs-${node.version}-dependencies" { } ''
  touch "$out"
''
