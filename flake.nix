{
  description = "Interactive AWS profile selector";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      inherit (nixpkgs) lib;
      manifest = (lib.importTOML ./Cargo.toml).package;
      # nixpkgs 26.11 dropped x86_64-darwin; darwin stdenv has no Intel bootstrap files
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forEachSystem =
        f:
        lib.genAttrs supportedSystems (
          system:
          f (
            import nixpkgs {
              inherit system;
              overlays = [ self.overlays.default ];
            }
          )
        );
      source = lib.fileset.toSource {
        root = ./.;
        fileset = lib.fileset.unions [
          ./Cargo.toml
          ./Cargo.lock
          ./src
          ./tests
          ./shell
        ];
      };
      shellIntegration = "share/${manifest.name}/awsps.nu";
    in
    {
      overlays.default = lib.composeManyExtensions [
        rust-overlay.overlays.default
        (
          final: prev:
          let
            rustPlatform = prev.makeRustPlatform {
              cargo = final.rustToolchain;
              rustc = final.rustToolchain;
            };
          in
          {
            rustToolchain = prev.rust-bin.stable.latest.default.override {
              extensions = [
                "rust-src"
                "rust-analyzer"
              ];
            };

            aws-profile-selector = rustPlatform.buildRustPackage {
              pname = manifest.name;
              inherit (manifest) version;
              src = source;
              cargoLock.lockFile = ./Cargo.lock;
              nativeCheckInputs = [
                prev.bash
                prev.coreutils
              ];
              passthru = {
                awsProfileContextVersion = 1;
                awsProfileConfigVersion = 1;
              };

              postInstall = ''
                install -Dm644 shell/awsps.nu "$out/${shellIntegration}"
                install -Dm644 shell/awsps.bash "$out/share/${manifest.name}/awsps.bash"
              '';

              meta = {
                inherit (manifest) description;
                homepage = manifest.repository;
                license = lib.licenses.mit;
                mainProgram = manifest.name;
                platforms = supportedSystems;
              };
            };
          }
        )
      ];

      packages = forEachSystem (pkgs: {
        default = pkgs.aws-profile-selector;
        inherit (pkgs) aws-profile-selector;
      });

      checks = forEachSystem (pkgs: {
        package = pkgs.aws-profile-selector;
        clippy = pkgs.aws-profile-selector.overrideAttrs (_: {
          pname = "${manifest.name}-clippy";
          buildPhase = ''
            runHook preBuild
            cargo clippy --offline --locked --all-targets -- -D warnings
            runHook postBuild
          '';
          doCheck = false;
          installPhase = "touch $out";
          postInstall = "";
        });
        formatting =
          pkgs.runCommand "${manifest.name}-formatting"
            {
              nativeBuildInputs = [
                pkgs.rustToolchain
                pkgs.nixfmt
              ];
            }
            ''
              cp -r ${source} source
              chmod -R +w source
              cd source
              cargo fmt --all -- --check
              nixfmt --check ${./flake.nix}
              touch "$out"
            '';
        shells =
          pkgs.runCommand "${manifest.name}-shells"
            {
              nativeBuildInputs = [
                pkgs.aws-profile-selector
                pkgs.bash
                pkgs.coreutils
                pkgs.fish
                pkgs.nushell
              ];
            }
            ''
              mkdir -p work/shell
              cp -r ${source}/tests work/tests
              ln -s ${pkgs.aws-profile-selector}/${shellIntegration} work/shell/awsps.nu
              ln -s ${pkgs.aws-profile-selector}/share/${manifest.name}/awsps.bash work/shell/awsps.bash
              cd work
              export HOME="$(mktemp -d)"
              bash tests/bash.sh ${pkgs.aws-profile-selector}/bin/${manifest.name}
              nu --no-config-file tests/nushell.nu ${pkgs.aws-profile-selector}/bin/${manifest.name}
              export HOME="$(mktemp -d)"
              nu --no-config-file tests/shells.nu
              touch "$out"
            '';
        interactive =
          pkgs.runCommand "${manifest.name}-interactive"
            {
              nativeBuildInputs = [
                pkgs.aws-profile-selector
                pkgs.bash
                pkgs.nushell
                pkgs.python3
              ];
            }
            ''
              cd ${source}
              python3 tests/interactive.py ${pkgs.aws-profile-selector}/bin/${manifest.name}
              touch "$out"
            '';
      });

      formatter = forEachSystem (pkgs: pkgs.nixfmt);

      devShells = forEachSystem (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            rustToolchain
            cargo-deny
            cargo-edit
            nixfmt
            nushell
            python3
            fish
          ];
        };
      });
    };
}
