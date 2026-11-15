// bindgen: --allowlist=sqrt|pow|floor|frexp|ldexp --pure=sqrt|pow|floor
// The C library's math.h through `cirka bindgen` (functions only: the
// other declarations of extern blocks come with FEAT-0250).
#include <math.h>
