/// <reference types="node" />
import assert from "node:assert/strict";
import test from "node:test";
import {
  emptySelection,
  moveSelection,
  removeSelection,
  selectRow,
} from "./selection.ts";

const paths = ["a", "b", "c", "d", "e"];
test("click replaces selection; modifier toggles without changing other rows", () => {
  let state = selectRow(emptySelection(), paths, "b");
  state = selectRow(state, paths, "d", false, true);
  assert.deepEqual([...state.selected], ["b", "d"]);
  state = selectRow(state, paths, "b", false, true);
  assert.deepEqual([...state.selected], ["d"]);
  assert.deepEqual([...selectRow(state, paths, "a").selected], ["a"]);
});
test("shift range grows and shrinks around a stable anchor in either direction", () => {
  let state = selectRow(emptySelection(), paths, "c");
  state = selectRow(state, paths, "e", true);
  assert.deepEqual([...state.selected], ["c", "d", "e"]);
  state = moveSelection(state, paths, -1, true);
  assert.deepEqual([...state.selected], ["c", "d"]);
  state = selectRow(state, paths, "a", true);
  assert.deepEqual([...state.selected], ["a", "b", "c"]);
  assert.equal(state.anchor, "c");
});
test("arrow navigation handles no focus, edges, and empty lists", () => {
  const first = moveSelection(emptySelection(), paths, 1);
  assert.equal(first.focus, "a");
  assert.equal(moveSelection(first, paths, -1).focus, "a");
  const last = moveSelection(emptySelection(), paths, -1);
  assert.equal(last.focus, "e");
  assert.equal(moveSelection(last, paths, 1).focus, "e");
  assert.deepEqual(moveSelection(first, [], 1), emptySelection());
});
test("removal focuses nearest survivor and clears selection when all are removed", () => {
  let state = selectRow(emptySelection(), paths, "b");
  state = selectRow(state, paths, "d", true);
  assert.equal(removeSelection(state, paths).focus, "e");
  state = selectRow(emptySelection(), paths, "e");
  assert.equal(removeSelection(state, paths).focus, "d");
  assert.deepEqual(
    removeSelection({ ...state, selected: new Set(paths) }, paths),
    emptySelection(),
  );
});
