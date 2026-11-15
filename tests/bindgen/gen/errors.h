// A header with errors: bindgen reports clang's diagnostics and writes
// nothing.
#include <stdint.h>

struct S { int x }
unknown_t f(void);
int g(int);
