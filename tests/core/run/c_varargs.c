/* The C side of c_varargs.core (FEAT-0047): variadic functions that read
 * their arguments with va_arg, structures included, so that the caller's
 * placement of variadic arguments (registers, then the stack, in order) is
 * checked against the C compiler's va_arg. */
#include <errno.h>
#include <stdarg.h>
#include <stdint.h>

typedef struct { int64_t a, b; } pair_l;
typedef struct { double x, y; } vec2;
typedef struct { int64_t a, b, c; } big;

/* The n integers as the digits of a number. */
int64_t cv_sum(int n, ...) {
    va_list ap;
    va_start(ap, n);
    int64_t s = 0;
    for (int i = 0; i < n; i++)
        s = s * 10 + va_arg(ap, int64_t);
    va_end(ap);
    return s;
}

/* The same with doubles. */
int64_t cv_fsum(int n, ...) {
    va_list ap;
    va_start(ap, n);
    int64_t s = 0;
    for (int i = 0; i < n; i++)
        s = s * 10 + (int64_t)va_arg(ap, double);
    va_end(ap);
    return s;
}

/* Arguments described by a template: 'i' an int64_t, 'd' a double, 'p' a
 * pair_l, 'v' a vec2, 'b' a big. */
int64_t cv_mix(const char *t, ...) {
    va_list ap;
    va_start(ap, t);
    int64_t s = 0;
    for (; *t; t++) {
        switch (*t) {
        case 'i':
            s = s * 10 + va_arg(ap, int64_t);
            break;
        case 'd':
            s = s * 10 + (int64_t)va_arg(ap, double);
            break;
        case 'p': {
            pair_l p = va_arg(ap, pair_l);
            s = s * 100 + p.a * 10 + p.b;
            break;
        }
        case 'v': {
            vec2 v = va_arg(ap, vec2);
            s = s * 100 + (int64_t)v.x * 10 + (int64_t)v.y;
            break;
        }
        case 'b': {
            big b = va_arg(ap, big);
            s = s * 1000 + b.a * 100 + b.b * 10 + b.c;
            break;
        }
        }
    }
    va_end(ap);
    return s;
}

/* Fails with errno e. */
int cv_fail(int e) {
    errno = e;
    return -1;
}
