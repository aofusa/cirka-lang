// C side of extern_callbacks.cirka: C calling CirkaLang functions through
// function pointers, with scalars and structs.
typedef struct { int x; int y; } pt;
typedef struct { double a, b, c; long n; } big;

int apply_twice(int (*f)(int), int x) { return f(f(x)); }
pt map_pt(pt (*f)(pt), pt p) { return f(p); }
double sum_big(big (*f)(double), double x) { big b = f(x); return b.a + b.b + b.c + (double)b.n; }
void each(void (*f)(int), int n) { for (int i = 0; i < n; i++) f(i); }
