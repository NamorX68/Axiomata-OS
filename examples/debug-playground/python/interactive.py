"""Reads from the keyboard — tick “Run in a terminal” in the Debug view, then start it.

Without the tick there is no keyboard for input() to read from.
"""


def main():
    name = input("Your name: ")  # breakpoint here, step over, then type in the Debug pane
    greeting = f"Hello, {name}!"
    print(greeting)  # breakpoint here: `name` and `greeting` are in Variables


if __name__ == "__main__":
    main()
