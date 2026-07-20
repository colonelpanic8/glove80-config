GLOVE80_RMK := dependencies/glove80-rmk
CONTROL := ./bin/glove80-control
CONFIG := config/glove80.toml

.PHONY: init check apply show devices firmware

init:
	git submodule update --init --recursive

check:
	$(CONTROL) config validate $(CONFIG)

apply:
	$(CONTROL) config apply $(CONFIG)

show:
	$(CONTROL) config show

devices:
	$(CONTROL) devices

firmware:
	@config_dirty=false; \
	if test -n "$$(git status --porcelain --untracked-files=normal)"; then \
		config_dirty=true; \
	fi; \
	GLOVE80_CONFIG_GIT_COMMIT="$$(git rev-parse HEAD)" \
	GLOVE80_CONFIG_GIT_DIRTY="$$config_dirty" \
		$(MAKE) -C $(GLOVE80_RMK) firmware
