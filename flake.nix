{
  description = "A cross-platform WinDirStat clone with a native desktop GUI and a terminal UI";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    fenix.url = "github:nix-community/fenix";
  };

  outputs = { self, nixpkgs, flake-utils, fenix }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        inherit (pkgs) lib stdenv;

        cargoToml = lib.importTOML ./Cargo.toml;

        # Needed to *link* the desktop GUI. The terminal UI needs none of
        # this, but both binaries come out of one crate, so the build
        # needs them regardless of which one you end up running.
        # `pkgs.libx11 or pkgs.xorg.libX11`: recent nixpkgs moved these to
        # the top level and deprecated the `xorg` set, but a consumer
        # pointing `nixpkgs` at a stable channel only has the old names.
        # The `or` fallback works on both instead of picking a side.
        guiBuildInputs = lib.optionals stdenv.hostPlatform.isLinux [
          pkgs.libxkbcommon
          pkgs.wayland
          (pkgs.libx11 or pkgs.xorg.libX11)
          (pkgs.libxcursor or pkgs.xorg.libXcursor)
          (pkgs.libxi or pkgs.xorg.libXi)
          (pkgs.libxrandr or pkgs.xorg.libXrandr)
        ];

        # ...and to *load* it. winit and wgpu resolve these through
        # dlopen at startup rather than at link time, so they have to be
        # on the runtime search path too or the GUI dies on launch with a
        # missing-library error that the build gave no warning about.
        guiRuntimeLibs = guiBuildInputs ++ lib.optionals stdenv.hostPlatform.isLinux [
          pkgs.vulkan-loader
          pkgs.libGL
        ];

        darwinFrameworks = lib.optionals stdenv.hostPlatform.isDarwin [
          pkgs.darwin.apple_sdk.frameworks.AppKit
          pkgs.darwin.apple_sdk.frameworks.CoreGraphics
          pkgs.darwin.apple_sdk.frameworks.Foundation
          pkgs.darwin.apple_sdk.frameworks.Metal
          pkgs.darwin.apple_sdk.frameworks.QuartzCore
        ];

        # The pinned toolchain, from the same file rustup reads. The
        # shell used to take unstable's cargo/rustc/clippy/rustfmt, which
        # drifted from the pin in rust-toolchain.toml — a lint the pinned
        # compiler in CI could see could not be reproduced here.
        # `fromToolchainFile` reads the channel and components straight
        # out of that file, so the pin lives in exactly one place. (The
        # `sha256` is the hash of the toolchain file itself.)
        toolchain = fenix.packages.${system}.fromToolchainFile {
          file = ./rust-toolchain.toml;
          sha256 = "sha256-P30Tm3O7vQAE725YtDCDHGjNrSsfZO4us11UwJGZSJo=";
        };

        rustdirstat = pkgs.rustPlatform.buildRustPackage {
          pname = cargoToml.package.name;
          inherit (cargoToml.package) version;
          src = self;

          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = [ pkgs.pkg-config ]
            ++ lib.optional stdenv.hostPlatform.isLinux pkgs.makeWrapper;
          buildInputs = guiBuildInputs ++ darwinFrameworks;

          # The pty stress test drives the real binary through a terminal
          # and the GUI tests need a display; neither survives the
          # sandbox. `cargo test --lib` covers the logic either way, and
          # CI runs the whole suite on three platforms.
          doCheck = false;

          postInstall = lib.optionalString stdenv.hostPlatform.isLinux ''
            wrapProgram $out/bin/rustdirstat-gui \
              --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath guiRuntimeLibs}
          '';

          meta = {
            inherit (cargoToml.package) description;
            homepage = cargoToml.package.repository;
            license = lib.licenses.mit;
            mainProgram = "rustdirstat";
            platforms = lib.platforms.unix ++ lib.platforms.windows;
          };
        };
      in
      {
        packages = {
          default = rustdirstat;
          inherit rustdirstat;
        };

        # Written out rather than built with `flake-utils.lib.mkApp`,
        # which produces apps without a `meta` attribute and makes
        # `nix flake check` complain about both of them.
        apps = {
          default = {
            type = "app";
            program = "${rustdirstat}/bin/rustdirstat";
            meta = rustdirstat.meta;
          };
          gui = {
            type = "app";
            program = "${rustdirstat}/bin/rustdirstat-gui";
            meta = rustdirstat.meta // { mainProgram = "rustdirstat-gui"; };
          };
        };

        devShells.default = pkgs.mkShell {
          inputsFrom = [ rustdirstat ];
          packages = [
            # The pinned toolchain — the whole point of the shell is that
            # `cargo clippy` here is the same clippy CI runs.
            toolchain
            pkgs.rust-analyzer
          ];

          # `cargo run` inside the shell builds an unwrapped binary, so it
          # needs the dlopen-time libraries pointed at explicitly the same
          # way postInstall does for the installed one.
          LD_LIBRARY_PATH = lib.optionalString stdenv.hostPlatform.isLinux
            (lib.makeLibraryPath guiRuntimeLibs);
          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
        };

        formatter = pkgs.nixpkgs-fmt;
      });
}
