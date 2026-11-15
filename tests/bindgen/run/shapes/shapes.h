// bindgen: --file=*/shapes.h --pure=lerp
// A small C library (shapes.c) that Main.hs uses through the module that
// `achc bindgen` writes from this header.
#include <stdint.h>

#define SHAPES_VERSION ((1u << 16) | 2u)
#define SHAPES_NAME "shapes"
#define SHAPES_EPSILON 0.25

typedef enum ShapeKind { SHAPE_NONE = -1, SHAPE_CIRCLE = 1, SHAPE_RECT = 2 } ShapeKind;

typedef struct Vec2 {
    double x, y;
} Vec2;

typedef struct Shape {
    ShapeKind kind;
    Vec2 center;
    double size[2];
    uint8_t visible : 1;
    uint8_t layer : 7;
    char label[6];
} Shape;

typedef union Bits {
    uint32_t word;
    uint8_t bytes[4];
} Bits;

typedef int32_t (*Visitor)(const Shape *shape, void *data);

double shape_area(const Shape *s);
void shape_move(Shape *s, const Vec2 *by);
int32_t shapes_visit(Shape *shapes, int32_t n, Visitor v, void *data);
Visitor shapes_layer_visitor(void);
double lerp(double a, double b, double t);
uint32_t bits_reverse(Bits *b);
