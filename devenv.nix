{ pkgs, lib, config, inputs, ... }:

{
  packages = with pkgs; [
    pkg-config
    openssl
  ];

  languages.rust.enable = true;
}
