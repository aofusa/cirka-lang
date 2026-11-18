/* The C library of counter.h. */
#include <stdlib.h>

#include "counter.h"

struct counter {
  long n;
};

struct counter *counter_new(long start) {
  struct counter *c = malloc(sizeof *c);
  if (c != NULL)
    c->n = start;
  return c;
}

long counter_next(struct counter *c) { return c->n++; }

void counter_free(struct counter *c) { free(c); }
