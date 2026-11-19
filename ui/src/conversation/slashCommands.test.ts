import { expect, test } from "vitest";
import { BUILTINS, matchCommands, slashQuery } from "./slashCommands";

const names = (q: string, skills: { name: string; description: string }[] = []) =>
  matchCommands(q, skills).map((e) => e.name);

test("slashQuery: the typed name while only a command name is being typed", () => {
  expect(slashQuery("/")).toBe("");
  expect(slashQuery("/co")).toBe("co");
  expect(slashQuery("/co ")).toBeNull();
  expect(slashQuery("/co\n")).toBeNull();
  expect(slashQuery("x/co")).toBeNull();
  expect(slashQuery("")).toBeNull();
});

test("prefix matches, built-ins in name order", () => {
  expect(names("co").slice(0, 4)).toEqual(["compact", "config", "context", "cost"]);
});

test("no prefix match: names containing the text", () => {
  expect(names("pact")).toEqual(["compact"]);
});

test("a skill named like a built-in comes after it", () => {
  const got = matchCommands("review", [{ name: "review", description: "skill" }]);
  expect(got.map((e) => [e.name, e.builtin])).toEqual([["review", true], ["review", false]]);
});

test("nothing matches: empty", () => {
  expect(names("zzz")).toEqual([]);
});

test("skills keep their namespace and match on it", () => {
  expect(names("cave", [{ name: "caveman:caveman-help", description: "Card." }])).toEqual(["caveman:caveman-help"]);
});

test("an empty query lists everything, built-ins first", () => {
  const got = matchCommands("", [{ name: "aaa", description: "" }]);
  expect(got.length).toBe(BUILTINS.length + 1);
  expect(got[got.length - 1].name).toBe("aaa");
});

test("picker commands are marked for the terminal", () => {
  const by = (n: string) => BUILTINS.find((b) => b.name === n)!;
  expect(by("model").terminal).toBe(true);
  expect(by("compact").terminal).toBe(false);
  expect(by("clear").terminal).toBe(false);
});
