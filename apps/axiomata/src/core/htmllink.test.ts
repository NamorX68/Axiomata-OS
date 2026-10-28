import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  PAGE_MESSAGE_SOURCE,
  ZOOM_MESSAGE_SOURCE,
  isOpenableExternal,
  pageScriptNonce,
  resolveRelativeLink,
  withNavIntercept,
} from "./htmllink";

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
    const scriptTag = withNavIntercept("<body></body>").match(
      /<script>([\s\S]*)<\/script>/,
    )?.[1];
    if (!scriptTag)
      throw new Error("withNavIntercept produced no <script> tag");
    new Function(scriptTag)();
  }

  beforeEach(() => {
    document.body.innerHTML = "";
    install();
  });

  it("scrolls to the target element for a same-page anchor, without asking the parent", () => {
    document.body.innerHTML =
      '<a id="link" href="#etappe-a">go</a><section id="etappe-a"></section>';
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
    expect(post).toHaveBeenCalledWith(
      { source: PAGE_MESSAGE_SOURCE, href: "0003-next.html" },
      "*",
    );
  });

  it("hands web and mail links to the parent as external, to open in the browser", () => {
    document.body.innerHTML =
      '<a id="web" href="https://example.com">ext</a><a id="mail" href="mailto:a@b.c">m</a>';
    const post = vi.fn();
    window.parent.postMessage = post;

    document.getElementById("web")!.click();
    document.getElementById("mail")!.click();

    expect(post).toHaveBeenCalledWith(
      {
        source: PAGE_MESSAGE_SOURCE,
        href: "https://example.com",
        external: true,
      },
      "*",
    );
    expect(post).toHaveBeenCalledWith(
      { source: PAGE_MESSAGE_SOURCE, href: "mailto:a@b.c", external: true },
      "*",
    );
  });

  it("leaves other scheme'd links (javascript:, file:) alone", () => {
    document.body.innerHTML =
      '<a id="js" href="javascript:void 0">x</a><a id="file" href="file:///etc/hosts">f</a>';
    const post = vi.fn();
    window.parent.postMessage = post;

    document.getElementById("js")!.click();
    document.getElementById("file")!.click();

    expect(post).not.toHaveBeenCalled();
  });

  // In jsdom the test window is its own parent, so a message "from the parent" has `window` as its source.
  function zoomMessage(
    data: unknown,
    source: MessageEventSource | null = window,
  ): void {
    window.dispatchEvent(new MessageEvent("message", { data, source }));
  }

  it("scales the page when the parent posts a zoom, and lets go of it again", () => {
    zoomMessage({ source: ZOOM_MESSAGE_SOURCE, zoom: 1.5 });
    expect(document.documentElement.style.zoom).toBe("1.5");
    zoomMessage({ source: ZOOM_MESSAGE_SOURCE, zoom: 1 });
    expect(document.documentElement.style.zoom).toBe("1");
  });

  it("ignores a zoom that is not from the parent, is not a positive number, or has another source", () => {
    document.documentElement.style.zoom = "";
    zoomMessage({ source: ZOOM_MESSAGE_SOURCE, zoom: 2 }, null);
    zoomMessage({ source: ZOOM_MESSAGE_SOURCE, zoom: "2" });
    zoomMessage({ source: ZOOM_MESSAGE_SOURCE, zoom: 0 });
    zoomMessage({ source: ZOOM_MESSAGE_SOURCE, zoom: -1 });
    zoomMessage({ source: "something-else", zoom: 2 });
    zoomMessage(null);
    expect(document.documentElement.style.zoom).toBe("");
  });
});

