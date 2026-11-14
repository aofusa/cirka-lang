/* cirka_rt.h - public C ABI of libcirka_rt.a (docs/design/05-runtime.md).
 * Keep in sync with crates/cirka-runtime/src (checked by unit tests). */
#ifndef CIRKA_RT_H
#define CIRKA_RT_H
#include <stddef.h>
#include <stdint.h>

/* Object header: 8 bytes, followed by 8-byte fields (pointers first). */
typedef struct cirka_header {
    int32_t  rc;   /* >0 single-threaded count, <0 shared (atomic), 0 immortal */
    uint16_t tag;  /* constructor tag / closure arity / PAP argument count */
    uint8_t  nptr; /* number of RC-managed pointer fields */
    uint8_t  kind; /* CIRKA_KIND_* */
} cirka_header;

#define CIRKA_KIND_CON       0
#define CIRKA_KIND_FUN       1
#define CIRKA_KIND_PAP       2
#define CIRKA_KIND_THUNK     3
#define CIRKA_KIND_BLACKHOLE 4
#define CIRKA_KIND_IND       5
#define CIRKA_KIND_ARRAY     6
#define CIRKA_KIND_BYTES     7
#define CIRKA_KIND_BIGINT    8
#define CIRKA_KIND_FOREIGN   9
#define CIRKA_KIND_MUTVAR    10
#define CIRKA_KIND_MVAR      11

/* allocation and reference counting */
void *cirka_alloc(size_t size);
void  cirka_free_object(void *obj);
void  cirka_free_raw(void *obj);
void  cirka_dup(void *obj);
void  cirka_drop(void *obj);
void  cirka_dec_ref_slow(void *obj);
void  cirka_drop_fields(void *obj);
void *cirka_drop_reuse(void *obj);
void  cirka_mark_shared(void *obj);

/* evaluation */
void *cirka_force(void *thunk);
void *cirka_mk_pap(void *fun, size_t n, void *const *args);

/* runtime errors and exit */
_Noreturn void cirka_panic(const char *msg, size_t len);
_Noreturn void cirka_panic_bytes(void *bytes);
_Noreturn void cirka_exit(int64_t code);

/* I/O */
void    cirka_write_i64(int64_t n);
void    cirka_write_f64(double x);
void    cirka_write_char(int64_t c);
void    cirka_write_bytes(void *bytes);
void    cirka_ewrite_bytes(void *bytes);
void    cirka_flush(void);
int64_t cirka_read_byte(void);
int64_t cirka_argc(void);
void   *cirka_arg(int64_t i);
void   *cirka_bytes_from(const uint8_t *p, size_t len);
void   *cirka_bytes_new(int64_t len);
void   *cirka_array_new(int64_t len, void *init);
void   *cirka_array_unique(void *a);
void   *cirka_bytes_unique(void *b);
/* Cycle collector (--cycles=collect, 04-memory-model.md §5). */
void    cirka_cycles_enable(void);
void    cirka_cycle_candidate(void *o);
/* Region arenas (04-memory-model.md §6). A cell is addressed by an integer. */
void   *cirka_region_new(void);
int64_t cirka_region_free(void *r);
int64_t cirka_region_cell(void *r, void *v);
void   *cirka_region_get(int64_t cell);
int64_t cirka_region_set(int64_t cell, void *v);
void   *cirka_f32_show_hs(double x);
double  cirka_f32_round(double x);
int64_t cirka_f64_bits(double x);
/* Raw memory access for Foreign.Storable (kind: see mem.rs). */
int64_t cirka_mem_read(int64_t addr, int64_t kind);
void cirka_mem_write(int64_t addr, int64_t kind, int64_t val);
double cirka_mem_read_f(int64_t addr, int64_t kind);
void cirka_mem_write_f(int64_t addr, int64_t kind, double val);
double  cirka_f64_from_bits(int64_t b);

/* Integer (BYTES objects in cirka-bigint's encoding; arguments borrowed) */
void   *cirka_big_from_int(int64_t x);
int64_t cirka_big_fits_int(void *a);
int64_t cirka_big_to_int(void *a);
double  cirka_big_to_double(void *a);
void   *cirka_big_from_double(double x);
void   *cirka_big_op(int64_t op, void *a, void *b);
int64_t cirka_big_cmp(void *a, void *b);
void   *cirka_big_show(void *a);
void   *cirka_big_read(void *digits, int64_t neg);

/* Mutable references (IORef / STRef; arguments borrowed). */
void   *cirka_mutvar_new(void *v);
void   *cirka_mutvar_read(void *r);
int64_t cirka_mutvar_write(void *r, void *v);
int64_t cirka_mutvar_cas(void *r, void *old_v, void *new_v);

/* Concurrency (05-runtime.md §9; pointer arguments borrowed). */
void   *cirka_thread_spawn(void *io);
void   *cirka_thread_join(void *handle);
int64_t cirka_thread_id(void);
int64_t cirka_thread_yield(void);
int64_t cirka_cpu_count(void);
void   *cirka_mvar_new(void *v);
void   *cirka_mvar_new_empty(void);
void   *cirka_mvar_take(void *m);
void   *cirka_mvar_read(void *m);
int64_t cirka_mvar_put(void *m, void *v);
void   *cirka_mvar_try_take(void *m, void *dflt);
int64_t cirka_mvar_try_put(void *m, void *v);
void   *cirka_chan_new(void);
int64_t cirka_chan_send(void *c, void *v);
void   *cirka_chan_recv(void *c);
void   *cirka_chan_try_recv(void *c, void *dflt);
int64_t cirka_chan_len(void *c);

/* Event loops and sockets (05-runtime.md §10; pointer arguments borrowed;
 * failures are -errno). A loop is used by the thread that created it;
 * other threads post to it through a remote. */
void   *cirka_loop_new(void);
void   *cirka_loop_current(void);
int64_t cirka_loop_watch(void *loop, int64_t fd, int64_t events, void *io);
int64_t cirka_loop_unwatch(void *loop, int64_t fd);
int64_t cirka_loop_events(void *loop);
int64_t cirka_loop_timer(void *loop, int64_t ms, void *io);
int64_t cirka_loop_cancel(void *loop, int64_t timer);
int64_t cirka_loop_run(void *loop);
int64_t cirka_loop_stop(void *loop);
void   *cirka_loop_remote(void *loop);
int64_t cirka_remote_post(void *remote, void *io);
int64_t cirka_loop_blocking(void *loop, void *work, void *done);
int64_t cirka_net_listen(void *host, int64_t port, int64_t backlog);
int64_t cirka_net_accept(int64_t fd);
int64_t cirka_net_connect(void *host, int64_t port);
int64_t cirka_net_connect_result(int64_t fd);
int64_t cirka_net_local_port(int64_t fd);
int64_t cirka_net_set_nodelay(int64_t fd, int64_t on);
int64_t cirka_net_shutdown(int64_t fd, int64_t how);
void   *cirka_net_resolve(void *host);
int64_t cirka_net_send(int64_t fd, void *data, int64_t off);
int64_t cirka_fd_write_from(int64_t fd, void *data, int64_t off);
int64_t cirka_fd_set_nonblocking(int64_t fd, int64_t on);
int64_t cirka_fd_limit_raise(void);

/* provided by generated code */
int64_t cirka_main(void);
void   *cirka_call_1(void *f, void *arg);
extern void *cirka_caf_roots[];
extern int64_t cirka_caf_count;

#endif
