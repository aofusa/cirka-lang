/* The C side of c_structs.core (FEAT-0047): functions taking and returning
 * structs and unions by value, compiled by the C compiler so that the
 * classification of the backend and of the interpreter is checked against
 * the real ABI. */
#include <stdint.h>

typedef struct { int32_t a, b; } pair_i;
typedef struct { int64_t a, b; } pair_l;
typedef struct { double x, y; } vec2;
typedef struct { float x, y, z; } vec3;
typedef struct { float f; int32_t i; double d; } mixed;
typedef struct { int64_t a, b, c; } big;
typedef struct { uint8_t a, b, c; } three;
typedef union { float f[4]; int32_t i[4]; } clear;
typedef struct __attribute__((packed)) { uint8_t tag; uint32_t v; } packed;

int64_t cs_sum_pair_i(pair_i p) { return p.a + p.b; }

pair_l cs_make_pair_l(int64_t a, int64_t b) {
    pair_l r = {a, b};
    return r;
}

double cs_dot(vec2 a, vec2 b) { return a.x * b.x + a.y * b.y; }

vec3 cs_scale3(vec3 v, float k) {
    vec3 r = {v.x * k, v.y * k, v.z * k};
    return r;
}

mixed cs_mixed(mixed m) {
    m.f += 1;
    m.i *= 2;
    m.d /= 4;
    return m;
}

big cs_big_add(big a, big b) {
    big r = {a.a + b.a, a.b + b.b, a.c + b.c};
    return r;
}

int64_t cs_three(three t) { return t.a * 10000 + t.b * 100 + t.c; }

int64_t cs_clear_bits(clear c) { return c.i[0] ^ c.i[3]; }

uint32_t cs_packed_v(packed p) { return p.v + p.tag; }

/* Five integer registers are taken: the struct goes on the stack, the last
 * argument in the sixth register. */
int64_t cs_many(int64_t a, int64_t b, int64_t c, int64_t d, int64_t e, pair_l p, int64_t f) {
    return a + b + c + d + e + p.a * 1000 + p.b * 100 + f;
}

/* Seven SSE registers are taken: the struct goes on the stack. */
double cs_many_f(double a, double b, double c, double d, double e, double f, double g, vec2 v,
                 double h) {
    return a + b + c + d + e + f + g + v.x * 100 + v.y * 1000 + h * 10000;
}
