"""Debug playground (Python): open this folder as a project, set breakpoints, press Debug."""

from helpers import average, describe


class Counter:
    def __init__(self, name):
        self.name = name
        self.count = 0

    def add(self, amount):
        self.count += amount  # try a breakpoint here, then step into it from main()
        return self.count


def fibonacci(n):
    a, b = 0, 1
    for _ in range(n):
        a, b = b, a + b  # breakpoint in a loop: Continue (F1) hits it every round
    return a


def collatz(start):
    steps = 0
    value = start
    while value != 1:
        value = value // 2 if value % 2 == 0 else 3 * value + 1
        steps += 1
    return steps


def main():
    counter = Counter("demo")
    scores = {"fib": fibonacci(10), "collatz": collatz(27)}  # step into (F3) here
    for name, value in scores.items():
        total = counter.add(value)
        print(f"{name:8} {value:5}  running total {total}  {describe(name, value)}")
    print("average", average(list(scores.values())))
    print("done")  # evaluate in the console: scores, counter.count, len(scores) ...


if __name__ == "__main__":
    main()
