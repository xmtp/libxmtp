# Ruby directly spawns RN's `command -v ccache` probe. Keep it in the store.
{ writeShellScriptBin }:
writeShellScriptBin "command" ''
  command "$@"
''
