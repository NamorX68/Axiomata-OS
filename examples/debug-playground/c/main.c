/* Debug playground (C): open this folder as a project, open this file, press Debug
   with “Current file” — it is compiled with -g -O0 and started under the debugger. */
#include <stdio.h>

typedef struct {
    const char *name;
    long count;
} Counter;

long add(Counter *counter, long amount) {
    counter->count += amount; /* try a breakpoint here, then step into it from main() */
    return counter->count;
}

long fibonacci(int n) {
    long a = 0, b = 1;
    for (int i = 0; i < n; i++) {
        long next = a + b; /* breakpoint in a loop: Continue (F1) hits it every round */
        a = b;
        b = next;
    }
    return a;
}

int collatz(long start) {
    int steps = 0;
    long value = start;
    while (value != 1) {
        value = (value % 2 == 0) ? value / 2 : 3 * value + 1;
        steps++;
    }
    return steps;
}

int main(void) {
    Counter counter = {"demo", 0};
    long scores[2] = {fibonacci(10), collatz(27)}; /* step into (F3) here */
    const char *names[2] = {"fib", "collatz"};
    for (int i = 0; i < 2; i++) {
        long total = add(&counter, scores[i]);
        printf("%-8s %5ld  running total %ld\n", names[i], scores[i], total);
    }
    printf("done\n");
    return 0;
}
