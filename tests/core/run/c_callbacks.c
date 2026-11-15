/* The C side of c_callbacks.core (FEAT-0047): C functions calling the
 * function pointers they are given, with arguments and results of every
 * class, and a callback that calls C that calls another callback. */
#include <stdint.h>

typedef struct { int64_t a, b, c; } big;
typedef struct { float x, y, z; } vec3;

/* f(1) + ... + f(n). */
int64_t cb_each(int64_t (*f)(int64_t), int64_t n) {
    int64_t s = 0;
    for (int64_t i = 1; i <= n; i++)
        s += f(i);
    return s;
}

/* A structure in memory both ways (`byval`, `sret`). */
int64_t cb_big(big (*f)(big, int64_t)) {
    big b = {1, 2, 3};
    big r = f(b, 10);
    return r.a * 10000 + r.b * 100 + r.c;
}

/* A structure in SSE registers both ways. */
double cb_vec3(vec3 (*f)(vec3, float)) {
    vec3 v = {1, 2, 3};
    vec3 r = f(v, 2.0f);
    return r.x + r.y * 10 + r.z * 100;
}

/* Arguments beyond the registers. */
typedef double (*many_fn)(int64_t, int64_t, int64_t, int64_t, int64_t, int64_t, int64_t, int64_t,
                          double, double, double, double, double, double, double, double, double,
                          double);
double cb_many(many_fn f) {
    return f(1, 2, 3, 4, 5, 6, 7, 8, 0.5, 1.5, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5, 8.5, 9.5);
}

/* Narrow integers: the callback sees them extended, C its result narrowed. */
int64_t cb_narrow(int8_t (*f)(uint8_t, int16_t, uint32_t)) { return f(250, -3, 4000000000u); }

/* f(g, 5), for an f that calls cb_each(g, 5). */
int64_t cb_nested(int64_t (*f)(int64_t (*)(int64_t), int64_t), int64_t (*g)(int64_t)) {
    return f(g, 5);
}
