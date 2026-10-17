# Compiler path remapping belongs here, after Cargo computes its fingerprints.
# Both dependency and final builds invoke the same store wrapper and keep all
# caller flags. No source file or completed artifact is rewritten.
set -euo pipefail
mode=$1
shift
build_root=${NIX_BUILD_TOP:?Nix must provide its build root}
case "$mode" in
  rustc)
    exec "$@" "--remap-path-prefix=$build_root=/build"
    ;;
  cc|cxx)
    if [[ "$mode" == cc ]]; then
      compiler=${XMTP_NATIVE_CC:?native compiler is not set}
    else
      compiler=${XMTP_NATIVE_CXX:?native C++ compiler is not set}
    fi
    args=()
    for arg in "$@"; do
      case "$arg" in
        # Vendored static OpenSSL records its temporary installation prefix in
        # these two configured macros. File-prefix-map does not remap literals.
        -DENGINESDIR=*|-DMODULESDIR=*) arg=${arg//"$build_root"/\/build} ;;
      esac
      args+=("$arg")
    done
    exec "$compiler" "-ffile-prefix-map=$build_root=/build" "${args[@]}"
    ;;
  *)
    echo "error: unknown Apple compiler role: $mode" >&2
    exit 1
    ;;
esac
