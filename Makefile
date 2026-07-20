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
	$(MAKE) -C $(GLOVE80_RMK) firmware
