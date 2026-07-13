import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { App } from "./App";

describe("M1 production shell", () => {
  it("renders the complete generated-client walking-slice path", () => {
    const markup = renderToStaticMarkup(<App />);
    expect(markup).toContain("Human session");
    expect(markup).toContain("Exploration contract");
    expect(markup).toContain("Immutable activity");
    expect(markup).toContain("generated public client");
  });
});
