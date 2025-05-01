{ dprint
, stylua
, deno
, nixfmt
, yamlfmt
, rubyPackages
, dotenv-linter
, html-tidy
, statix
, deadnix
, markdownlint-cli
, shellcheck
, golangci-lint
, ktlint
, mkShell
, taplo
, extraInputs ? { }
}:
(mkShell
  {
    name = "Common Linters + Formatters";
    buildInputs = [
      taplo
      dprint
      stylua
      deno
      nixfmt
      yamlfmt
      rubyPackages.htmlbeautifier

      # Linters
      # cbfmt
      dotenv-linter
      html-tidy
      statix
      deadnix
      markdownlint-cli
      shellcheck
      golangci-lint
      ktlint
    ];

  } // extraInputs)
