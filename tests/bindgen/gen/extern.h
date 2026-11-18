// bindgen: --file=*/extern.h --extern --link=m --link=pthread --pure=twice
// `achc bindgen --extern` (FEAT-0130): the functions and the opaque types
// are in an `extern "C"` block, which links the first library; a function
// is named as in C unless Haskell cannot name it so (`c_` imports).
#include <stddef.h>

struct handle;
struct point {
  int x;
  int y;
};
enum color { RED, GREEN = 5 };
typedef int (*callback)(int);

struct handle *handle_open(const char *name);
int handle_close(struct handle *h);
int twice(int x);
int lines(const char *text);
void free(void *p);
int apply(callback f, int x);
int Upper(int x);
int peek(struct handle *h);
int point_x(struct point *p);
int data(int x);
int sum(int n, ...);
struct point mid(struct point a, struct point b);
