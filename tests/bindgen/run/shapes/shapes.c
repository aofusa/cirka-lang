#include "shapes.h"

double shape_area(const Shape *s) {
    switch (s->kind) {
    case SHAPE_CIRCLE:
        return 3.0 * s->size[0] * s->size[0];
    case SHAPE_RECT:
        return s->size[0] * s->size[1];
    default:
        return 0.0;
    }
}

void shape_move(Shape *s, const Vec2 *by) {
    s->center.x += by->x;
    s->center.y += by->y;
}

int32_t shapes_visit(Shape *shapes, int32_t n, Visitor v, void *data) {
    int32_t total = 0;
    for (int32_t i = 0; i < n; i++) {
        if (shapes[i].visible) {
            total += v(&shapes[i], data);
        }
    }
    return total;
}

static int32_t layer_of(const Shape *s, void *data) {
    (void)data;
    return s->layer;
}

Visitor shapes_layer_visitor(void) { return layer_of; }

double lerp(double a, double b, double t) { return a + (b - a) * t; }

uint32_t bits_reverse(Bits *b) {
    uint8_t t = b->bytes[0];
    b->bytes[0] = b->bytes[3];
    b->bytes[3] = t;
    t = b->bytes[1];
    b->bytes[1] = b->bytes[2];
    b->bytes[2] = t;
    return b->word;
}
