import { beforeEach, describe, expect, it, vi } from "vitest";

import { resolveRelativeLink, withNavIntercept } from "./htmllink";

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

  it("posts same-folder links to the parent instead of navigating", () => {
    document.body.innerHTML = '<a id="link" href="0003-next.html">next</a>';
    const post = vi.fn();
    window.parent.postMessage = post;

    document.getElementById("link")!.click();

    expect(post).toHaveBeenCalledWith({ source: "ax-md-file", href: "0003-next.html" }, "*");
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
});
