import { describe, expect, it } from "vitest";

import { detectDelimiter, parseDelimited, toDelimited } from "./csv";

describe("csv", () => {
  it("parses quoted fields with commas and quotes", () => {
    const rows = parseDelimited('name,host\n"A, B",x\n"he said ""hi""",y', ",");
    expect(rows).toEqual([
      ["name", "host"],
      ["A, B", "x"],
      ['he said "hi"', "y"],
    ]);
  });

  it("detects tab delimiter", () => {
    expect(detectDelimiter("a\tb\nc\td")).toBe("\t");
    expect(detectDelimiter("a,b\nc,d")).toBe(",");
  });

  it("round-trips via serialization", () => {
    const original = [
      ["name", "host"],
      ["CORE, JKT", "10.0.0.1"],
    ];
    const text = toDelimited(original, ",");
    expect(parseDelimited(text, ",")).toEqual(original);
  });

  it("ignores empty trailing rows", () => {
    expect(parseDelimited("a,b\n\n", ",")).toEqual([["a", "b"]]);
  });
});
