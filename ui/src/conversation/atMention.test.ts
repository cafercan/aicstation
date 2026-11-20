import { expect, test } from "vitest";
import { atQuery, completeAt } from "./atMention";

test("the @ word at the caret is the query", () => {
  expect(atQuery("bak @gi", 7)).toEqual({ start: 4, end: 7, query: "gi", quoted: false });
  expect(atQuery("@", 1)).toEqual({ start: 0, end: 1, query: "", quoted: false });
  expect(atQuery("x\n@src/ma", 9)).toEqual({ start: 2, end: 9, query: "src/ma", quoted: false });
  // Caret inside the word: the query is up to the caret, the word to its end.
  expect(atQuery("@readme sonra", 3)).toEqual({ start: 0, end: 7, query: "re", quoted: false });
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

test("a quoted @ word goes on over spaces", () => {
  expect(atQuery('bak @"rapor son', 15)).toEqual({ start: 4, end: 15, query: "rapor son", quoted: true });
  // The closing quote belongs to the word.
  expect(atQuery('@"a b" sonra', 4)).toEqual({ start: 0, end: 6, query: "a ", quoted: true });
  // A closed quote before the caret ends the word.
  expect(atQuery('@"a b" so', 9)).toBeNull();
  expect(atQuery('@"a\nb', 5)).toBeNull();
});

test("a folder with spaces stays open inside its quote", () => {
  const at = atQuery('@"bos', 5)!;
  expect(completeAt('@"bos', at, { path: "bosluklu klasor", dir: true })).toEqual({ text: '@"bosluklu klasor/', caret: 18 });
  const inner = atQuery('@"bosluklu klasor/ic', 20)!;
  expect(inner.query).toBe("bosluklu klasor/ic");
  expect(completeAt('@"bosluklu klasor/ic', inner, { path: "bosluklu klasor/ic dosya.txt", dir: false }))
    .toEqual({ text: '@"bosluklu klasor/ic dosya.txt" ', caret: 32 });
  // A plain file picked in a quoted word keeps the quotes.
  expect(completeAt('@"gi', atQuery('@"gi', 4)!, { path: "gizli.txt", dir: false })).toEqual({ text: '@"gizli.txt" ', caret: 13 });
});
