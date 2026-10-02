"""An uncaught exception: the debugger stops on the line that raised it, with all variables intact.

Run it with “Current file” while this file is open.
"""


def parse_age(text):
    return int(text)  # raises ValueError for anything that is not a number


def main():
    ages = ["31", "27", "forty", "19"]
    parsed = []
    for text in ages:
        parsed.append(parse_age(text))  # stops here on "forty"; look at `text` and `parsed`
    print(parsed)


if __name__ == "__main__":
    main()
