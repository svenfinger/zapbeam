/// <reference types="node" />
import assert from "node:assert/strict";
import test from "node:test";
import { formatSize, type ImageFile, mergeFiles } from "./files.ts";

const file: ImageFile = {
  path: "/images/a.png",
  name: "a.png",
  format: "PNG",
  size: 1234,
};
test("formats decimal sizes, boundaries and invalid input", () => {
  for (const [bytes, expected] of [
    [0, "0 B"],
    [999, "999 B"],
    [1000, "1 kB"],
    [1234, "1.2 kB"],
    [1000000, "1 MB"],
    [1000000000, "1 GB"],
    [-1, "—"],
    [NaN, "—"],
  ] as const)
    assert.equal(formatSize(bytes), expected);
});
test("deduplicates both existing files and a single incoming batch", () => {
  const next = { ...file, path: "/images/b.png" };
  const result = mergeFiles(
    [file],
    [
      { kind: "image", file },
      { kind: "image", file: next },
      { kind: "image", file: next },
    ],
  );
  assert.deepEqual(result.files, [file, next]);
  assert.equal(result.duplicates, 2);
  assert.equal(result.added, 1);
});
test("retains valid files alongside inspection failures without mutating current queue", () => {
  const current: ImageFile[] = [];
  const result = mergeFiles(current, [
    { kind: "rejected", name: "notes.txt", message: "Not a supported image." },
    { kind: "image", file },
  ]);
  assert.deepEqual(result.issues, ["notes.txt: Not a supported image."]);
  assert.deepEqual(result.files, [file]);
  assert.deepEqual(current, []);
  assert.equal(mergeFiles([], [{ kind: "image", file }]).added, 1);
});
