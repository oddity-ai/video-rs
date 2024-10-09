{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem
      (system:
        let
          overlays = [ (import rust-overlay) ];
          pkgs = import nixpkgs {
            inherit system overlays;
          };
          nativeBuildInputs = with pkgs; [
            (pkgs.pkgsBuildHost.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml)

            # Used by ffmpeg-next-sys to find ffmpeg libraries.
            pkg-config
            # Used by ffmpeg-next-sys via bindgen to generate bindings.
            llvmPackages.libclang

            # Without this bindgen will not be able to find libclang and won't be
            # able to generate bindings.
            rustPlatform.bindgenHook
          ];
          buildInputs = with pkgs; [
            ffmpeg
          ];
        in
        with pkgs;
        {
          devShells.default = mkShell {
            inherit buildInputs nativeBuildInputs;
            # This is needed or rust-analyzer will not work correctly.
            # Source: https://discourse.nixos.org/t/rust-src-not-found-and-other-misadventures-of-developing-rust-on-nixos/11570
            RUST_SRC_PATH = "${pkgs.rust-bin.stable.latest.default.override { extensions = [ "rust-src" ]; }}/lib/rustlib/src/rust/library";
          };
        }
      );
}
