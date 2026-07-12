import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { App } from "./App";

describe("M0 shell", () => {
  it("does not claim that the walking slice is implemented", () => {
    const markup = renderToStaticMarkup(<App />);
    expect(markup).toContain("M0 contract substrate");
    expect(markup).toContain("Product behavior begins");
  });
});
