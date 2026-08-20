# Nix

## Install with a Nix profile

Install `rustdirstat` into your default Nix profile:

```bash
nix profile add github:lnorton89/rustdirstat
```

## Uninstall with a Nix profile

Remove `rustdirstat` from your Nix profile:

```bash
nix profile remove rustdirstat
```

If the profile entry has a different name, list your installed profiles first:

```bash
nix profile list
```

Then remove the appropriate name or index.

## Install declaratively with a flake input

If you maintain your own Nix configuration with flakes, add `rustdirstat` as an input in your `flake.nix`:

```nix
{
  inputs = {
    # Keep your existing inputs here.

    rustdirstat = {
      url = "github:lnorton89/rustdirstat";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };
}
```

Then add the package to whatever package list your own configuration uses. The exact location depends on your setup, for example Home Manager, NixOS, nix-darwin, or another Nix-based configuration system.

The package expression is:

```nix
inputs.rustdirstat.packages.${pkgs.stdenv.hostPlatform.system}.default
```

For example, in a module that receives `inputs` and `pkgs`:

```nix
{ config, pkgs, inputs, ... }:

{
  # Add the package to the appropriate package list for your setup.
  #
  # Examples, depending on your configuration system:
  #
  # Home Manager:
  # home.packages = [
  #   inputs.rustdirstat.packages.${pkgs.stdenv.hostPlatform.system}.default
  # ];
  #
  # NixOS:
  # environment.systemPackages = [
  #   inputs.rustdirstat.packages.${pkgs.stdenv.hostPlatform.system}.default
  # ];
}
```

If `inputs` is not already available in that file, pass it through from your flake outputs or reference the flake input from the surrounding scope.

After updating your configuration, rebuild or switch it using your usual command.

For command-line and GUI usage instructions after installing with Nix, see [README.md](README.md).
