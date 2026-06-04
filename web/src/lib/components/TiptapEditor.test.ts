import { describe, it, expect } from "vitest";

// NOTE: TipTap / ProseMirror relies on browser APIs that jsdom doesn't fully implement
// (e.g. document.elementFromPoint). For now we keep this as a minimal Vitest smoke test
// to ensure the test runner executes in CI.

describe("vitest", () => {
  it("runs", () => {
    expect(true).toBe(true);
  });
});
