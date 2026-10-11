#include <chafa.h>
#include <stdint.h>

/* Only the graphics layer reaches this canvas. Blink's text stays in Rust's
 * grapheme layer and is painted over these symbols by the existing painter. */
ChafaCanvas *carbonyl_chafa_new(int width, int height) {
    ChafaSymbolMap *symbols = chafa_symbol_map_new();
    ChafaSymbolMap *fill = chafa_symbol_map_new();
    ChafaCanvasConfig *config = chafa_canvas_config_new();
    chafa_symbol_map_add_by_tags(symbols, CHAFA_SYMBOL_TAG_HHALF |
        CHAFA_SYMBOL_TAG_VHALF | CHAFA_SYMBOL_TAG_QUAD | CHAFA_SYMBOL_TAG_SEXTANT);
    chafa_canvas_config_set_geometry(config, width, height);
    chafa_canvas_config_set_cell_geometry(config, 8, 16);
    chafa_canvas_config_set_symbol_map(config, symbols);
    chafa_canvas_config_set_fill_symbol_map(config, fill);
    chafa_canvas_config_set_canvas_mode(config, CHAFA_CANVAS_MODE_TRUECOLOR);
    chafa_canvas_config_set_pixel_mode(config, CHAFA_PIXEL_MODE_SYMBOLS);
    chafa_canvas_config_set_preprocessing_enabled(config, FALSE);
    chafa_canvas_config_set_dither_mode(config, CHAFA_DITHER_MODE_NONE);
    chafa_canvas_config_set_work_factor(config, 0.75f);
    /* Zero makes even opaque pixels transparent. Keep Chafa's normal cutoff. */
    chafa_canvas_config_set_transparency_threshold(config, 0.5f);
    ChafaCanvas *canvas = chafa_canvas_new(config);
    chafa_canvas_config_unref(config);
    chafa_symbol_map_unref(symbols);
    chafa_symbol_map_unref(fill);
    return canvas;
}

void carbonyl_chafa_free(ChafaCanvas *canvas) {
    chafa_canvas_unref(canvas);
}

void carbonyl_chafa_draw(ChafaCanvas *canvas, const uint8_t *pixels,
                        int width, int height, int stride) {
    chafa_canvas_draw_all_pixels(canvas, CHAFA_PIXEL_BGRA8_PREMULTIPLIED,
                                pixels, width, height, stride);
}

uint32_t carbonyl_chafa_get(ChafaCanvas *canvas, int x, int y,
                           int *foreground, int *background) {
    chafa_canvas_get_colors_at(canvas, x, y, foreground, background);
    return chafa_canvas_get_char_at(canvas, x, y);
}
