/*
 * Copyright (c) 2020 The ZMK Contributors
 *
 * SPDX-License-Identifier: MIT
 */

#pragma once

#include <stddef.h>
#include <stdint.h>

struct zmk_led_hsb {
    uint16_t h;
    uint8_t s;
    uint8_t b;
};

#if IS_ENABLED(CONFIG_ZMK_HOST_LIGHTING)
struct zmk_rgb_underglow_host_pixel {
    uint8_t index;
    uint8_t r;
    uint8_t g;
    uint8_t b;
};

enum zmk_rgb_underglow_host_effect_type {
    ZMK_RGB_UNDERGLOW_HOST_EFFECT_STATIC = 0,
    ZMK_RGB_UNDERGLOW_HOST_EFFECT_BLINK = 1,
    ZMK_RGB_UNDERGLOW_HOST_EFFECT_BREATHE = 2,
};

struct zmk_rgb_underglow_host_effect {
    uint8_t index;
    uint8_t r;
    uint8_t g;
    uint8_t b;
    uint16_t period_ms;
    uint16_t phase_ms;
    uint8_t type;
    uint8_t duty_percent;
};

size_t zmk_rgb_underglow_host_pixel_count(void);
int zmk_rgb_underglow_host_replace(uint32_t timeout_ms);
int zmk_rgb_underglow_host_update(const struct zmk_rgb_underglow_host_pixel *updates,
                                  size_t update_count, uint32_t timeout_ms);
int zmk_rgb_underglow_host_update_effects(const struct zmk_rgb_underglow_host_effect *effects,
                                          size_t effect_count, uint32_t timeout_ms);
int zmk_rgb_underglow_host_clear(void);
#endif

int zmk_rgb_underglow_toggle(void);
int zmk_rgb_underglow_get_state(bool *state);
int zmk_rgb_underglow_on(void);
int zmk_rgb_underglow_off(void);
int zmk_rgb_underglow_cycle_effect(int direction);
int zmk_rgb_underglow_calc_effect(int direction);
int zmk_rgb_underglow_select_effect(int effect);
struct zmk_led_hsb zmk_rgb_underglow_calc_hue(int direction);
struct zmk_led_hsb zmk_rgb_underglow_calc_sat(int direction);
struct zmk_led_hsb zmk_rgb_underglow_calc_brt(int direction);
int zmk_rgb_underglow_change_hue(int direction);
int zmk_rgb_underglow_change_sat(int direction);
int zmk_rgb_underglow_change_brt(int direction);
int zmk_rgb_underglow_change_spd(int direction);
int zmk_rgb_underglow_set_hsb(struct zmk_led_hsb color);
int zmk_rgb_underglow_status(void);
