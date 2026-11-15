// C side of extern_records.cirka: structs and unions of each System V
// class by value, enums, data and function addresses, variadic calls.
#include <stdint.h>

typedef struct { int x; int y; } pt;                       // one INTEGER eightbyte
typedef struct { float x; float y; double z; } vec3;        // two SSE eightbytes
typedef struct { double w; long n; } mixed;                 // SSE + INTEGER
typedef struct { double a, b, c; long n; } big;             // memory
typedef union { int i; float f; } num;
typedef enum { RED = 1, GREEN = 2, BLUE = 4 } color;
typedef struct { char tag; pt p; float w[2]; } nested;
typedef struct __attribute__((packed)) { char c; int i; short s; } packed3;

pt pt_add(pt a, pt b) { pt r = { a.x + b.x, a.y + b.y }; return r; }
vec3 vec3_scale(vec3 v, float k) { vec3 r = { v.x * k, v.y * k, v.z * k }; return r; }
mixed mixed_swap(mixed m) { mixed r = { (double)m.n, (long)m.w }; return r; }
double big_sum(big b) { return b.a + b.b + b.c + (double)b.n; }
big big_make(double x) { big r = { x, x * 2, x * 3, 7 }; return r; }
int num_bits(num n) { return n.i; }
num num_of_float(float f) { num n; n.f = f; return n; }
color color_next(color c) { return c == BLUE ? RED : (color)(c * 2); }
float nested_sum(nested n) { return (float)n.tag + (float)n.p.x + (float)n.p.y + n.w[0] + n.w[1]; }
long packed_sum(packed3 p) { return p.c + p.i + p.s; }
packed3 packed_make(int i) { packed3 p = { 1, i, 3 }; return p; }
void pt_fill(pt *out, int v) { out->x = v; out->y = -v; }
int pt_norm1(const pt *p) { return (p->x < 0 ? -p->x : p->x) + (p->y < 0 ? -p->y : p->y); }

static int twice(int x) { return 2 * x; }
int (*get_twice(void))(int) { return twice; }
int answer_value = 42;
