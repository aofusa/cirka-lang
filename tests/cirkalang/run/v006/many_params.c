/* Functions with more parameters than a closure takes (BUG-0027). */
static long last;

void weigh10(int a, int b, int c, int d, int e, int f, int g, int h, int i, int j) {
  last = a + 2L * b + 3L * c + 4L * d + 5L * e + 6L * f + 7L * g + 8L * h + 9L * i + 10L * j;
}

long last_weight(void) { return last; }

long weigh9(long a, long b, long c, long d, long e, long f, long g, long h, long i) {
  return a + 2 * b + 3 * c + 4 * d + 5 * e + 6 * f + 7 * g + 8 * h + 9 * i;
}
