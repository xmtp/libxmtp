#!/usr/bin/env bash
set -euo pipefail

repository_root="$(git rev-parse --show-toplevel)"
output_path="${1:-apps/docs/generated/reference/swift}"
case "${output_path}" in
  /*) ;;
  *) output_path="${repository_root}/${output_path}" ;;
esac
mkdir -p "$(dirname "${output_path}")"

documentation_package="$(mktemp -d)"
trap 'rm -rf "${documentation_package}"' EXIT

cp "${repository_root}/Package.swift" "${documentation_package}/Package.swift"
cp "${repository_root}/Package.resolved" "${documentation_package}/Package.resolved"
mkdir -p "${documentation_package}/sdks/ios"
cp -R "${repository_root}/sdks/ios/Sources" "${documentation_package}/sdks/ios/Sources"
cp -R "${repository_root}/sdks/ios/Tests" "${documentation_package}/sdks/ios/Tests"
ln -s "${repository_root}/bindings" "${documentation_package}/bindings"

# Hide generated declarations from DocC without changing their access or behavior.
while IFS= read -r generated_file; do
  annotated_file="${generated_file}.docc"
  awk '
    /@_documentation\(visibility: (private|internal)\)/ {
      has_documentation_visibility = 1
    }
    /^[[:space:]]*(public|open)[[:space:]]/ {
      if (!has_documentation_visibility) {
        match($0, /^[[:space:]]*/)
        print substr($0, 1, RLENGTH) "@_documentation(visibility: internal)"
      }
      has_documentation_visibility = 0
    }
    { print }
  ' "${generated_file}" > "${annotated_file}"
  mv "${annotated_file}" "${generated_file}"
done < <(
  find \
    "${documentation_package}/sdks/ios/Sources/XMTPiOS/Libxmtp/xmtpv3.swift" \
    "${documentation_package}/sdks/ios/Sources/XMTPiOS/Proto" \
    -type f -name '*.swift' -print
)

cd "${documentation_package}"
swift package \
  --scratch-path "${repository_root}/.build/xmtp-docc" \
  --disable-automatic-resolution \
  --allow-writing-to-directory "${output_path}" \
  generate-documentation \
  --target XMTPiOS \
  --disable-indexing \
  --hosting-base-path reference/swift \
  --output-path "${output_path}"
