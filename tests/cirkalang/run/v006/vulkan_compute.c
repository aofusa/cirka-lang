/* The SPIR-V code of the shaders vulkan_compute.<name>.comp, which the
 * test harness compiles with `glslc -mfmt=c` to <name>.spv.inc (FEAT-0251). */
#define _GNU_SOURCE
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#define SHADER_SIZE(name) \
  size_t name##_spv_size(void) { return sizeof name##_spv; }

const uint32_t vadd_spv[] =
#include "vadd.spv.inc"
;
SHADER_SIZE(vadd)

const uint32_t saxpy_spv[] =
#include "saxpy.spv.inc"
;
SHADER_SIZE(saxpy)

const uint32_t matmul_spv[] =
#include "matmul.spv.inc"
;
SHADER_SIZE(matmul)

const uint32_t reduce_spv[] =
#include "reduce.spv.inc"
;
SHADER_SIZE(reduce)

static char path[64];

/* Writes the SPIR-V code of vadd to a new temporary file, for
 * `Vk.spirvFile`, and returns its name ("" when it cannot). */
const char *vadd_spv_file(void) {
  strcpy(path, "/tmp/cirka-vadd-XXXXXX.spv");
  int fd = mkstemps(path, 4);
  if (fd < 0) {
    return "";
  }
  ssize_t n = write(fd, vadd_spv, sizeof vadd_spv);
  close(fd);
  return n == (ssize_t)sizeof vadd_spv ? path : "";
}

/* Removes the file of vadd_spv_file. */
void remove_vadd_spv_file(void) {
  if (path[0] != '\0') {
    unlink(path);
  }
}
