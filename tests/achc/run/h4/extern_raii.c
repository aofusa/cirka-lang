/* C side of extern_raii.chs (FEAT-0130): resources that count how many are
   alive and record the order in which they are closed. The interpreter runs
   the program several times in one process, so the program starts by
   clearing the record. */
#include <stdlib.h>

struct res {
  long id;
};

static long live;
static long order[64];
static long nclosed;

struct res *res_open(long id) {
  if (id < 0)
    return NULL;
  struct res *r = malloc(sizeof *r);
  if (r == NULL)
    return NULL;
  r->id = id;
  live++;
  return r;
}

long res_id(const struct res *r) { return r->id; }

long res_close(struct res *r) {
  long id = r->id;
  free(r);
  live--;
  if (nclosed < 64)
    order[nclosed++] = id;
  return id;
}

long res_live(void) { return live; }

/* Takes over a resource that may be NULL. */
long res_close_maybe(struct res *r) { return r == NULL ? -1 : res_close(r); }

/* Borrowed results: the pointer itself, or NULL unless it has the id. */
struct res *res_self(struct res *r) { return r; }

struct res *res_if(long id, struct res *r) { return r->id == id ? r : NULL; }

long res_closed(long k) { return k < nclosed ? order[k] : -1; }

void res_reset(void) { nclosed = 0; }
