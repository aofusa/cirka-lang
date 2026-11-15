// bindgen: --file=*/features.h --pure=scale|lerp
// Every kind of declaration that bindgen converts (08-ffi.md §6), with the
// cases it skips.
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

/* Macro constants: integers (also through function-like macros), floats
   and strings. Macros that are not constants give nothing. */
#define MAKE_VERSION(major, minor) (((uint32_t)(major) << 22U) | ((uint32_t)(minor) << 12U))
#define VERSION MAKE_VERSION(1, 3)
#define GREETING "hello\n"
#define PAREN_STRING ("paren")
#define RATIO 0.5F
#define LIMIT 1e100
#define ALL_BITS (~0ULL)
#define NEGATIVE (-5)
#define EMPTY
#define UNUSED __attribute__((unused))
#define TYPE_NAME int

/* Opaque handles. */
typedef struct Instance_T *Instance;
struct Window;

/* Enums: the integer type is clang's. */
typedef enum Color { RED = 1, GREEN = 2, BLUE = 4 } Color;
typedef enum Sign { MINUS = -1, ZERO, PLUS } Sign;
enum { ANONYMOUS_LIMIT = 3 };
typedef enum Big { BIG_FIRST = 0, BIG_MAX = 0x7FFFFFFF } Big;

/* Structs: nested, arrays, bit-fields, anonymous members, packed. */
typedef struct Point { int32_t x, y; } Point;
typedef struct Rect {
    Point min, max;
    float matrix[2][3];
    char name[8];
    uint32_t tag : 24;
    uint32_t flags : 8;
    uint64_t id;
} Rect;
typedef struct {
    int kind;
    union { int i; float f; };
    struct { char c; } named;
} Value;
struct __attribute__((packed)) Packed { char a; int b; };
union Number { int32_t i; float f; uint8_t bytes[4]; };
typedef struct List { struct List *next; bool last; } List;

/* Function pointers. */
typedef void (*Callback)(int code, const char *message);
typedef int Transform(int);
typedef struct Handler {
    Callback on_event;
    void *user_data;
    Point (*locate)(void);
} Handler;

/* Variables: constants or not. */
static const uint64_t FLAG_MASK = 4ULL;
static const double SCALE = 2.5;
extern int global_counter;

/* Functions. */
int32_t area(const Rect *r);
double scale(double x, float by);
double lerp(double a, double b, double t);
void register_handler(struct Window *w, Handler *h, Callback cb);
int apply(Transform *f, int values[4], size_t n);
bool check(Instance i);
int log_message(const char *format, ...);
Point origin(void);
void fill(union Number n);
long double precise(long double x);
static inline int twice(int x) { return 2 * x; }
const char *version_string(void);
int __reserved_function(void);
int type(int data, int module);