describe("resolveRelativeLink", () => {
  it("refuses a link that still climbs out after decoding (..%2f), instead of handing on ../", () => {
    expect(
      resolveRelativeLink(
        "Learning/Rust/lessons/0002-x.html",
        "..%2f..%2f..%2fetc%2fpasswd",
      ),
    ).toBeNull();
    expect(resolveRelativeLink("a/b.html", "%2e%2e%2fsecret.md")).toBeNull();
    // A malformed escape names no file either.
    expect(resolveRelativeLink("a/b.html", "%E0%A4%A")).toBeNull();
    // An encoded name that stays inside is fine.
    expect(resolveRelativeLink("a/b.html", "c%2fd.html")).toBe("a/c/d.html");
  });

  it("resolves a same-folder link against the current file's folder", () => {
    expect(
      resolveRelativeLink(
        "Learning/Rust/lessons/0002-variablen.html",
        "0003-funktionen.html",
      ),
    ).toBe("Learning/Rust/lessons/0003-funktionen.html");
  });

  it("resolves ../ the same way a real browser would", () => {
    expect(
      resolveRelativeLink(
        "Learning/Rust/lessons/0002-variablen.html",
        "../roadmap.html",
      ),
    ).toBe("Learning/Rust/roadmap.html");
  });

  it("resolves a link from a file with no folder of its own", () => {
    expect(resolveRelativeLink("index.html", "about.html")).toBe("about.html");
  });

  it("decodes percent-escaped characters in the target", () => {
    expect(
      resolveRelativeLink("Learning/lessons/a.html", "Lektion%20zwei.html"),
    ).toBe("Learning/lessons/Lektion zwei.html");
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
    expect(
      resolveRelativeLink(
        "Learning/Rust/lessons/0002-x.html",
        "../../../../escape.html",
      ),
    ).toBe("escape.html");
    expect(resolveRelativeLink("a.html", "../../escape.html")).toBe(
      "escape.html",
    );
  });
});

describe("withNavIntercept with the app's nonce (the CSP)", () => {
  it("gives every inline script of the page and the intercept the nonce, never a script loaded by src", () => {
    const out = withNavIntercept(
      '<body><script>quiz()</script><script src="https://cdn.x/y.js"></script></body>',
      "123",
    );
    expect(out.match(/nonce="123"/g)).toHaveLength(2);
    expect(out).toContain('<script src="https://cdn.x/y.js"></script>');
  });

  it("inserts the nonce as it is, without replacement patterns", () => {
    expect(
      withNavIntercept("<body><script>a()</script></body>", "12"),
    ).toContain('<script nonce="12">a()');
  });

  it("keeps a script's own nonce", () => {
    const out = withNavIntercept(
      '<body><script nonce="own">x()</script></body>',
      "123",
    );
    expect(out).toContain('<script nonce="own">');
    expect(out).not.toContain('nonce="123" nonce');
  });

  it("leaves the page as it was without a nonce", () => {
    expect(withNavIntercept("<body><script>a()</script></body>")).toContain(
      "<script>a()</script>",
    );
  });
});

describe("pageScriptNonce", () => {
  function docWith(html: string): Document {
    return new DOMParser().parseFromString(html, "text/html");
  }

  it("reads the nonce Tauri filled in", () => {
    expect(pageScriptNonce(docWith('<script nonce="987654"></script>'))).toBe(
      "987654",
    );
  });

  it("takes only a number, never something that could break out of an attribute", () => {
    expect(
      pageScriptNonce(
        docWith('<script nonce="1&quot; onload=&quot;x"></script>'),
      ),
    ).toBeNull();
  });

  it("has none where nobody filled the placeholder, or there is no such script", () => {
    expect(
      pageScriptNonce(
        docWith('<script nonce="__TAURI_SCRIPT_NONCE__"></script>'),
      ),
    ).toBeNull();
    expect(pageScriptNonce(docWith("<p>no script</p>"))).toBeNull();
  });
});

describe("isOpenableExternal", () => {
  it("opens web and mail links only", () => {
    expect(isOpenableExternal("https://doc.rust-lang.org/book/")).toBe(true);
    expect(isOpenableExternal("http://example.com")).toBe(true);
    expect(isOpenableExternal("mailto:a@b.c")).toBe(true);
    expect(isOpenableExternal("javascript:alert(1)")).toBe(false);
    expect(isOpenableExternal("file:///etc/hosts")).toBe(false);
    expect(isOpenableExternal("tauri://localhost")).toBe(false);
  });

  it("never opens a service on this Mac", () => {
    for (const href of [
      "http://localhost:11434/api",
      "http://127.0.0.1:4323/",
      "http://[::1]/",
      "http://a.localhost/",
    ]) {
      expect(isOpenableExternal(href), href).toBe(false);
    }
  });
});
