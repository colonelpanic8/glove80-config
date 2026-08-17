set shell := ["bash", "-euo", "pipefail", "-c"]

firmware_repo := "dependencies/moergo-rmk"
control := "./bin/moergo-control"
glove80_config := "config/glove80.toml"
glove80_firmware_config := "config/firmware.toml"
go60_config := "config/go60.toml"
go60_firmware_config := "config/go60-firmware.toml"

init:
    git submodule update --init --recursive

check:
    {{ control }} config validate {{ glove80_config }}
    {{ control }} config validate {{ go60_config }}

parity-check: check
    bash -c 'cd {{ firmware_repo }} && nix develop path:. --command just parity-check'

glove80-check:
    {{ control }} config validate {{ glove80_config }}

go60-check:
    {{ control }} config validate {{ go60_config }}

go60-profile-check:
    config_path="$(pwd)/{{ go60_firmware_config }}"; \
        bash -c 'cd {{ firmware_repo }} && nix develop path:. --command cargo run --quiet -p xtask -- verify-config-profile crates/go60-rmk/keyboard.toml "'"$config_path"'"'

apply:
    {{ control }} config apply {{ glove80_config }}

diff:
    {{ control }} config diff {{ glove80_config }}

pull:
    {{ control }} config pull {{ glove80_config }}

show:
    {{ control }} config show

go60-diff device:
    {{ control }} config diff --device {{ device }} {{ go60_config }}

go60-apply device:
    {{ control }} config apply --device {{ device }} {{ go60_config }}

go60-pull device:
    {{ control }} config pull --device {{ device }} {{ go60_config }}

ctl *args:
    {{ control }} {{ args }}

# Behavior usage report for every managed runtime config (docs/layout-tools.md).
usage:
    nix develop ./{{ firmware_repo }} --command cargo run --quiet --bin moergo-layout -- \
        usage {{ glove80_config }} {{ go60_config }} config/tailorkey-v52-bilateral.toml

# Offline layout tooling: usage / os / alpha / preset (docs/layout-tools.md).
layout *args:
    nix develop ./{{ firmware_repo }} --command cargo run --quiet --bin moergo-layout -- {{ args }}

firmware:
    config_dirty=false; \
        if test -n "$(git status --porcelain --untracked-files=normal)"; then \
            config_dirty=true; \
        fi; \
        config_path="$(pwd)/{{ glove80_firmware_config }}"; \
        KEYBOARD_TOML_PATH="$config_path" \
        MOERGO_CONFIG_GIT_COMMIT="$(git rev-parse HEAD)" \
        MOERGO_CONFIG_GIT_DIRTY="$config_dirty" \
            bash -c 'cd {{ firmware_repo }} && nix develop path:. --command just dist'

go60-firmware: go60-profile-check
    config_dirty=false; \
        if test -n "$(git status --porcelain --untracked-files=normal)"; then \
            config_dirty=true; \
        fi; \
        config_path="$(pwd)/{{ go60_firmware_config }}"; \
        KEYBOARD_TOML_PATH="$config_path" \
        MOERGO_CONFIG_GIT_COMMIT="$(git rev-parse HEAD)" \
        MOERGO_CONFIG_GIT_DIRTY="$config_dirty" \
            bash -c 'cd {{ firmware_repo }} && nix develop path:. --command just go60-firmware'
    nix develop path:./{{ firmware_repo }} --command ./bin/go60-firmware-reference-check

firmware-all: firmware go60-firmware

attention-check:
    nix develop ./{{ firmware_repo }} --command cargo test
    nix develop ./{{ firmware_repo }} --command cargo fmt --all -- --check

attention-run *args:
    nix develop ./{{ firmware_repo }} --command cargo run --bin rmk-attentiond -- {{ args }}
