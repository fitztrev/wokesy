{ pkgs, ... }:

{
  languages.rust = {
    enable = true;
    channel = "stable";
    components = [ "rustc" "cargo" "clippy" "rustfmt" "rust-analyzer" "rust-src" ];
  };

  packages = [
    pkgs.bacon
    pkgs.cargo-edit
    pkgs.cargo-nextest
    pkgs.gh
    pkgs.git-cliff
    pkgs.jq
    pkgs.kcl
    pkgs.kubeconform
    pkgs.skopeo
  ];

  env.WOKESY_TELEMETRY__LOG_FORMAT = "pretty";

  scripts.run-on-zulip.exec = ''cd "$DEVENV_ROOT" && exec secretspec run -- cargo run'';

  git-hooks.hooks.nixpkgs-pin = {
    enable = true;
    entry = "ci/nixpkgs-pin.sh";
    files = "^(flake|devenv)\\.lock$";
    pass_filenames = false;
  };

  scripts.render-manifests.exec = ''cd "$DEVENV_ROOT/deploy" && exec kcl run --quiet prod/main.k'';

  scripts.test-manifests.exec = ''cd "$DEVENV_ROOT/deploy" && exec kcl test ./...'';

  scripts.release.exec = ''cd "$DEVENV_ROOT" && exec nix run "$DEVENV_ROOT#release" -- "$@"'';

  scripts.deploy.exec = ''
    set -eu
    if [ $# -ne 1 ]; then
      echo "usage: deploy <version>" >&2
      exit 2
    fi
    version=''${1#v}
    gh workflow run deploy.yml --repo lichess-org/wokesy -f version="$version"
    echo "Requested a production deploy of $version. Follow it with:"
    echo "  gh run list --repo lichess-org/wokesy --workflow deploy.yml"
    echo "  gh run watch --repo lichess-org/wokesy"
  '';

  enterTest = ''
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo nextest run
    cargo test --doc
    test-manifests
  '';
}
