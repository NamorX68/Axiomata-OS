//! A second binary: the Debug view offers “cargo: tiny” next to “cargo: debug-demo”.

fn main() {
    let numbers = [3, 1, 4, 1, 5, 9, 2, 6];
    let mut sum = 0;
    for n in numbers {
        sum += n; // breakpoint here
    }
    println!("sum = {sum}");
}
