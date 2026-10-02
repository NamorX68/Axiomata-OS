//! Debug playground (Rust): open this folder as a project, set breakpoints, press Debug.

use std::collections::HashMap;

#[derive(Debug)]
struct Counter {
    name: String,
    count: u64,
}

impl Counter {
    fn add(&mut self, amount: u64) -> u64 {
        self.count += amount; // try a breakpoint here, then step into it from main()
        self.count
    }
}

fn fibonacci(n: u32) -> u64 {
    let (mut a, mut b) = (0u64, 1u64);
    for _ in 0..n {
        let next = a + b; // breakpoint in a loop: Continue (F1) hits it every round
        a = b;
        b = next;
    }
    a
}

fn collatz(start: u64) -> u32 {
    let mut steps = 0;
    let mut value = start;
    while value != 1 {
        value = if value % 2 == 0 { value / 2 } else { 3 * value + 1 };
        steps += 1;
    }
    steps
}

fn main() {
    let mut counter = Counter { name: "demo".to_string(), count: 0 };
    let mut scores: HashMap<&str, u64> = HashMap::new();
    scores.insert("fib", fibonacci(10)); // step into (F3) here
    scores.insert("collatz", collatz(27) as u64);
    let mut names: Vec<_> = scores.keys().copied().collect();
    names.sort();
    for name in names {
        let value = scores[name];
        let total = counter.add(value);
        println!("{name:8} {value:5}  running total {total}");
    }
    println!("done: {} counted {}", counter.name, counter.count); // look at `counter`, `scores` and `names` in Variables
}
