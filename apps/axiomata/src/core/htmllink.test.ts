import { beforeEach, describe, expect, it, vi } from "vitest";

import { PAGE_MESSAGE_SOURCE, resolveRelativeLink, withNavIntercept } from "./htmllink";

describe("withNavIntercept", () => {
  it("inserts the script before </body>", () => {
    const out = withNavIntercept("<html><body><p>hi</p></body></html>");
    expect(out).toMatch(/<script>.*<\/script><\/body>/);
    expect(out.indexOf("<script>")).toBeLessThan(out.indexOf("</body>"));
  });

  it("appends the script when there is no </body>", () => {
    const out = withNavIntercept("<p>fragment only</p>");
    expect(out.startsWith("<p>fragment only</p>")).toBe(true);
    expect(out).toContain("<script>");
  });

  it("is case-insensitive about matching the closing tag", () => {
    const out = withNavIntercept("<BODY><p>x</p></BODY>");
    expect(out).toMatch(/<script>.*<\/script><\/body>/);
  });
});

describe("the injected click handler (run for real in jsdom)", () => {
  // Pulls the JS out of the <script> tag withNavIntercept injects and
  // actually executes it against a live `document`, the same way the
  // srcdoc'd iframe would — this is the behavior that matters (case 2 in
  // the module doc comment), not just the string it's embedded in.
  function install(): void {
    const scriptTag = withNavIntercept("<body></body>").match(/<script>([\s\S]*)<\/script>/)?.[1];
    if (!scriptTag) throw new Error("withNavIntercept produced no <script> tag");
    new Function(scriptTag)();
  }

  beforeEach(() => {
    document.body.innerHTML = "";
    install();
  });

  it("scrolls to the target element for a same-page anchor, without asking the parent", () => {
    document.body.innerHTML = '<a id="link" href="#etappe-a">go</a><section id="etappe-a"></section>';
    const target = document.getElementById("etappe-a")!;
    const scrollIntoView = vi.fn();
    target.scrollIntoView = scrollIntoView;
    const post = vi.fn();
    window.parent.postMessage = post;

    document.getElementById("link")!.click();

    expect(scrollIntoView).toHaveBeenCalledOnce();
    expect(post).not.toHaveBeenCalled();
  });

  it("does nothing (but does not throw) for a #anchor with no matching element", () => {
    document.body.innerHTML = '<a id="link" href="#missing">go</a>';
    expect(() => document.getElementById("link")!.click()).not.toThrow();
  });

  it("posts same-folder links to the parent instead of navigating, tagged with the shared message source", () => {
    document.body.innerHTML = '<a id="link" href="0003-next.html">next</a>';
    const post = vi.fn();
    window.parent.postMessage = post;

    document.getElementById("link")!.click();

    // The literal source string is exercised here (what the injected script
    // actually posts) and matched against the exported constant `HtmlPreview`
    // filters incoming messages by — a drift between the two would silently
    // break link navigation without either side raising an error.
    expect(post).toHaveBeenCalledWith({ source: PAGE_MESSAGE_SOURCE, href: "0003-next.html" }, "*");
  });

  it("leaves scheme'd links (http:, mailto:, javascript:) alone", () => {
    document.body.innerHTML = '<a id="link" href="https://example.com">ext</a>';
    const post = vi.fn();
    window.parent.postMessage = post;

    document.getElementById("link")!.click();

    expect(post).not.toHaveBeenCalled();
  });
});

describe("resolveRelativeLink", () => {
  it("refuses a link that still climbs out after decoding (..%2f), instead of handing on ../", () => {
    expect(resolveRelativeLink("Learning/Rust/lessons/0002-x.html", "..%2f..%2f..%2fetc%2fpasswd")).toBeNull();
    expect(resolveRelativeLink("a/b.html", "%2e%2e%2fsecret.md")).toBeNull();
    // A malformed escape names no file either.
    expect(resolveRelativeLink("a/b.html", "%E0%A4%A")).toBeNull();
    // An encoded name that stays inside is fine.
    expect(resolveRelativeLink("a/b.html", "c%2fd.html")).toBe("a/c/d.html");
  });

  it("resolves a same-folder link against the current file's folder", () => {
    expect(resolveRelativeLink("Learning/Rust/lessons/0002-variablen.html", "0003-funktionen.html")).toBe(
      "Learning/Rust/lessons/0003-funktionen.html",
    );
  });

  it("resolves ../ the same way a real browser would", () => {
    expect(resolveRelativeLink("Learning/Rust/lessons/0002-variablen.html", "../roadmap.html")).toBe(
      "Learning/Rust/roadmap.html",
    );
  });

  it("resolves a link from a file with no folder of its own", () => {
    expect(resolveRelativeLink("index.html", "about.html")).toBe("about.html");
  });

  it("decodes percent-escaped characters in the target", () => {
    expect(resolveRelativeLink("Learning/lessons/a.html", "Lektion%20zwei.html")).toBe(
      "Learning/lessons/Lektion zwei.html",
    );
  });

  it("cannot be made to escape above the workspace root with excess ../ segments", () => {
    // `URL`'s own relative-resolution algorithm clamps a `..` that would
    // walk past the root, rather than producing a negative/escaped path —
    // e.g. four levels of "../" from a two-level-deep file still lands on
    // "escape.html" at the root, never something like "../../escape.html".
    // This is what makes the result safe to hand to `openFilePanel`/the file
    // service unchecked: the file service's own root containment then only
    // has to refuse an absolute-looking path, never undo an already-escaped
    // relative one.
    expect(resolveRelativeLink("Learning/Rust/lessons/0002-x.html", "../../../../escape.html")).toBe(
      "escape.html",
    );
    expect(resolveRelativeLink("a.html", "../../escape.html")).toBe("escape.html");
  });
});
