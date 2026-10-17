# Pin the agent CLI without updating the workspace's nixpkgs or Rust toolchain.
# Source and vendor hashes match nixpkgs' RTK 0.49.0 package.
{
  lib,
  rustPlatform,
  fetchFromGitHub,
  makeWrapper,
  pkg-config,
  sqlite,
  gitMinimal,
  writableTmpDirAsHomeHook,
  versionCheckHook,
}:
rustPlatform.buildRustPackage (finalAttrs: {
  pname = "rtk";
  version = "0.49.0";
  __structuredAttrs = true;
  src = fetchFromGitHub {
    owner = "rtk-ai";
    repo = "rtk";
    tag = "v${finalAttrs.version}";
    hash = "sha256-wlb+yPTsMiZsh3AKzLNM1eFpOvbnLgLb/cCsLG/YrJU=";
  };
  cargoHash = "sha256-cgRtXTd75uKInBnf6dP6e4KHyA2IP9lLEKwVzGq16gg=";
  nativeBuildInputs = [
    makeWrapper
    pkg-config
  ];
  buildInputs = [ sqlite ];
  env.LIBSQLITE3_SYS_USE_PKG_CONFIG = "1";
  postInstall = ''
    wrapProgram $out/bin/rtk --prefix PATH : ${lib.makeBinPath [ gitMinimal ]}
  '';
  nativeCheckInputs = [
    gitMinimal
    writableTmpDirAsHomeHook
  ];
  # Same exclusion as nixpkgs: this signal test times out in the build sandbox.
  checkFlags = [ "--skip=signalled_run_still_prints_captured_output" ];
  nativeInstallCheckInputs = [ versionCheckHook ];
  doInstallCheck = true;
  meta = {
    description = "Compact command output for coding agents";
    homepage = "https://github.com/rtk-ai/rtk";
    license = lib.licenses.asl20;
    mainProgram = "rtk";
  };
})
