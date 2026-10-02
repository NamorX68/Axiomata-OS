// Debug playground (C++): open this folder as a project and press Debug on “cmake: demo”.
#include <iostream>
#include <map>
#include <string>
#include <vector>

struct Counter {
    std::string name;
    long count = 0;

    long add(long amount) {
        count += amount;  // try a breakpoint here, then step into it from main()
        return count;
    }
};

long fibonacci(int n) {
    long a = 0, b = 1;
    for (int i = 0; i < n; i++) {
        long next = a + b;  // breakpoint in a loop: Continue (F5) hits it every round
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
        ++steps;
    }
    return steps;
}

int main() {
    Counter counter{"demo"};
    std::map<std::string, long> scores{{"fib", fibonacci(10)}, {"collatz", collatz(27)}};  // step into (F11) here
    std::vector<std::string> names;
    for (const auto& entry : scores) names.push_back(entry.first);
    for (const auto& name : names) {
        long total = counter.add(scores[name]);
        std::cout << name << "  " << scores[name] << "  running total " << total << "\n";
    }
    std::cout << "done\n";  // look at `counter`, `scores` and `names` in Variables
    return 0;
}
