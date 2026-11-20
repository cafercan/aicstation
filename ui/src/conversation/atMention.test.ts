import { expect, test } from "vitest";
import { atQuery, completeAt } from "./atMention";

test("the @ word at the caret is the query", () => {
  expect(atQuery("bak @gi", 7)).toEqual({ start: 4, end: 7, query: "gi" });
  expect(atQuery("@", 1)).toEqual({ start: 0, end: 1, query: "" });
  expect(atQuery("x\n@src/ma", 9)).toEqual({ start: 2, end: 9, query: "src/ma" });
  // Caret inside the word: the query is up to the caret, the word to its end.
  expect(atQuery("@readme sonra", 3)).toEqual({ start: 0, end: 7, query: "re" });
});

test("no query outside an @ word", () => {
  expect(atQuery("bak @gizli.txt ", 15)).toBeNull();
  expect(atQuery("mail a@b.com", 12)).toBeNull();
  expect(atQuery("bak", 3)).toBeNull();
  expect(atQuery("@gi", 0)).toBeNull();
});

test("a file completes with a space, a folder with a slash", () => {
  const at = atQuery("bak @gi", 7)!;
  expect(completeAt("bak @gi", at, { path: "gizli.txt", dir: false })).toEqual({ text: "bak @gizli.txt ", caret: 15 });
  expect(completeAt("bak @gi", at, { path: "alt", dir: true })).toEqual({ text: "bak @alt/", caret: 9 });
  const mid = atQuery("@re sonra", 2)!;
  expect(completeAt("@re sonra", mid, { path: "README.md", dir: false })).toEqual({ text: "@README.md sonra", caret: 11 });
});

test("a path with spaces is quoted", () => {
  const at = atQuery("@my", 3)!;
  expect(completeAt("@my", at, { path: "my docs/a b.txt", dir: false })).toEqual({ text: '@"my docs/a b.txt" ', caret: 19 });
});
