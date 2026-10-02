// Debug playground (Swift): open this folder as a project and press Debug on “swift: demo”.

struct Counter {
    var name: String
    var count = 0

    mutating func add(_ amount: Int) -> Int {
        count += amount  // try a breakpoint here, then step into it from the loop
        return count
    }
}

func fibonacci(_ n: Int) -> Int {
    var a = 0, b = 1
    for _ in 0..<n {
        let next = a + b  // breakpoint in a loop: Continue (F1) hits it every round
        a = b
        b = next
    }
    return a
}

func collatz(_ start: Int) -> Int {
    var steps = 0
    var value = start
    while value != 1 {
        value = value % 2 == 0 ? value / 2 : 3 * value + 1
        steps += 1
    }
    return steps
}

var counter = Counter(name: "demo")
let scores = ["fib": fibonacci(10), "collatz": collatz(27)]  // step into (F4) here
for name in scores.keys.sorted() {
    let total = counter.add(scores[name]!)
    print("\(name) \(scores[name]!)  running total \(total)")
}
print("done")  // look at `counter` and `scores` in Variables
