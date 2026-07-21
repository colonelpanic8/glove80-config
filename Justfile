set shell := ["bash", "-euo", "pipefail", "-c"]

glove80_rmk := "dependencies/glove80-rmk"
control := "./bin/glove80-control"
config := "config/glove80.toml"

init:
    git submodule update --init --recursive

check:
    {{ control }} config validate {{ config }}

apply:
    {{ control }} config apply {{ config }}

show:
    {{ control }} config show

firmware:
    config_dirty=false; \
        if test -n "$(git status --porcelain --untracked-files=normal)"; then \
            config_dirty=true; \
        fi; \
        GLOVE80_CONFIG_GIT_COMMIT="$(git rev-parse HEAD)" \
        GLOVE80_CONFIG_GIT_DIRTY="$config_dirty" \
            bash -c 'cd {{ glove80_rmk }} && nix develop --command ./scripts/build-release.sh'
