BUILD := build
CFLAGS := -O2 -Wall -Wextra $(shell pkg-config --cflags wayland-client libavformat libavutil libswscale)
LDLIBS := $(shell pkg-config --libs wayland-client libavformat libavcodec libavutil libswscale)

all: $(BUILD)/window-capture

$(BUILD):
	mkdir -p $(BUILD)

$(BUILD)/toplevel.h: protocols/hyprland-toplevel-export-v1.xml | $(BUILD)
	wayland-scanner client-header $< $@

$(BUILD)/toplevel.c: protocols/hyprland-toplevel-export-v1.xml | $(BUILD)
	wayland-scanner private-code $< $@

$(BUILD)/foreign.c: protocols/wlr-foreign-toplevel-management-unstable-v1.xml | $(BUILD)
	wayland-scanner private-code $< $@

$(BUILD)/window-capture: window_capture.c $(BUILD)/toplevel.h $(BUILD)/toplevel.c $(BUILD)/foreign.c
	$(CC) $(CFLAGS) -I$(BUILD) window_capture.c $(BUILD)/toplevel.c $(BUILD)/foreign.c $(LDLIBS) -o $@
