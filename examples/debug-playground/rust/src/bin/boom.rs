//! A panic: the debugger stops on the line that panicked (pick “cargo: boom”).

fn check(n: u32) -> u32 {
    if n > 3 {
        panic!("{n} is too big"); // the debugger stops here, with `n` in Variables
    }
    n * 2
}

fn main() {
    let mut total = 0;
    for n in 0..6 {
        total += check(n);
    }
    println!("{total}");
}
