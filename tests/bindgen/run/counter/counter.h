// bindgen: --extern
// A C library through `achc bindgen --extern` (FEAT-0130): the generated
// extern block declares the opaque type and the functions by their C names.
struct counter;

struct counter *counter_new(long start);
long counter_next(struct counter *c);
void counter_free(struct counter *c);
