"""Step into these from main.py (F3) to see the debugger move between files."""


def describe(name, value):
    label = name.upper()
    text = f"{label}={value}"  # breakpoint here: the call stack shows main -> describe
    return text


def average(values):
    total = sum(values)
    return total / len(values)  # try average([]) in the console: ZeroDivisionError
