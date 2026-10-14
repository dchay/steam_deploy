PROJECT

steam_deploy

ALIAS

PinkChoco Deploy

PURPOSE

Command-line deployment tool for:

- Unreal Engine 5.8.3
- Steam publishing
- Steam depot uploads
- Steam build verification

PLATFORM

Windows 11

LANGUAGE

Rust Edition 2024

TOOLCHAIN

1.98.0

TARGET

x86_64-pc-windows-msvc

CURRENT SOURCE TREE

src/
lib.rs
main.rs

models/
config.rs

stages/
validate.rs
build.rs
vdf.rs

CURRENT STATUS

Deployment pipeline reconstruction phase.

KNOWN MISSING COMPONENTS

upload.rs

verify.rs

PRIMARY GOAL

Automate the manual UE build and Steam publishing workflow.