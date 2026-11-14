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

  enterTest = ''
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo nextest run
    cargo test --doc
  '';
}
